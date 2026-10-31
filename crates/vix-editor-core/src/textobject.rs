//! Tree-sitter textobjects (T616): select the function, class or parameter
//! around a position, and find the next or previous one.
//!
//! Each language with a grammar may bundle `langs/<lang>/textobjects.scm`,
//! whose captures name the object kind and its extent: `@function.around`
//! (the whole definition) and `@function.inside` (its body), likewise
//! `@class.*` and `@parameter.*`. See
//! `crates/vix-editor-core/spec/textobjects/index.md`.
//!
//! Every query runs over a narrow byte window of the existing parse tree, so
//! the cost does not grow with the buffer; with no tree yet (a large buffer
//! whose first parse is still running in the background) every lookup
//! returns `None`.

use crate::code::{Code, LangAssets, RopeProvider, compiled_query};
use std::rc::Rc;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Query, QueryCursor};

/// What a textobject selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TextObjectKind {
    /// A function, method, closure or lambda (`@function.*`).
    Function,
    /// A class, struct, enum, trait, interface or impl block (`@class.*`).
    Class,
    /// One parameter or argument in a list (`@parameter.*`).
    Parameter,
}

impl TextObjectKind {
    /// The capture-name prefix this kind uses in `textobjects.scm`.
    #[must_use]
    pub const fn capture_prefix(self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Class => "class",
            Self::Parameter => "parameter",
        }
    }
}

/// Which way [`Code::next_textobject`] searches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextObjectDir {
    /// Toward the end of the buffer.
    Next,
    /// Toward the start of the buffer.
    Previous,
}

/// One textobject found in the buffer, as character ranges (`start..end`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextObject {
    /// The whole object, e.g. a function's signature and body.
    pub around: (usize, usize),
    /// Its interior, e.g. the statements of the body without its braces.
    pub inside: (usize, usize),
}

/// A query match in bytes: the object's whole range and, when the query
/// names one, its inside.
type RawMatch = ((usize, usize), Option<(usize, usize)>);

/// The first window [`Code::next_textobject`] scans, in bytes; it doubles
/// until a match turns up or the buffer edge is reached.
const FIRST_WINDOW: usize = 16 * 1024;

impl Code {
    /// The compiled textobjects query for this buffer's language, if it has a
    /// grammar and bundles a `textobjects.scm` that compiles.
    fn textobject_query(&self) -> Option<Rc<Query>> {
        let lang = self.lang();
        let language = Self::get_language(lang)?;
        let asset = LangAssets::get(&format!("{lang}/textobjects.scm"))?;
        let source = std::str::from_utf8(asset.data.as_ref()).ok()?;
        compiled_query(lang, &language, source).ok()
    }

    /// Whether this buffer's language has textobjects at all (a grammar plus a
    /// bundled query). Says nothing about whether the tree is parsed yet.
    #[must_use]
    pub fn has_textobjects(&self) -> bool {
        self.textobject_query().is_some()
    }

    /// Every `kind` textobject with a node intersecting the byte window
    /// `start..end`, as byte ranges `(around, inside)`, in no set order.
    fn textobjects_in(&self, kind: TextObjectKind, start: usize, end: usize) -> Vec<RawMatch> {
        let (Some(tree), Some(query)) = (self.syntax_tree(), self.textobject_query()) else {
            return Vec::new();
        };
        let prefix = kind.capture_prefix();
        let names = query.capture_names();
        let mut cursor = QueryCursor::new();
        cursor.set_byte_range(start..end);
        let mut matches = cursor.matches(
            &query,
            tree.root_node(),
            RopeProvider(self.rope().slice(..)),
        );
        let mut out = Vec::new();
        while let Some(m) = matches.next() {
            let mut around: Option<(usize, usize)> = None;
            let mut inside: Option<(usize, usize)> = None;
            for capture in m.captures() {
                let name = names[capture.index as usize];
                let Some(part) = name.strip_prefix(prefix).and_then(|n| n.strip_prefix('.')) else {
                    continue;
                };
                let range = (capture.node.start_byte(), capture.node.end_byte());
                let slot = match part {
                    "around" => &mut around,
                    "inside" => &mut inside,
                    _ => continue,
                };
                // A quantified capture (`(_)+ @class.inside`) yields several
                // nodes in one match: the object spans all of them.
                *slot = Some(slot.map_or(range, |(s, e)| (s.min(range.0), e.max(range.1))));
            }
            match (around, inside) {
                (Some(a), i) => out.push((a, i)),
                (None, Some(i)) => out.push((i, None)),
                (None, None) => {}
            }
        }
        out
    }

