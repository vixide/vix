//! The Vim-parity table (T844): key sequences run against Vix on every CI run,
//! each row marked stock Vim, optional Vix or external tool. Stock rows are
//! also checked against a real, unconfigured Vim (`vim -u NONE`) by
//! [`stock_rows_match_an_unconfigured_vim`], which CI's `vim-parity` job
//! runs with a pinned Vim; a row whose Vix result differs from Vim's says so
//! and records both, so neither drifts unnoticed.
//!
//! The table is the source of truth for every parity claim in
//! `docs/vix-for-vim-users/index.md`: [`the_guide_shows_this_table`] fails
//! when the guide's generated table is stale, and
//! `VIX_UPDATE_VIM_PARITY=1 cargo test -p vix-tui --test integration vim_parity`
//! rewrites it.
//!
//! Notation: `|` in a buffer is the cursor (before a character in Normal
//! and Visual mode, between characters in Insert mode); keys use Vim's
//! `<Esc>` / `<CR>` / `<BS>` / `<lt>` / `<C-r>` notation.
#![allow(
    clippy::wildcard_imports,
    reason = "every test needs a handful of the shared helpers in common.rs"
)]

use crate::integration::common::*;
use std::fmt::Write as _;
use vix::modal::Mode;
use vix::modal::register::RegisterKind;

/// Where a row's behavior comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Origin {
    /// Stock Vim behavior, compared against `vim -u NONE`.
    Vim,
    /// Optional Vix behavior a stock Vim has no key for (Tree-sitter
    /// objects, say); never compared with Vim.
    Vix,
    /// Behavior that runs an external tool (a build, a test runner); Vim's
    /// counterpart needs `:make` and a `makeprg`, so it is not compared.
    External,
}

