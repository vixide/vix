//! A pragmatic subset of [Emmet](https://emmet.io/) abbreviation expansion.
//!
//! Supports the common operators and modifiers: child `>`, sibling `+`, multiply
//! `*N`, id `#id`, classes `.a.b`, literal text `{text}`, and `$` numbering inside
//! text within a multiplied element. For example:
//!
//! ```
//! let html = vix_emmet::expand("ul>li.item*2").unwrap();
//! assert!(html.contains("<ul>"));
//! assert!(html.matches("<li class=\"item\">").count() == 2);
//! ```
//!
//! Grouping with `()` is not supported; such input returns `None`.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// One element node in the parsed abbreviation tree.
struct Node {
    tag: String,
    id: Option<String>,
    classes: Vec<String>,
    text: Option<String>,
    count: usize,
    parent: Option<usize>,
    children: Vec<usize>,
}

/// Expand an Emmet `abbr` into indented HTML, or `None` if it can't be parsed.
#[must_use]
pub fn expand(abbr: &str) -> Option<String> {
    let abbr = abbr.trim();
    if abbr.is_empty() {
        return None;
    }
    let chars: Vec<char> = abbr.chars().collect();
    let mut p = Parser { chars, pos: 0, nodes: Vec::new() };
    let roots = p.parse()?;
    if p.pos != p.chars.len() {
        return None; // unparsed remainder (e.g. unsupported `()`)
    }
    let mut out = String::new();
    for r in roots {
        p.render(r, 0, &mut out);
    }
    Some(out)
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
    nodes: Vec<Node>,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    /// Read a run of identifier characters (`[A-Za-z0-9_-$]`).
    fn read_name(&mut self) -> String {
        let mut s = String::new();
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || matches!(c, '-' | '_' | '$') {
                s.push(c);
                self.pos += 1;
            } else {
                break;
            }
        }
        s
    }

    /// Parse the element sequence into the arena; returns the root node indices.
    fn parse(&mut self) -> Option<Vec<usize>> {
        let mut roots = Vec::new();
        let mut last: Option<usize> = None;
        let mut first = true;
        loop {
            let op = if first {
                None
            } else {
                match self.peek() {
                    Some(c @ ('>' | '+')) => {
                        self.pos += 1;
                        Some(c)
                    }
                    _ => break,
                }
            };
            let node = self.parse_item()?;
            let idx = self.nodes.len();
            let parent = match op {
                None => None,
                Some('>') => last,
                _ => last.and_then(|l| self.nodes[l].parent), // '+': sibling of last
            };
            self.nodes.push(Node { parent, ..node });
            match parent {
                Some(p) => self.nodes[p].children.push(idx),
                None => roots.push(idx),
            }
            last = Some(idx);
            first = false;
        }
        (!roots.is_empty()).then_some(roots)
    }

    /// Parse a single element: `tag` plus `#id` / `.class` / `{text}` / `*count`
    /// modifiers in any order.
    fn parse_item(&mut self) -> Option<Node> {
        let tag = self.read_name();
        let mut node =
            Node { tag, id: None, classes: Vec::new(), text: None, count: 1, parent: None, children: Vec::new() };
        loop {
            match self.peek() {
                Some('#') => {
                    self.pos += 1;
                    node.id = Some(self.read_name());
                }
                Some('.') => {
                    self.pos += 1;
                    node.classes.push(self.read_name());
                }
                Some('{') => {
                    self.pos += 1;
                    let mut t = String::new();
                    while let Some(c) = self.peek() {
                        self.pos += 1;
                        if c == '}' {
                            break;
                        }
                        t.push(c);
                    }
                    node.text = Some(t);
                }
                Some('*') => {
                    self.pos += 1;
                    let n = self.read_name();
                    node.count = n.parse().ok()?;
                }
                _ => break,
            }
        }
        if node.tag.is_empty() {
            node.tag = "div".to_string();
        }
        Some(node)
    }

    /// Render node `idx` (repeating it `count` times) into `out` at `depth`.
    fn render(&self, idx: usize, depth: usize, out: &mut String) {
        use std::fmt::Write as _;
        let node = &self.nodes[idx];
        for i in 1..=node.count.max(1) {
            let indent = "  ".repeat(depth);
            let id = node.id.as_ref().map(|v| format!(" id=\"{}\"", number(v, i))).unwrap_or_default();
            let class = if node.classes.is_empty() {
                String::new()
            } else {
                format!(" class=\"{}\"", node.classes.iter().map(|c| number(c, i)).collect::<Vec<_>>().join(" "))
            };
            let open = format!("<{}{id}{class}>", node.tag);
            if node.children.is_empty() {
                let text = node.text.as_ref().map(|t| number(t, i)).unwrap_or_default();
                let _ = writeln!(out, "{indent}{open}{text}</{}>", node.tag);
            } else {
                let _ = writeln!(out, "{indent}{open}");
                for &c in &node.children {
                    self.render(c, depth + 1, out);
                }
                let _ = writeln!(out, "{indent}</{}>", node.tag);
            }
        }
    }
}

/// Replace `$` in `s` with the 1-based copy index `i` (Emmet numbering).
fn number(s: &str, i: usize) -> String {
    if s.contains('$') {
        s.replace('$', &i.to_string())
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_tag() {
        assert_eq!(expand("div").unwrap(), "<div></div>\n");
    }

    #[test]
    fn id_classes_and_text() {
        assert_eq!(expand("a#home.nav.big{Home}").unwrap(), "<a id=\"home\" class=\"nav big\">Home</a>\n");
    }

    #[test]
    fn child_and_multiply_with_numbering() {
        let html = expand("ul>li.item$*2").unwrap();
        assert_eq!(
            html,
            "<ul>\n  <li class=\"item1\"></li>\n  <li class=\"item2\"></li>\n</ul>\n"
        );
    }

    #[test]
    fn siblings() {
        let html = expand("h1+p").unwrap();
        assert_eq!(html, "<h1></h1>\n<p></p>\n");
    }

    #[test]
    fn nested_then_sibling() {
        // `a>b+c`: b and c are siblings under a.
        let html = expand("a>b+c").unwrap();
        assert_eq!(html, "<a>\n  <b></b>\n  <c></c>\n</a>\n");
    }

    #[test]
    fn grouping_unsupported_returns_none() {
        assert!(expand("(a>b)+c").is_none());
    }
}