    /// Turn a byte-range match into a [`TextObject`] of character ranges,
    /// deriving whatever the query left out (see the spec's "Derived
    /// ranges").
    fn finish_textobject(
        &self,
        kind: TextObjectKind,
        around: (usize, usize),
        inside: Option<(usize, usize)>,
    ) -> TextObject {
        let chars = |(s, e): (usize, usize)| (self.byte_to_char(s), self.byte_to_char(e));
        let around_c = chars(around);
        if kind == TextObjectKind::Parameter && inside.is_none() {
            // A parameter query names only the parameter itself; "around"
            // adds the separating comma, as Vim's `aa` does.
            return TextObject {
                around: self.parameter_with_separator(around_c),
                inside: around_c,
            };
        }
        let inside_c = inside.map_or(around_c, |i| self.trim_braces(chars(i)));
        TextObject {
            around: around_c,
            inside: inside_c,
        }
    }

    /// `range` widened by the comma that separates it from its neighbor: the
    /// following comma and the blanks after it, or, for the last item, the
    /// preceding comma and the blanks before the item.
    fn parameter_with_separator(&self, (start, end): (usize, usize)) -> (usize, usize) {
        let rope = self.rope();
        let len = rope.len_chars();
        let is_blank = |c: char| c == ' ' || c == '\t';
        let mut e = end;
        while e < len && is_blank(rope.char(e)) {
            e += 1;
        }
        if e < len && rope.char(e) == ',' {
            e += 1;
            while e < len && rope.char(e).is_whitespace() {
                e += 1;
            }
            return (start, e);
        }
        let mut s = start;
        while s > 0 && rope.char(s - 1).is_whitespace() {
            s -= 1;
        }
        if s > 0 && rope.char(s - 1) == ',' {
            return (s - 1, end);
        }
        (start, end)
    }

    /// A body in braces (`{ … }`) minus the braces and the blank space just
    /// inside them, so "inside" means the statements; an empty body gives
    /// the empty range just after `{`. Anything else is returned unchanged.
    fn trim_braces(&self, (start, end): (usize, usize)) -> (usize, usize) {
        let rope = self.rope();
        if end < start + 2 || rope.char(start) != '{' || rope.char(end - 1) != '}' {
            return (start, end);
        }
        let mut s = start + 1;
        let mut e = end - 1;
        while s < e && rope.char(s).is_whitespace() {
            s += 1;
        }
        while e > s && rope.char(e - 1).is_whitespace() {
            e -= 1;
        }
        if s == e {
            (start + 1, start + 1)
        } else {
            (s, e)
        }
    }

    /// Every `kind` textobject whose whole ("around") range contains the
    /// character `pos`, innermost first. Empty without a parse tree or a
    /// textobjects query for the language.
    #[must_use]
    pub fn enclosing_textobjects(&self, kind: TextObjectKind, pos: usize) -> Vec<TextObject> {
        let len = self.len_chars();
        if pos > len {
            return Vec::new();
        }
        let byte = self.char_to_byte(pos);
        let total = self.len_bytes();
        let mut found: Vec<TextObject> = self
            .textobjects_in(kind, byte, (byte + 1).min(total).max(byte))
            .into_iter()
            .map(|(a, i)| self.finish_textobject(kind, a, i))
            .filter(|o| o.around.0 <= pos && pos < o.around.1)
            .collect();
        found.sort_by_key(|o| (o.around.1 - o.around.0, o.around.0));
        found.dedup();
        found
    }

    /// The innermost `kind` textobject containing the character `pos`: its
    /// interior when `inside`, else the whole object. `None` when there is
    /// none, no parse tree yet, or no textobjects query for the language.
    #[must_use]
    pub fn textobject_range(
        &self,
        kind: TextObjectKind,
        inside: bool,
        pos: usize,
    ) -> Option<(usize, usize)> {
        let obj = *self.enclosing_textobjects(kind, pos).first()?;
        Some(if inside { obj.inside } else { obj.around })
    }

