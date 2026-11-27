//! Large buffers (T749): an idle frame does no whole-buffer work, and opening
//! a file costs about its own size in memory, not a multiple of it.
#![allow(
    clippy::wildcard_imports,
    reason = "every test needs a handful of the shared helpers in common.rs"
)]

use std::io::Write as _;

use crate::integration::common::*;

/// `n` log lines of about 90 bytes each.
fn write_log(path: &Path, n: usize) {
    let mut out = std::io::BufWriter::new(fs::File::create(path).unwrap());
    for i in 0..n {
        writeln!(
            out,
            "2026-10-30T12:00:{:02}Z INFO worker={} request id={i} path=/api/v1/items/{} status=200",
            i % 60,
            i % 16,
            i * 7
        )
        .unwrap();
    }
}

/// Draw one frame, as the event loop does after every tick.
fn draw(app: &mut App) {
    use ratatui::{Terminal, backend::TestBackend};
    let mut term = Terminal::new(TestBackend::new(120, 40)).unwrap();
    term.draw(|f| vix::ui::draw(app, f)).unwrap();
}

/// The event loop with no input: tick, then draw. Before T749 every frame
/// copied the whole buffer (to size the horizontal scrollbar), which on a
/// 100 MB file was ~7.5% of a core with nothing happening.
#[test]
fn idle_frames_make_no_whole_buffer_pass() {
    use vix::editor_core::code::full_text_scans;
    let dir = unique_dir("t749-idle");
    let path = dir.join("server.log");
    write_log(&path, 5_000);
    let _ = fs::remove_dir_all(dir.join("backup"));
    let mut app = app_at(&dir).with_backup_dir(dir.join("backup"));
    app.open_initial(&path);
    assert!(app.editor.active_tab().is_some_and(|t| t.path.is_some()));
    // The first frames may measure the buffer once.
    for _ in 0..2 {
        app.tick();
        draw(&mut app);
    }
    let before = full_text_scans();
    for _ in 0..10 {
        app.tick();
        draw(&mut app);
    }
    assert_eq!(
        full_text_scans(),
        before,
        "an idle frame read the whole buffer"
    );

    // An edit is measured again, once.
    app.on_key(key('x'));
    draw(&mut app);
    let after_edit = full_text_scans();
    // The buffer is now dirty, so hot exit's idle pass (every 2 s) copies it
    // once into a backup — specified work, once per changed revision
    // (`crates/vix-undo-store/spec/hot-exit/index.md`), not idle-frame work.
    // Whether that pass fell inside the frames below used to depend on how
    // slow they were, so make it due now: the first tick takes the one copy
    // (T773).
    app.hot_exit.make_pass_due();
    app.tick();
    draw(&mut app);
    assert_eq!(
        full_text_scans(),
        after_edit + 1,
        "hot exit backs the changed buffer up with one copy"
    );
    let backed_up = full_text_scans();
    for _ in 0..5 {
        app.tick();
        draw(&mut app);
    }
    // A later pass, due or not, finds the backup current and copies nothing.
    app.hot_exit.make_pass_due();
    app.tick();
    draw(&mut app);
    assert_eq!(
        full_text_scans(),
        backed_up,
        "an idle frame after the backup read the whole buffer"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// This process's resident memory in KiB: the peak (`VmHWM`) where Linux
/// reports it, otherwise the current figure from `ps`. On macOS the two
/// agree here: the allocator keeps freed large blocks resident, which is
/// why transient copies of the text showed up as memory at all.
#[cfg(unix)]
fn resident_kib() -> u64 {
    if let Ok(status) = fs::read_to_string("/proc/self/status")
        && let Some(line) = status.lines().find(|l| l.starts_with("VmHWM:"))
    {
        return line
            .split_whitespace()
            .nth(1)
            .and_then(|n| n.parse().ok())
            .unwrap_or(0);
    }
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse()
        .unwrap_or(0)
}

const CHILD_ENV: &str = "VIX_T749_MEMORY_CHILD";
const FILE_MB: usize = 20;

/// Run in a process of its own (see the test below): open a ~20 MB log,
/// draw and tick a few frames, and print how much the resident memory grew.
#[cfg(unix)]
#[test]
#[ignore = "run as a child process by opening_a_large_file_costs_about_its_size"]
fn memory_child() {
    if std::env::var_os(CHILD_ENV).is_none() {
        return;
    }
    let dir = unique_dir("t749-memory");
    let path = dir.join("big.log");
    write_log(&path, FILE_MB * 1_000_000 / 90);
    let size = fs::metadata(&path).unwrap().len();
    let mut app = app_at(&dir);
    draw(&mut app);
    let before = resident_kib();
    app.open_initial(&path);
    for _ in 0..5 {
        app.tick();
        draw(&mut app);
    }
    let after = resident_kib();
    assert!(app.editor.active_tab().is_some_and(|t| t.path.is_some()));
    println!("T749 size={size} grew_kib={}", after.saturating_sub(before));
    let _ = fs::remove_dir_all(&dir);
}

/// Opening a file holds its text once (the rope, about the file's size),
/// not several transient copies of it. Before T749 a 100 MB file made Vix
/// ~1.1 GiB resident; the budget here is three times the file's size.
/// Measured in a child process so no other test's allocations count.
#[cfg(unix)]
#[test]
fn opening_a_large_file_costs_about_its_size() {
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "integration::big_buffer::memory_child",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_ENV, "1")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{stdout}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let line = stdout
        .lines()
        // The harness may print the test's name on the same line.
        .find_map(|l| l.find("T749 ").map(|i| &l[i..]))
        .unwrap_or_else(|| panic!("no measurement in {stdout}"));
    let field = |name: &str| -> u64 {
        line.split_whitespace()
            .find_map(|w| w.strip_prefix(name))
            .and_then(|v| v.parse().ok())
            .unwrap()
    };
    let size_kib = field("size=") / 1024;
    let grew = field("grew_kib=");
    assert!(
        grew < 3 * size_kib,
        "opening a {size_kib} KiB file grew resident memory by {grew} KiB"
    );
}

/// A Vi (modal-engine) app editing `text` in a fresh file.
fn vi_app_with(tag: &str, text: &str) -> App {
    let mut settings = Settings::default();
    settings.keymap = "vi".to_string();
    settings.subsystems.modal_engine = true;
    let dir = unique_dir(tag);
    let file = dir.join("a.txt");
    fs::write(&file, text).unwrap();
    let mut app = app_at_with(&dir, settings);
    app.open_initial(&file);
    app
}

fn vi_cursor(app: &App) -> usize {
    app.editor.active_tab().map_or(0, |t| t.editor.get_cursor())
}

fn set_vi_cursor(app: &mut App, at: usize) {
    app.editor.active_tab_mut().unwrap().editor.set_cursor(at);
}

/// Three copies of one line, the cursor on the middle one.
const VI_LINES: &str = "    alpha beta.gamma(delta, epsilon) zeta_eta\n\
                        \x20   alpha beta.gamma(delta, epsilon) zeta_eta\n\
                        \x20   alpha beta.gamma(delta, epsilon) zeta_eta\n";

/// Unrelated text around [`VI_LINES`] in the large buffer.
fn vi_filler() -> String {
    "filler words, filler words. More filler!\n\n".repeat(20_000)
}

/// T843: every Vi motion and operator below gives the same result —
/// cursor relative to the lines, and the lines' text — whether the three
/// lines are the whole buffer or sit inside ~1.7 MB of unrelated text.
#[test]
fn vi_motions_on_a_large_buffer_match_the_same_lines_alone() {
    let filler = vi_filler();
    let big_text = format!("{filler}{VI_LINES}{filler}");
    let off = filler.chars().count();
    let line_len = VI_LINES.chars().count() / 3;
    let start = line_len + 8;
    let sequences: &[&str] = &[
        "l", "3l", "h", "4h", "w", "2w", "b", "3b", "e", "2e", "$", "0", "^", "fa", "2fa", "t)",
        "Fa", "Tb", "j", "k", "x", "3x", "dw", "3dw", "d2w", "db", "de", "d$", "dfa", "dt)", "dd",
        "dk", "yw", "ywP", "cwX",
    ];
    let mut small = vi_app_with("t843-small", VI_LINES);
    let mut big = vi_app_with("t843-big", &big_text);
    for seq in sequences {
        set_vi_cursor(&mut small, start);
        set_vi_cursor(&mut big, off + start);
        for c in seq.chars() {
            small.on_key(key(c));
            big.on_key(key(c));
        }
        if seq.starts_with('c') {
            small.on_key(esc());
            big.on_key(esc());
        }
        let small_now = small.editor.active_tab().unwrap().text();
        let big_now = big.editor.active_tab().unwrap().text();
        let middle = big_now
            .strip_prefix(filler.as_str())
            .and_then(|t| t.strip_suffix(filler.as_str()))
            .unwrap_or_else(|| panic!("{seq}: the edit reached the filler"));
        assert_eq!(middle, small_now, "{seq}: same text");
        assert_eq!(
            vi_cursor(&big).checked_sub(off),
            Some(vi_cursor(&small)),
            "{seq}: same cursor"
        );
        // Put both buffers back for the next sequence.
        if small_now != VI_LINES {
            small
                .editor
                .active_tab_mut()
                .unwrap()
                .editor
                .set_content(VI_LINES);
            big.editor
                .active_tab_mut()
                .unwrap()
                .editor
                .set_content(&big_text);
        }
    }
}

/// T843: a Vi motion or operator reads the lines it acts on, never the whole
/// buffer — before it, `l` copied all of the text out (8.6 ms at 8 MiB).
#[test]
fn vi_motions_and_operators_make_no_whole_buffer_pass() {
    use vix::editor_core::code::full_text_scans;
    let filler = vi_filler();
    let mut app = vi_app_with("t843-scans", &format!("{filler}{VI_LINES}{filler}"));
    let off = filler.chars().count();
    set_vi_cursor(&mut app, off + 50);
    let before = full_text_scans();
    for seq in [
        "l", "h", "3l", "w", "b", "e", "j", "k", "$", "0", "^", "fa", "G", "gg", "}", "{", ")",
        "(", "3)", "2(",
    ] {
        for c in seq.chars() {
            app.on_key(key(c));
        }
        assert_eq!(full_text_scans(), before, "{seq} read the whole buffer");
        set_vi_cursor(&mut app, off + 50);
    }
    for seq in ["x", "3dw", "d$", "dd", "2dd", "yw"] {
        for c in seq.chars() {
            app.on_key(key(c));
        }
        assert_eq!(full_text_scans(), before, "{seq} read the whole buffer");
    }
}

/// T830: `(` and `)` land on the same sentence, relative to it, whether
/// four sentences are the whole buffer or sit inside ~1.7 MB of other
/// sentences: the scan reads from the cursor outward, so the filler (which
/// ends in a sentence end and a blank line) changes nothing.
#[test]
fn vi_sentence_motions_on_a_large_buffer_match_the_same_text_alone() {
    const SENTENCES: &str = "Alpha one. Beta two! Gamma (three)? Delta.\n";
    let filler = vi_filler();
    let big_text = format!("{filler}{SENTENCES}{filler}");
    let off = filler.chars().count();
    let beta = SENTENCES.find("Beta").unwrap() + 2;
    let mut small = vi_app_with("t830-sentence-small", SENTENCES);
    let mut big = vi_app_with("t830-sentence-big", &big_text);
    for (seq, want) in [
        (")", "Gamma"),
        ("(", "Beta"),
        ("2)", "Delta"),
        ("2(", "Alpha"),
    ] {
        set_vi_cursor(&mut small, beta);
        set_vi_cursor(&mut big, off + beta);
        for c in seq.chars() {
            small.on_key(key(c));
            big.on_key(key(c));
        }
        assert_eq!(vi_cursor(&small), SENTENCES.find(want).unwrap(), "{seq}");
        assert_eq!(
            vi_cursor(&big).checked_sub(off),
            Some(vi_cursor(&small)),
            "{seq}: same cursor"
        );
    }
}

/// T830: Go → Sentence → Next/Previous read the rope from the cursor to
/// the start they land on, never the whole buffer, and land where they
/// would in the same sentences alone.
#[test]
fn go_sentence_next_and_previous_make_no_whole_buffer_pass() {
    use vix::editor_core::code::full_text_scans;
    const SENTENCES: &str = "Alpha one. Beta two! Gamma (three)? Delta.\n";
    let filler = vi_filler();
    let mut app = vi_app_with("t830-go-sentence", &format!("{filler}{SENTENCES}{filler}"));
    let off = filler.chars().count();
    let at = |word: &str| off + SENTENCES.find(word).unwrap();
    set_vi_cursor(&mut app, at("Beta") + 2);
    let before = full_text_scans();
    app.run_action("nav.sentence_next");
    assert_eq!(vi_cursor(&app), at("Gamma"), "next");
    app.run_action("nav.sentence_prev");
    assert_eq!(vi_cursor(&app), at("Beta"), "previous");
    app.run_action("nav.sentence_prev");
    assert_eq!(vi_cursor(&app), at("Alpha"), "previous again");
    assert_eq!(
        full_text_scans(),
        before,
        "Go → Sentence read the whole buffer"
    );
}

/// T851: Go → Word (Start, End, Next, Previous, Number), Go → Sentence
/// (Start, End, Number), Go → Matching Tag and Highlight Word read the
/// rope out from the cursor (or, for Number, from the top to the target),
/// never a copy of the whole buffer, and land where they should.
#[test]
fn go_word_sentence_tag_and_highlight_word_make_no_whole_buffer_pass() {
    use vix::editor_core::code::full_text_scans;
    const MIDDLE: &str =
        "Alpha one. Beta two! Gamma (three)? Delta.\n<ul><li>item</li><li>item</li></ul>\n";
    let filler = vi_filler();
    let mut app = vi_app_with("t851-scans", &format!("{filler}{MIDDLE}{filler}"));
    app.settings.editor_visual.highlight_word = true;
    let off = filler.chars().count();
    let at = |s: &str| off + MIDDLE.find(s).unwrap();
    let area = ratatui::layout::Rect::new(0, 0, 120, 40);
    let before = full_text_scans();
    for (from, action, want) in [
        (at("Beta") + 2, "nav.word_start", at("Beta")),
        (at("Beta") + 2, "nav.word_end", at("Beta") + 4),
        (at("Beta") + 2, "nav.word_next", at("two")),
        (at("two"), "nav.word_prev", at("Beta")),
        (at("Beta") + 2, "nav.sentence_start", at("Beta")),
        (at("Beta") + 2, "nav.sentence_end", at("Gamma") - 1),
        (at("<ul>") + 1, "nav.matching_tag", at("</ul>")),
        (at("</ul>") + 2, "nav.matching_tag", at("<ul>")),
    ] {
        set_vi_cursor(&mut app, from);
        app.run_action(action);
        assert_eq!(vi_cursor(&app), want, "{action} from {from}");
        assert_eq!(full_text_scans(), before, "{action} read the whole buffer");
    }
    app.editor.goto_word(3, area);
    assert_eq!(vi_cursor(&app), "filler words, ".len(), "Go → Word → 3");
    app.editor.goto_sentence(2, area);
    assert_eq!(
        vi_cursor(&app),
        "filler words, filler words. ".len(),
        "Go → Sentence → 2"
    );
    set_vi_cursor(&mut app, at("item") + 1);
    app.refresh_word_highlight();
    let marks = app
        .editor
        .active_tab()
        .unwrap()
        .editor
        .word_marks()
        .cloned();
    let second = at("item</li></ul>");
    assert_eq!(
        marks,
        Some(vec![(at("item"), at("item") + 4), (second, second + 4)])
    );
    assert_eq!(full_text_scans(), before, "a whole-buffer pass");
}