/// What an unconfigured Vim does with a [`Origin::Vim`] row.
#[derive(Debug, Clone, Copy)]
enum VimResult {
    /// Exactly what Vix does: text, cursor, mode and register.
    Same,
    /// Something else: Vim's buffer (cursor marked) and mode. The row's
    /// `note` says why; the Vix columns hold Vix's documented behavior.
    Differs { after: &'static str, mode: Mode },
    /// Not a stock-Vim row.
    NotApplicable,
}

/// Extra state a row needs before its keys are typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Setup {
    None,
    /// A `build` task printing this gcc-style line, run (and finished)
    /// from Tools -> Tasks so its problem is on the quickfix list.
    GccTask(&'static str),
}

/// One row of the table.
#[derive(Debug, Clone, Copy)]
struct Row {
    keys: &'static str,
    origin: Origin,
    /// The file the buffer is opened as (Tree-sitter rows need `a.rs`).
    file: &'static str,
    before: &'static str,
    after: &'static str,
    mode: Mode,
    /// The named register the keys write, its text and kind.
    register: Option<(char, &'static str, RegisterKind)>,
    vim: VimResult,
    setup: Setup,
    note: &'static str,
}

const fn stock(keys: &'static str, before: &'static str, after: &'static str) -> Row {
    Row {
        keys,
        origin: Origin::Vim,
        file: "a.txt",
        before,
        after,
        mode: Mode::Normal,
        register: None,
        vim: VimResult::Same,
        setup: Setup::None,
        note: "",
    }
}

const fn vix(keys: &'static str, before: &'static str, after: &'static str) -> Row {
    Row {
        origin: Origin::Vix,
        vim: VimResult::NotApplicable,
        ..stock(keys, before, after)
    }
}

const fn external(keys: &'static str, before: &'static str, after: &'static str) -> Row {
    Row {
        origin: Origin::External,
        vim: VimResult::NotApplicable,
        ..stock(keys, before, after)
    }
}

impl Row {
    const fn mode(self, mode: Mode) -> Self {
        Self { mode, ..self }
    }
    const fn reg(self, text: &'static str, kind: RegisterKind) -> Self {
        Self {
            register: Some(('a', text, kind)),
            ..self
        }
    }
    const fn line_reg(self, text: &'static str) -> Self {
        self.reg(text, RegisterKind::Line)
    }
    const fn char_reg(self, text: &'static str) -> Self {
        self.reg(text, RegisterKind::Char)
    }
    const fn file(self, file: &'static str) -> Self {
        Self { file, ..self }
    }
    const fn note(self, note: &'static str) -> Self {
        Self { note, ..self }
    }
    const fn setup(self, setup: Setup) -> Self {
        Self { setup, ..self }
    }
    /// Vim ends in `after` (Normal mode) instead; `note` says why.
    const fn vim_differs(self, after: &'static str, note: &'static str) -> Self {
        Self {
            vim: VimResult::Differs {
                after,
                mode: Mode::Normal,
            },
            note,
            ..self
        }
    }
    const fn vim_differs_in(self, after: &'static str, mode: Mode, note: &'static str) -> Self {
        Self {
            vim: VimResult::Differs { after, mode },
            note,
            ..self
        }
    }
    /// Whether this build can run the row (Tree-sitter objects need a
    /// grammar; a task needs the `tasks` capability and a POSIX shell).
    fn runnable(&self) -> bool {
        let tree_sitter = cfg!(any(
            feature = "syntax-common",
            feature = "syntax-all",
            feature = "lang-rust"
        ));
        let tasks = cfg!(all(unix, feature = "tasks"));
        (self.file != "a.rs" || tree_sitter) && (self.setup == Setup::None || tasks)
    }
}

/// The table. Every row selects register `"a` before an operator or a put:
/// the unnamed register is the process-wide clipboard, which tests running
/// in parallel would race on.
const ROWS: &[Row] = &[
    // --- motions
    stock("3l", "|hello world\n", "hel|lo world\n"),
    stock("$", "|hello\n", "hell|o\n"),
    stock("0", "  ab|c\n", "|  abc\n"),
    stock("^", "  ab|c\n", "  |abc\n"),
    stock("w", "|pkg::run(arg);\n", "pkg|::run(arg);\n"),
    stock("3w", "|pkg::run(arg);\n", "pkg::run|(arg);\n"),
    stock("b", "foo ba|r\n", "foo |bar\n"),
    stock("e", "|foo bar\n", "fo|o bar\n"),
    stock("W", "|a.b c\n", "|a.b c\n").vim_differs(
        "a.b |c\n",
        "WORD motions (`W` `B` `E`) are not implemented: the key does nothing",
    ),
    stock("gg", "one\ntwo\nth|ree\n", "|one\ntwo\nthree\n"),
    stock("G", "|one\ntwo\nthree\n", "one\ntwo\n|three\n"),
    stock("2G", "|one\ntwo\nthree\n", "one\n|two\nthree\n"),
    stock("4ljj", "|abcdef\nx\nabcdef\n", "abcdef\nx\nabcd|ef\n")
        .note("the column is kept across a shorter line"),
    stock("2f.", "|a.b.c\n", "a.b|.c\n"),
    stock("t.", "|ab.c\n", "a|b.c\n"),
    stock("F.", "a.b.|c\n", "a.b|.c\n"),
    stock("T.", "a.b.|c\n", "a.b.|c\n"),
    stock("f.;", "|a.b.c\n", "a|.b.c\n").vim_differs(
        "a.b|.c\n",
        "`;` and `,` (repeat the last `f`/`t`) are not implemented",
    ),
    stock("}", "|one\ntwo\n\nthree\n", "one\ntwo\n\n|three\n").vim_differs(
        "one\ntwo\n|\nthree\n",
        "`}` stops on the next paragraph's first line, not on the blank line before it (a parity bug)",
    ),
    stock(")", "|One. Two.\n", "One. |Two.\n"),
    stock("%", "|(a [b] c)\n", "(a [b] c|)\n"),
    stock("*", "|foo bar foo\n", "|foo bar foo\n").vim_differs(
        "foo bar |foo\n",
        "`*` and `#` are not implemented",
    ),
    stock("/bar<CR>", "|foo bar\n", "foo |bar\n").note("`/` opens Vix's find bar"),
    stock("/o<CR>n", "|xo o\n", "xo |o\n"),
    stock("/bar<CR>\"ax", "|foo bar\n", "foo |bar\n").vim_differs(
        "foo |ar\n",
        "after `/pattern<CR>` the find bar keeps the keyboard until `<Esc>`, so the next keys edit the search, not the buffer (a parity bug)",
    ),
    stock("/bar<CR><Esc>\"ax", "|foo bar\n", "foo |ar\n").char_reg("b"),
    // --- operators
    stock("\"adw", "|foo bar\n", "|bar\n").char_reg("foo "),
    stock("\"ade", "|foo bar\n", "| bar\n").char_reg("foo"),
    stock("\"add", "|one\n  two\n", "  |two\n").line_reg("one\n"),
    stock("\"a2dd", "|one\ntwo\nthree\n", "|three\n").line_reg("one\ntwo\n"),
    stock("\"adw", "foo |bar\nbaz\n", "foo| \nbaz\n")
        .char_reg("bar")
        .note("`dw` on a line's last word keeps the line break"),
    stock("\"ad$", "fo|o bar\n", "f|o\n").char_reg("o bar"),
    stock("\"aD", "fo|o bar\n", "fo|o bar\n").vim_differs(
        "f|o\n",
        "`D` is not bound (`d$` is)",
    ),
    stock("\"ax", "|abc\n", "|bc\n").char_reg("a"),
    stock("\"a3x", "|abcd\n", "|d\n").char_reg("abc"),
    stock("\"aX", "ab|c\n", "ab|c\n").vim_differs("a|c\n", "`X` is not bound (`dh` is)"),
    stock("\"adf.", "|a.b.c\n", "|b.c\n").char_reg("a."),
    stock("\"a2d3w", "|a b c d e f g h\n", "|g h\n")
        .char_reg("a b c d e f ")
        .note("the two counts multiply"),
    stock("\"adj", "|one\ntwo\nthree\n", "|three\n").line_reg("one\ntwo\n"),
    stock("\"adG", "one\n|two\nthree\n", "|one\n").line_reg("two\nthree\n"),
    stock("\"acwX<Esc>", "|foo bar\n", "X| bar\n")
        .char_reg("foo")
        .vim_differs("|X bar\n", "leaving Insert mode does not move the cursor back onto the last typed character (a parity bug)"),
    stock("\"accX<Esc>", "  fo|o\nbar\n", "X|\nbar\n")
        .line_reg("  foo\n")
        .vim_differs(
            "|X\nbar\n",
            "leaving Insert mode does not move the cursor back onto the last typed character (a parity bug); an unconfigured Vim has `noautoindent`, so neither keeps the indent",
        ),
    stock("\"aCX<Esc>", "fo|o bar\n", "fo|o bar\n").vim_differs(
        "fo|X\n",
        "`C` is not bound (`c$` is), and neither is the `X` typed after it",
    ),
    stock("\"asX<Esc>", "|abc\n", "|abc\n").vim_differs(
        "|Xbc\n",
        "`s` is not bound (`cl` is)",
    ),
    stock("rX", "|abc\n", "|abc\n").vim_differs("|Xbc\n", "`r` is not bound"),
    stock("~", "|abc\n", "|abc\n").vim_differs("A|bc\n", "`~` is not implemented"),
    stock("J", "|one\ntwo\n", "|one\ntwo\n").vim_differs("one| two\n", "`J` is not bound"),
    stock(">>", "|one\n", "|one\n").vim_differs(
        "\t|one\n",
        "`>` and `<` are not implemented",
    ),
    // --- yank and put
    stock("\"ayy\"ap", "|one\ntwo\n", "one\n|one\ntwo\n").line_reg("one\n"),
    stock("\"ayw\"aP", "|foo bar\n", "foo| foo bar\n").char_reg("foo "),
    stock("\"ayl\"a3p", "|ab\n", "aaa|ab\n").char_reg("a"),
    stock("\"ayiw", "foo b|ar\n", "foo |bar\n").char_reg("bar"),
    // --- text objects
    stock("\"adiw", "foo b|ar baz\n", "foo | baz\n").char_reg("bar"),
    stock("\"adaw", "foo b|ar baz\n", "foo |baz\n").char_reg("bar "),
    stock("\"adi(", "f(a, |b)\n", "f(|)\n").char_reg("a, b"),
    stock("\"adib", "f(a, |b)\n", "f(|)\n").char_reg("a, b"),
    stock("\"ada\"", "say \"h|i\" now\n", "say | now\n")
        .char_reg("\"hi\"")
        .vim_differs(
            "say |now\n",
            "`a\"` leaves the white space after the closing quote (a parity bug)",
        ),
    stock("\"aci{X<Esc>", "{ a|b }\n", "{X|}\n")
        .char_reg(" ab ")
        .vim_differs("{|X}\n", "leaving Insert mode does not move the cursor back onto the last typed character (a parity bug)"),
    stock("\"adi<lt>", "<a|b>\n", "<|>\n").char_reg("ab"),
    stock("\"adip", "one\ntw|o\n\nthree\n", "one\ntw|o\n\nthree\n").vim_differs(
        "|\nthree\n",
        "`ip`/`ap` and `is`/`as` are not implemented: the operator is cancelled",
    ),
    // --- Visual mode
    stock("v", "|abc\n", "|abc\n").mode(Mode::Visual),
    stock("V", "|abc\n", "abc\n|").mode(Mode::VisualLine).vim_differs_in(
        "|abc\n",
        Mode::VisualLine,
        "Visual Line puts the cursor on the start of the next line (a parity bug)",
    ),
    stock("vll\"ad", "|hello\n", "|lo\n").char_reg("hel"),
    stock("Vj\"ad", "|one\ntwo\nthree\n", "|three\n").line_reg("one\ntwo\n"),
    stock("viw\"ay", "foo b|ar\n", "foo |bar\n").char_reg("bar"),
    stock("\"avlld", "|hello\n", "|lo\n").char_reg("hel").vim_differs(
        "|lo\n",
        "same text, but a `\"a` typed before `v` selects the register in Vix; Vim drops it and writes the unnamed register",
    ),
    stock("vw\"ad", "|foo bar\n", "foo |ar\n").char_reg("b").vim_differs(
        "|ar\n",
        "Visual mode moves with `h` `j` `k` `l` only: `w` moves the cursor and drops the selection",
    ),
    stock("vl\"ad.", "|abcdef\n", "|cdef\n").char_reg("ab").vim_differs(
        "|ef\n",
        "a Visual operator is not dot-repeatable: `.` does nothing",
    ),
    // --- dot-repeat
    stock("\"adw.", "|a b c\n", "|c\n").char_reg("b "),
    stock("\"a2dw3.", "|a b c d e f g\n", "|f g\n")
        .char_reg("c d e ")
        .note("a count before `.` replaces the recorded one"),
    stock("\"ax.", "|abc\n", "|c\n").char_reg("b"),
    stock("\"aciwX<Esc>w.", "|foo bar\n", "X X|\n")
        .char_reg("bar")
        .vim_differs("X |X\n", "leaving Insert mode does not move the cursor back onto the last typed character (a parity bug)"),
    stock("A;<Esc>j.", "|one\ntwo\n", "one;\ntwo;|\n").vim_differs("one;\ntwo|;\n", "leaving Insert mode does not move the cursor back onto the last typed character (a parity bug)"),
    stock("ofoo<Esc>.", "|top\n", "top\nfoo\nfoo|\n").vim_differs("top\nfoo\nfo|o\n", "leaving Insert mode does not move the cursor back onto the last typed character (a parity bug)"),
    // --- Insert mode
    stock("iX<Esc>", "ab|c\n", "abX|c\n").vim_differs("ab|Xc\n", "leaving Insert mode does not move the cursor back onto the last typed character (a parity bug)"),
    stock("aX<Esc>", "|abc\n", "aX|bc\n").vim_differs("a|Xbc\n", "leaving Insert mode does not move the cursor back onto the last typed character (a parity bug)"),
    stock("AX<Esc>", "|abc\n", "abcX|\n").vim_differs("abc|X\n", "leaving Insert mode does not move the cursor back onto the last typed character (a parity bug)"),
    stock("IX<Esc>", "  a|bc\n", "  X|abc\n").vim_differs("  |Xabc\n", "leaving Insert mode does not move the cursor back onto the last typed character (a parity bug)"),
    stock("oX<Esc>", "|one\ntwo\n", "one\nX|\ntwo\n").vim_differs("one\n|X\ntwo\n", "leaving Insert mode does not move the cursor back onto the last typed character (a parity bug)"),
    stock("OX<Esc>", "one\n|two\n", "one\nX|\ntwo\n").vim_differs("one\n|X\ntwo\n", "leaving Insert mode does not move the cursor back onto the last typed character (a parity bug)"),
    stock("iX", "ab|c\n", "abX|c\n").mode(Mode::Insert),
    stock("iXY<BS><Esc>", "|c\n", "X|c\n").vim_differs("|Xc\n", "leaving Insert mode does not move the cursor back onto the last typed character (a parity bug)"),
    // --- undo and redo
    stock("\"ax\"axu", "|abc\n", "b|c\n").char_reg("b").vim_differs(
        "|bc\n",
        "after `u` the cursor stays put instead of moving to the undone change (a parity bug)",
    ),
    stock("\"aciwXY<Esc>u", "|foo bar\n", "foo| bar\n")
        .char_reg("foo")
        .vim_differs(
            "|foo bar\n",
            "one `u` undoes the whole change, as in Vim; the cursor stays put (a parity bug)",
        ),
    stock("\"axu<C-r>", "|abc\n", "r|bc\n").vim_differs(
        "|bc\n",
        "`Ctrl-R` is not redo: Normal mode types an `r` into the buffer (a parity bug; Edit -> Redo works)",
    ),
    stock("\"ax\"axuug+", "|abc\n", "a|bc\n").vim_differs(
        "|bc\n",
        "`g-`/`g+` are not bound; Edit -> Undo Tree browses the tree instead",
    ),
    // --- Ex commands
    stock(":2<CR>", "|one\ntwo\n", "one\n|two\n"),
    stock(":s/o/0/<CR>", "|foo\n", "|f0o\n"),
    stock(":s/o/0/g<CR>", "|foo\n", "|f00\n"),
    stock(":%s/a/b/g<CR>", "|a\na\n", "b\n|b\n"),
    stock(":s/./X/<CR>", "|a.b\n", "|a.b\n").vim_differs(
        "|X.b\n",
        "`:s` takes literal patterns only and refuses Vim pattern syntax such as `.`, leaving the buffer unchanged",
    ),
    // --- optional Vix: Tree-sitter objects (a stock Vim has no such keys)
    vix("\"aciaX<Esc>", "fn f(a: u8, |b: u8) {}\n", "fn f(a: u8, X|) {}\n")
        .file("a.rs")
        .char_reg("b: u8")
        .note("Tree-sitter argument object (a plugin in Vim)"),
    vix("\"adaf", "fn a() {}\nfn |b() {}\n", "fn a() {}\n|\n")
        .file("a.rs")
        .char_reg("fn b() {}")
        .note("Tree-sitter function object (a plugin in Vim)"),
    vix("]f", "|fn a() {}\nfn b() {}\n", "fn a() {}\n|fn b() {}\n")
        .file("a.rs")
        .note("Tree-sitter next-function motion (a plugin in Vim)"),
    // --- external tool: the quickfix list is a task's problems
    external(":cn<CR>", "|one\ntwo\n", "one\nt|wo\n")
        .setup(Setup::GccTask("a.txt:2:2: error: boom"))
        .note("after a task printed `a.txt:2:2: error: boom` (Vim: `:make` with a `makeprg`)"),
];

// ----- running a row ---------------------------------------------------------

/// A buffer, cursor, mode and named register after a row's keys.
#[derive(Debug, PartialEq, Eq)]
struct Outcome {
    text: String,
    cursor: usize,
    mode: Mode,
    register: Option<(String, RegisterKind)>,
}

impl Outcome {
    fn shown(&self) -> String {
        mark_cursor(&self.text, self.cursor)
    }
}

/// `text` with `|` at char offset `cursor`.
fn mark_cursor(text: &str, cursor: usize) -> String {
    let mut out: String = text.chars().take(cursor).collect();
    out.push('|');
    out.extend(text.chars().skip(cursor));
    out
}

/// Split a marked buffer into its text and the cursor's char offset.
fn split_cursor(marked: &str) -> (String, usize) {
    let at = marked
        .chars()
        .position(|c| c == '|')
        .unwrap_or_else(|| panic!("no | cursor in {marked:?}"));
    (marked.replacen('|', "", 1), at)
}

/// Vim key notation to key events: `<Esc>`, `<CR>`, `<BS>`, `<Tab>`,
/// `<lt>` and `<C-x>`; every other character is itself.
pub(super) fn parse_keys(keys: &str) -> Vec<KeyEvent> {
    let mut out = Vec::new();
    let mut rest = keys;
    while let Some(c) = rest.chars().next() {
        if c == '<'
            && let Some(end) = rest.find('>')
        {
            let name = &rest[1..end];
            out.push(match name {
                "Esc" => esc(),
                "CR" => keycode(KeyCode::Enter),
                "BS" => keycode(KeyCode::Backspace),
                "Tab" => keycode(KeyCode::Tab),
                "lt" => key('<'),
                _ => {
                    let ch = name
                        .strip_prefix("C-")
                        .and_then(|s| s.chars().next())
                        .unwrap_or_else(|| panic!("unknown key <{name}>"));
                    ctrl(ch)
                }
            });
            rest = &rest[end + 1..];
        } else {
            out.push(key(c));
            rest = &rest[c.len_utf8()..];
        }
    }
    out
}

/// A temp-directory tag unique to `row` and to the test `caller` (tests run
/// in parallel, and two sharing a directory would delete it under each other).
fn row_tag(caller: &str, row: &Row) -> String {
    let safe: String = row
        .keys
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    format!(
        "vim-parity-{caller}-{safe}-{:x}",
        fxhash(row.before) ^ fxhash(row.keys)
    )
}

fn fxhash(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// Type `row.keys` into a fresh Vi-keymap Vix and read the result.
fn run_in_vix(caller: &str, row: &Row) -> Outcome {
    let (text, cursor) = split_cursor(row.before);
    let dir = unique_dir(&row_tag(caller, row));
    let file = dir.join(row.file);
    fs::write(&file, &text).unwrap();
    let mut settings = Settings::default();
    settings.keymap = "vi".to_string();
    let mut app = app_at_with(&dir, settings);
    if let Setup::GccTask(line) = row.setup {
        fs::write(
            dir.join("tasks.toml"),
            format!("[[task]]\nname = \"build\"\ncommand = \"echo '{line}'\"\nmatcher = \"gcc\"\n"),
        )
        .unwrap();
        app.run_action("tools.tasks");
        app.on_key(keycode(KeyCode::Enter));
        wait_until(
            &mut app,
            App::poll_command,
            |a| !a.command_running(),
            Duration::from_secs(10),
        );
    }
    app.open_initial(&file);
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_cursor(cursor);
    for k in parse_keys(row.keys) {
        app.on_key(k);
    }
    let tab = app.editor.active_tab().unwrap();
    let outcome = Outcome {
        text: tab.text(),
        cursor: tab.editor.get_cursor(),
        mode: app.vi_mode(),
        register: row
            .register
            .and_then(|(name, _, _)| app.vi_register(name).map(|v| (v.text.clone(), v.kind))),
    };
    let _ = fs::remove_dir_all(&dir);
    outcome
}

fn expected_vix(row: &Row) -> Outcome {
    let (text, cursor) = split_cursor(row.after);
    Outcome {
        text,
        cursor,
        mode: row.mode,
        register: row.register.map(|(_, t, k)| (t.to_string(), k)),
    }
}

fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Normal => "Normal",
        Mode::Insert => "Insert",
        Mode::Visual => "Visual",
        Mode::VisualLine => "Visual Line",
    }
}

#[test]
fn every_row_does_what_the_table_says_in_vix() {
    let mut failures = Vec::new();
    for row in ROWS.iter().filter(|r| r.runnable()) {
        let got = run_in_vix("table", row);
        let want = expected_vix(row);
        if got != want {
            failures.push(format!(
                "{:?} on {:?}:\n   want {:?} {} {:?}\n    got {:?} {} {:?}",
                row.keys,
                row.before,
                want.shown(),
                mode_name(want.mode),
                want.register,
                got.shown(),
                mode_name(got.mode),
                got.register,
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn the_table_is_well_formed() {
    let mut seen = std::collections::HashSet::new();
    for row in ROWS {
        assert!(
            seen.insert((row.keys, row.before, row.file)),
            "duplicate row {:?}",
            row.keys
        );
        split_cursor(row.before);
        split_cursor(row.after);
        assert!(
            row.before.ends_with('\n'),
            "{:?}: end the buffer in \\n",
            row.keys
        );
        assert_eq!(
            matches!(row.vim, VimResult::NotApplicable),
            row.origin != Origin::Vim,
            "{:?}: only stock rows are compared with Vim",
            row.keys
        );
        if matches!(row.vim, VimResult::Differs { .. }) || row.origin != Origin::Vim {
            assert!(!row.note.is_empty(), "{:?}: say what differs", row.keys);
        }
    }
}

// ----- the reference Vim -----------------------------------------------------

/// The Vim to compare with: `$VIX_VIM`, else `vim` on `PATH`.
fn vim_program() -> String {
    std::env::var("VIX_VIM").unwrap_or_else(|_| "vim".to_string())
}

/// A Vim double-quoted string for Vim key notation `keys`.
fn vim_string(keys: &str) -> String {
    let mut out = String::new();
    let mut rest = keys;
    while let Some(c) = rest.chars().next() {
        if c == '<'
            && let Some(end) = rest.find('>')
        {
            out.push('\\');
            out.push_str(&rest[..=end]);
            rest = &rest[end + 1..];
            continue;
        }
        if c == '"' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

/// Type `row.keys` into `vim -u NONE` (no vimrc, no plugins, `nocompatible`)
/// and read the same state back through a `<Cmd>` mapping-free snapshot.
fn run_in_vim(caller: &str, row: &Row) -> Outcome {
    let (text, cursor) = split_cursor(row.before);
    let dir = unique_dir(&format!("{}-vim", row_tag(caller, row)));
    let file = dir.join(row.file);
    fs::write(&file, &text).unwrap();
    let before_cursor: String = text.chars().take(cursor).collect();
    let line = before_cursor.matches('\n').count() + 1;
    let col = before_cursor.rsplit('\n').next().unwrap_or("").len() + 1;
    let out = dir.join("out.json");
    let driver = format!(
        "call cursor({line}, {col})\n\
         function! Snap() abort\n\
         \x20 let g:out = {{'text': join(getline(1, '$'), \"\\n\") . \"\\n\", \
         'byte': line2byte(line('.')) + col('.') - 2, 'mode': mode(), \
         'reg': getreg('a'), 'regtype': getregtype('a')}}\n\
         endfunction\n\
         call feedkeys(\"{keys}\\<Cmd>call Snap()\\<CR>\", 'xt')\n\
         call writefile([json_encode(g:out)], '{out}')\n\
         qa!\n",
        keys = vim_string(row.keys),
        out = out.display(),
    );
    fs::write(dir.join("driver.vim"), driver).unwrap();
    let status = std::process::Command::new(vim_program())
        .current_dir(&dir)
        .args([
            "-u",
            "NONE",
            "-i",
            "NONE",
            "-N",
            "-n",
            "--not-a-term",
            "-S",
            "driver.vim",
        ])
        .arg(row.file)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("run vim (set VIX_VIM to its path)");
    assert!(
        status.success(),
        "vim exited with {status} on {:?}",
        row.keys
    );
    let json: serde_json::Value = serde_json::from_str(&fs::read_to_string(&out).unwrap()).unwrap();
    let _ = fs::remove_dir_all(&dir);
    let text = json["text"].as_str().unwrap().to_string();
    let byte = usize::try_from(json["byte"].as_i64().unwrap().max(0)).unwrap();
    let cursor = text[..byte.min(text.len())].chars().count();
    let mode = match json["mode"].as_str().unwrap() {
        "n" => Mode::Normal,
        "i" => Mode::Insert,
        "v" => Mode::Visual,
        "V" => Mode::VisualLine,
        other => panic!("vim ended in mode {other:?} on {:?}", row.keys),
    };
    let reg = json["reg"].as_str().unwrap().to_string();
    let register = row.register.map(|_| {
        let kind = if json["regtype"].as_str() == Some("V") {
            RegisterKind::Line
        } else {
            RegisterKind::Char
        };
        (reg, kind)
    });
    Outcome {
        text,
        cursor,
        mode,
        register,
    }
}

/// What Vim must produce for a stock row.
fn expected_vim(row: &Row) -> Option<Outcome> {
    match row.vim {
        VimResult::Same => Some(expected_vix(row)),
        VimResult::Differs { after, mode } => {
            let (text, cursor) = split_cursor(after);
            Some(Outcome {
                text,
                cursor,
                mode,
                register: None,
            })
        }
        VimResult::NotApplicable => None,
    }
}

#[test]
#[ignore = "needs a real Vim; CI's vim-parity job runs it with a pinned one"]
fn stock_rows_match_an_unconfigured_vim() {
    let mut failures = Vec::new();
    for row in ROWS {
        let Some(want) = expected_vim(row) else {
            continue;
        };
        let mut got = run_in_vim("vim", row);
        if want.register.is_none() {
            got.register = None;
        }
        if got != want {
            failures.push(format!(
                "{:?} on {:?}:\n   table says Vim gives {:?} {} {:?}\n   Vim gave            {:?} {} {:?}",
                row.keys,
                row.before,
                want.shown(),
                mode_name(want.mode),
                want.register,
                got.shown(),
                mode_name(got.mode),
                got.register,
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// ----- the guide's table -----------------------------------------------------

const BEGIN: &str =
    "<!-- vim-parity-table:begin (generated by tui/tests/integration/vim_parity.rs) -->";
const END: &str = "<!-- vim-parity-table:end -->";

/// A buffer for a Markdown cell: the final newline dropped, others as `⏎`,
/// the cursor as `¦` (a plain bar would end the table cell).
fn cell(marked: &str) -> String {
    let body = marked.strip_suffix('\n').unwrap_or(marked);
    format!("`{}`", body.replace('\n', "⏎").replace('|', "¦"))
}

fn origin_name(origin: Origin) -> &'static str {
    match origin {
        Origin::Vim => "stock Vim",
        Origin::Vix => "optional Vix",
        Origin::External => "external tool",
    }
}

fn render_table() -> String {
    let mut out = String::new();
    out.push_str(BEGIN);
    // Key sequences and sample buffers are not words: keep CSpell out.
    out.push_str("\n<!-- cspell:disable -->\n");
    out.push_str("\n| Keys | Before | After in Vix | Mode | Kind | Same in Vim? |\n");
    out.push_str("| ---- | ------ | ------------ | ---- | ---- | ------------ |\n");
    for row in ROWS {
        let keys = format!("`{}`", row.keys.replace('|', "\\|"));
        let vim = match row.vim {
            VimResult::Same => "yes".to_string(),
            VimResult::Differs { after, .. } => {
                format!("no: Vim gives {}. {}", cell(after), row.note)
            }
            VimResult::NotApplicable => format!("n/a: {}", row.note),
        };
        let vim = if matches!(row.vim, VimResult::Same) && !row.note.is_empty() {
            format!("yes: {}", row.note)
        } else {
            vim
        };
        let _ = writeln!(
            out,
            "| {keys} | {} | {} | {} | {} | {vim} |",
            cell(row.before),
            cell(row.after),
            mode_name(row.mode),
            origin_name(row.origin),
        );
    }
    out.push_str("\n<!-- cspell:enable -->\n");
    out.push_str(END);
    out
}

fn guide_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/vix-for-vim-users/index.md")
}

#[test]
fn the_guide_shows_this_table() {
    let path = guide_path();
    let guide = fs::read_to_string(&path).unwrap();
    let start = guide.find(BEGIN).expect("the guide has the begin marker");
    let end = guide.find(END).expect("the guide has the end marker") + END.len();
    let table = render_table();
    if guide[start..end] == table {
        return;
    }
    if std::env::var_os("VIX_UPDATE_VIM_PARITY").is_some() {
        let updated = format!("{}{table}{}", &guide[..start], &guide[end..]);
        fs::write(&path, updated).unwrap();
        return;
    }
    panic!(
        "docs/vix-for-vim-users/index.md's parity table is stale: rerun with \
         VIX_UPDATE_VIM_PARITY=1 cargo test -p vix-tui --test integration vim_parity"
    );
}

/// `VIX_VIM_PARITY_DUMP=1` prints every row as Vix and Vim actually run it,
/// for writing new rows.
#[test]
#[ignore = "a tool for writing rows, not a check"]
fn dump_rows() {
    for row in ROWS.iter().filter(|r| r.runnable()) {
        let ours = run_in_vix("dump", row);
        let line = format!(
            "{:?} {:?}\n  vix {:?} {} {:?}",
            row.keys,
            row.before,
            ours.shown(),
            mode_name(ours.mode),
            ours.register
        );
        let reference = (row.origin == Origin::Vim).then(|| run_in_vim("dump", row));
        match reference {
            Some(v) => println!(
                "{line}\n  vim {:?} {} {:?}{}",
                v.shown(),
                mode_name(v.mode),
                v.register,
                if v.text == ours.text && v.cursor == ours.cursor && v.mode == ours.mode {
                    ""
                } else {
                    "   <-- DIFFERS"
                }
            ),
            None => println!("{line}"),
        }
    }
}