    /// The start (a character offset) of the nearest `kind` textobject that
    /// begins after (`Next`) or before (`Previous`) the character `pos`,
    /// wrapping around the buffer's end when there is none that way. `None`
    /// when the buffer has no such object, no parse tree yet, or no
    /// textobjects query for its language.
    ///
    /// Scans a window around `pos` that doubles until it finds a candidate,
    /// so the usual case reads only nearby nodes even in a large buffer.
    #[must_use]
    pub fn next_textobject(
        &self,
        kind: TextObjectKind,
        dir: TextObjectDir,
        pos: usize,
    ) -> Option<usize> {
        self.syntax_tree()?;
        self.textobject_query()?;
        let total = self.len_bytes();
        let byte = self.char_to_byte(pos.min(self.len_chars()));
        let found = match dir {
            TextObjectDir::Next => self
                .nearest_start(kind, byte, total, |s| s > byte, true)
                .or_else(|| self.nearest_start(kind, 0, total, |_| true, true)),
            TextObjectDir::Previous => self
                .nearest_start(kind, 0, byte, |s| s < byte, false)
                .or_else(|| self.nearest_start(kind, 0, total, |_| true, false)),
        }?;
        Some(self.byte_to_char(found))
    }

    /// The smallest (`forward`) or largest start byte, among `kind` objects
    /// whose start passes `accept`, searching from the `forward` edge of
    /// `lo..hi` in doubling windows.
    fn nearest_start(
        &self,
        kind: TextObjectKind,
        lo: usize,
        hi: usize,
        accept: impl Fn(usize) -> bool,
        forward: bool,
    ) -> Option<usize> {
        let mut width = FIRST_WINDOW;
        loop {
            let (ws, we) = if forward {
                (lo, lo.saturating_add(width).min(hi))
            } else {
                (hi.saturating_sub(width).max(lo), hi)
            };
            let starts = self
                .textobjects_in(kind, ws, we.max(ws))
                .into_iter()
                .map(|(a, _)| a.0)
                .filter(|&s| accept(s) && s >= ws && s <= we);
            let best = if forward { starts.min() } else { starts.max() };
            if best.is_some() {
                return best;
            }
            if ws == lo && we == hi {
                return None;
            }
            width = width.saturating_mul(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{TextObjectDir, TextObjectKind};
    use crate::code::Code;

    /// The text of `range` in `code`.
    fn text(code: &Code, range: Option<(usize, usize)>) -> String {
        let (s, e) = range.expect("a textobject");
        code.slice(s, e)
    }

    fn code(src: &str, lang: &str) -> Code {
        Code::new(src, lang, None).expect("code")
    }

    #[test]
    fn no_grammar_means_no_textobjects() {
        let c = code("fn main() {}\n", "text");
        assert!(!c.has_textobjects());
        assert_eq!(c.textobject_range(TextObjectKind::Function, false, 3), None);
        assert_eq!(
            c.next_textobject(TextObjectKind::Function, TextObjectDir::Next, 0),
            None
        );
    }

    #[cfg(feature = "lang-rust")]
    mod rust {
        use super::{TextObjectDir, TextObjectKind, code, text};

        const SRC: &str = "struct Point {\n    x: i32,\n}\n\nfn add(a: i32, b: i32) -> i32 {\n    let s = a + b;\n    s\n}\n\nimpl Point {\n    fn norm(&self) -> i32 {\n        self.x\n    }\n}\n";

        #[test]
        fn function_inside_and_around() {
            let c = code(SRC, "rust");
            let pos = SRC.find("let s").unwrap();
            assert_eq!(
                text(&c, c.textobject_range(TextObjectKind::Function, false, pos)),
                "fn add(a: i32, b: i32) -> i32 {\n    let s = a + b;\n    s\n}"
            );
            assert_eq!(
                text(&c, c.textobject_range(TextObjectKind::Function, true, pos)),
                "let s = a + b;\n    s"
            );
        }

        #[test]
        fn class_inside_and_around() {
            let c = code(SRC, "rust");
            let pos = SRC.find("x: i32").unwrap();
            assert_eq!(
                text(&c, c.textobject_range(TextObjectKind::Class, false, pos)),
                "struct Point {\n    x: i32,\n}"
            );
            assert_eq!(
                text(&c, c.textobject_range(TextObjectKind::Class, true, pos)),
                "x: i32,"
            );
        }

        #[test]
        fn nested_objects_are_innermost_first() {
            let c = code(SRC, "rust");
            let pos = SRC.find("self.x").unwrap();
            let funcs = c.enclosing_textobjects(TextObjectKind::Function, pos);
            assert_eq!(funcs.len(), 1);
            let classes = c.enclosing_textobjects(TextObjectKind::Class, pos);
            assert_eq!(classes.len(), 1);
            assert_eq!(
                c.slice(classes[0].around.0, classes[0].around.0 + 10),
                "impl Point"
            );
        }

        #[test]
        fn parameter_around_takes_the_comma() {
            let c = code(SRC, "rust");
            let a = SRC.find("a: i32").unwrap();
            assert_eq!(
                text(&c, c.textobject_range(TextObjectKind::Parameter, true, a)),
                "a: i32"
            );
            assert_eq!(
                text(&c, c.textobject_range(TextObjectKind::Parameter, false, a)),
                "a: i32, "
            );
            // The last parameter has no comma after it: take the one before.
            let b = SRC.find("b: i32").unwrap();
            assert_eq!(
                text(&c, c.textobject_range(TextObjectKind::Parameter, false, b)),
                ", b: i32"
            );
        }

        #[test]
        fn next_and_previous_function_wrap() {
            let c = code(SRC, "rust");
            let add = SRC.find("fn add").unwrap();
            let norm = SRC.find("fn norm").unwrap();
            let next = |p| c.next_textobject(TextObjectKind::Function, TextObjectDir::Next, p);
            let prev = |p| c.next_textobject(TextObjectKind::Function, TextObjectDir::Previous, p);
            assert_eq!(next(0), Some(add));
            assert_eq!(next(add), Some(norm));
            assert_eq!(next(norm), Some(add), "wraps past the end");
            assert_eq!(prev(norm), Some(add));
            assert_eq!(prev(add), Some(norm), "wraps past the start");
        }

        #[test]
        fn next_class() {
            let c = code(SRC, "rust");
            let imp = SRC.find("impl Point").unwrap();
            assert_eq!(
                c.next_textobject(TextObjectKind::Class, TextObjectDir::Next, 1),
                Some(imp)
            );
        }

        #[test]
        fn outside_any_function_finds_none() {
            let c = code(SRC, "rust");
            assert_eq!(c.textobject_range(TextObjectKind::Function, false, 0), None);
        }

        #[test]
        fn empty_body_inside_is_empty() {
            let src = "fn f() {}\n";
            let c = code(src, "rust");
            let r = c
                .textobject_range(TextObjectKind::Function, true, 2)
                .unwrap();
            assert_eq!(r, (8, 8));
        }
    }

    #[cfg(feature = "lang-python")]
    #[test]
    fn python_function_and_class() {
        let src = "class Greeter:\n    def hello(self, name):\n        return name\n\ndef top(x):\n    return x\n";
        let c = code(src, "python");
        let pos = src.find("return name").unwrap();
        assert_eq!(
            text(&c, c.textobject_range(TextObjectKind::Function, false, pos)),
            "def hello(self, name):\n        return name"
        );
        assert_eq!(
            text(&c, c.textobject_range(TextObjectKind::Function, true, pos)),
            "return name"
        );
        assert!(
            text(&c, c.textobject_range(TextObjectKind::Class, false, pos))
                .starts_with("class Greeter:")
        );
        assert_eq!(
            text(
                &c,
                c.textobject_range(TextObjectKind::Parameter, false, src.find("self").unwrap())
            ),
            "self, "
        );
        let hello = src.find("def hello").unwrap();
        let top = src.find("def top").unwrap();
        assert_eq!(
            c.next_textobject(TextObjectKind::Function, TextObjectDir::Next, hello),
            Some(top)
        );
    }

    #[cfg(feature = "lang-javascript")]
    #[test]
    fn javascript_function_and_class() {
        let src = "class A {\n  run(a, b) {\n    return a;\n  }\n}\nconst f = (x) => x * 2;\n";
        let c = code(src, "javascript");
        let pos = src.find("return a").unwrap();
        assert_eq!(
            text(&c, c.textobject_range(TextObjectKind::Function, false, pos)),
            "run(a, b) {\n    return a;\n  }"
        );
        assert_eq!(
            text(&c, c.textobject_range(TextObjectKind::Function, true, pos)),
            "return a;"
        );
        assert_eq!(
            text(&c, c.textobject_range(TextObjectKind::Class, true, pos)),
            "run(a, b) {\n    return a;\n  }"
        );
        let arrow = src.find("(x) =>").unwrap();
        assert_eq!(
            c.next_textobject(TextObjectKind::Function, TextObjectDir::Next, pos),
            Some(arrow)
        );
    }

    #[cfg(feature = "lang-typescript")]
    #[test]
    fn typescript_function_and_interface() {
        let src = "interface Shape {\n  area(): number;\n}\nfunction area(s: Shape, k: number): number {\n  return k;\n}\n";
        let c = code(src, "typescript");
        assert_eq!(
            text(
                &c,
                c.textobject_range(TextObjectKind::Class, false, src.find("area()").unwrap())
            ),
            "interface Shape {\n  area(): number;\n}"
        );
        let pos = src.find("return k").unwrap();
        assert_eq!(
            text(&c, c.textobject_range(TextObjectKind::Function, true, pos)),
            "return k;"
        );
        assert_eq!(
            text(
                &c,
                c.textobject_range(
                    TextObjectKind::Parameter,
                    true,
                    src.find("k: number").unwrap()
                )
            ),
            "k: number"
        );
    }

    /// A per-language fixture: `(lang, compiled in, source, marker, kind,
    /// expected prefix)`.
    type Fixture = (
        &'static str,
        bool,
        &'static str,
        &'static str,
        TextObjectKind,
        &'static str,
    );

    /// One fixture per remaining bundled language: the object of `kind`
    /// around the `marker` text starts with `expected`.
    const FIXTURES: &[Fixture] = {
        use TextObjectKind::{Class, Function, Parameter};
        &[
            (
                "go",
                cfg!(feature = "lang-go"),
                "package m\n\ntype T struct {\n\tA int\n}\n\nfunc (t T) Get(k int) int {\n\treturn k\n}\n",
                "return k",
                Function,
                "func (t T) Get",
            ),
            (
                "go",
                cfg!(feature = "lang-go"),
                "package m\n\ntype T struct {\n\tA int\n}\n",
                "A int",
                Class,
                "type T struct",
            ),
            (
                "java",
                cfg!(feature = "lang-java"),
                "class A {\n  int f(int x) {\n    return x;\n  }\n}\n",
                "return x",
                Function,
                "int f(int x)",
            ),
            (
                "c_sharp",
                cfg!(feature = "lang-c-sharp"),
                "class A {\n  int F(int x) {\n    return x;\n  }\n}\n",
                "return x",
                Function,
                "int F(int x)",
            ),
            (
                "c",
                cfg!(feature = "lang-c"),
                "int add(int a, int b) {\n  return a + b;\n}\n",
                "return",
                Function,
                "int add(",
            ),
            (
                "cpp",
                cfg!(feature = "lang-cpp"),
                "class P {\n  int x;\n};\n",
                "int x",
                Class,
                "class P",
            ),
            (
                "shell",
                cfg!(feature = "lang-bash"),
                "greet() {\n  echo hi\n}\n",
                "echo",
                Function,
                "greet()",
            ),
            (
                "html",
                cfg!(feature = "lang-html"),
                "<div class=\"a\"><p>hi</p></div>\n",
                "hi",
                Class,
                "<p>",
            ),
            (
                "css",
                cfg!(feature = "lang-css"),
                "a { color: red; }\n",
                "color",
                Parameter,
                "color: red",
            ),
            (
                "toml",
                cfg!(feature = "lang-toml"),
                "[server]\nport = 1\n",
                "port",
                Class,
                "[server]",
            ),
            (
                "json",
                cfg!(feature = "lang-json"),
                "{\"a\": [1, 2]}\n",
                "2",
                Parameter,
                ", 2",
            ),
            (
                "yaml",
                cfg!(feature = "lang-yaml"),
                "a:\n  b: 1\n",
                "b: 1",
                Parameter,
                "b: 1",
            ),
            (
                "markdown",
                cfg!(feature = "lang-markdown"),
                "# Title\n\nBody text.\n",
                "Body",
                Class,
                "# Title",
            ),
        ]
    };

    #[test]
    fn per_language_fixtures() {
        for &(lang, enabled, src, marker, kind, expected) in FIXTURES {
            if !enabled {
                continue;
            }
            let c = code(src, lang);
            let pos = src.find(marker).expect("marker");
            let got = text(&c, c.textobject_range(kind, false, pos));
            assert!(got.starts_with(expected), "{lang}: got {got:?}");
        }
    }

    /// Every bundled `textobjects.scm` compiles against its grammar (each
    /// grammar compiled into this build), so a query typo fails here rather
    /// than silently disabling the feature for that language.
    #[test]
    fn every_bundled_query_compiles() {
        use crate::code::{LangAssets, compiled_query};
        let mut checked = 0;
        for file in LangAssets::iter() {
            let Some(lang) = file.strip_suffix("/textobjects.scm") else {
                continue;
            };
            let Some(language) = Code::get_language(lang) else {
                continue;
            };
            let asset = LangAssets::get(&file).expect("asset");
            let source = std::str::from_utf8(asset.data.as_ref()).expect("utf-8");
            if let Err(e) = compiled_query(lang, &language, source) {
                panic!("{lang}/textobjects.scm: {e}");
            }
            checked += 1;
        }
        assert!(checked > 0 || cfg!(not(feature = "lang-rust")));
    }
}
