//! Cheap, textual progress checks against a chapter's working buffer
//! (`spec/index.md`, § Progress checks). One hand-written check function per
//! chapter — deliberately not a generic checker DSL; see the spec for why.

/// How many of a chapter's tasks currently verify as complete.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Progress {
    /// Tasks in this chapter.
    pub total: usize,
    /// How many currently verify as complete, re-derived fresh from `text`
    /// and `cursor` on every call — nothing here is persisted (`spec/index.md`,
    /// § Session state).
    pub done: usize,
}

/// The current progress for `chapter_id` against its working buffer's `text`
/// and `cursor` (a character offset, matching `vix-modal`'s convention).
/// Unknown chapter ids report zero of zero, never a panic.
#[must_use]
pub fn progress(chapter_id: &str, text: &str, cursor: usize) -> Progress {
    match chapter_id {
        "moving-around" => moving_around(text, cursor),
        "editing-basics" => editing_basics(text, cursor),
        "find-and-replace" => find_and_replace(text, cursor),
        "multi-cursor-and-selection" => multi_cursor_and_selection(text, cursor),
        "files-tabs-and-palette" => files_tabs_and_palette(text, cursor),
        "git-basics" => git_basics(text, cursor),
        _ => Progress::default(),
    }
}

/// Whether some line starts with `prefix` and has more, once trimmed, than
/// `prefix` alone — the shared shape behind every "write something after
/// this label" task below (T403's files/palette and git chapters, both
/// asking the learner to report back in prose rather than a check that
/// could observe file/tab/git state, which no chapter's `text`-only check
/// can see).
fn some_line_has_more_than_prefix(text: &str, prefix: &str) -> bool {
    text.lines().any(|l| {
        let l = l.trim_start();
        l.starts_with(prefix) && l.trim() != prefix.trim()
    })
}

/// Whether some line contains `marker` but doesn't end at it — i.e.
/// something was typed right after it. The complement of
/// [`some_line_has_more_than_prefix`] for a task that asks the learner to
/// extend a line ending in `marker`, rather than one starting with a label.
fn some_line_extends_past_marker(text: &str, marker: &str) -> bool {
    text.lines().any(|l| {
        let l = l.trim_end();
        l.contains(marker) && !l.ends_with(marker)
    })
}

/// Chapter 1 checks (`crates/vix-tutor/lessons/01-moving-around.txt`): all
/// three are text-only, deliberately not cursor-position checks — a
/// "cursor is currently here" check would stop being true the moment the
/// learner moves on to the next task, undoing its own progress. `cursor` is
/// accepted (matching every chapter's uniform signature) but unused by this
/// particular chapter.
fn moving_around(text: &str, _cursor: usize) -> Progress {
    let total = 3;
    let mut done = 0;

    // Task 1: the ">>>" line now ends with "DONE".
    if text
        .lines()
        .any(|l| l.trim_start().starts_with(">>>") && l.trim_end().ends_with("DONE"))
    {
        done += 1;
    }

    // Task 2: the stray line is gone.
    if !text.contains("DELETE THIS ENTIRE LINE") {
        done += 1;
    }

    // Task 3: the last line still starts with "LAST LINE:" but now has more
    // after it than the bundled body did.
    if let Some(last) = text.lines().last() {
        let last = last.trim_start();
        if last.starts_with("LAST LINE:") && last.trim() != "LAST LINE:" {
            done += 1;
        }
    }

    Progress { total, done }
}

/// Chapter 2 checks (`02-editing-basics.txt`).
fn editing_basics(text: &str, _cursor: usize) -> Progress {
    let total = 2;
    let mut done = 0;

    // Task 1: the greeting line has more on it than just "Hello,".
    if some_line_has_more_than_prefix(text, "Hello,") {
        done += 1;
    }

    // Task 2: the original content line (word and all) is gone. Checking
    // for that exact original line rather than a bare `!contains("REDUNDANT")`
    // matters here: this chapter's own instructions spell the word out too
    // ("The word REDUNDANT below..."), so a bare substring check could never
    // pass — a real bug T403 caught via its own tests, not eyeballing.
    if !text.contains("This sentence has one REDUNDANT word that needs to go.") {
        done += 1;
    }

    Progress { total, done }
}

/// Chapter 3 checks (`03-find-and-replace.txt`).
fn find_and_replace(text: &str, _cursor: usize) -> Progress {
    let total = 2;
    let mut done = 0;

    // Task 1: the original all-"cat" sentence is gone, and "dog" showed up
    // somewhere — checking the exact original line (not a bare
    // `!text.contains("cat")`) for the same reason as chapter 2's task 2
    // above: the instructions themselves say `"cat"` too.
    if !text.contains("The cat sat on the cat mat, next to another cat entirely.")
        && text.contains("dog")
    {
        done += 1;
    }

    // Task 2: the "teh" typo is fixed (word-bounded, so a legitimate "teh"
    // substring elsewhere — there isn't one in this lesson — wouldn't
    // false-negative the check).
    if !text.contains(" teh ") {
        done += 1;
    }

    Progress { total, done }
}

/// Chapter 4 checks (`04-multi-cursor-and-selection.txt`).
fn multi_cursor_and_selection(text: &str, _cursor: usize) -> Progress {
    let total = 2;
    let mut done = 0;

    // Task 1: all three lines got " OK" appended, in one multi-cursor pass
    // or three separate edits — the check can't tell which, and doesn't
    // need to.
    if ["first line OK", "second line OK", "third line OK"]
        .iter()
        .all(|marker| text.contains(marker))
    {
        done += 1;
    }

    // Task 2: the whole three-line REMOVE block is gone. Checking that exact
    // three-in-a-row block, not a bare `!text.contains("REMOVE")`, for the
    // same instructions-leak reason as chapter 2/3 above (this chapter's own
    // instructions say "the whole block of REMOVE lines").
    if !text.contains("REMOVE\nREMOVE\nREMOVE") {
        done += 1;
    }

    Progress { total, done }
}

/// Chapter 5 checks (`05-files-tabs-and-palette.txt`). Both tasks ask the
/// learner to report back in prose (§ `some_line_has_more_than_prefix`) —
/// this chapter's real subject (the palette, other open tabs) isn't
/// something a single buffer's text can observe.
fn files_tabs_and_palette(text: &str, _cursor: usize) -> Progress {
    let total = 2;
    let mut done = 0;

    // Task 1: the ">>>" marker has more after it than the pristine body did
    // — not a bare `text.contains("CHECKED")`, since this chapter's own
    // instructions say "type CHECKED" too (same leak as above).
    if some_line_extends_past_marker(text, ">>>") {
        done += 1;
    }
    if some_line_has_more_than_prefix(text, "Another file I opened:") {
        done += 1;
    }

    Progress { total, done }
}

/// Chapter 6 checks (`06-git-basics.txt`). Same reporting-in-prose shape as
/// chapter 5, for the same reason (git state isn't visible from `text`
/// alone).
fn git_basics(text: &str, _cursor: usize) -> Progress {
    let total = 2;
    let mut done = 0;

    if some_line_has_more_than_prefix(text, "What the git status panel showed me:") {
        done += 1;
    }
    // Not a bare `text.to_ascii_lowercase().contains("staged")` — this
    // chapter's own instructions say `"staged"` too (same leak as above).
    if some_line_has_more_than_prefix(text, "Practiced staging a hunk:") {
        done += 1;
    }

    Progress { total, done }
}

#[cfg(test)]
mod tests {
    use super::{Progress, progress};
    use crate::chapter;

    #[test]
    fn unknown_chapter_is_zero_of_zero() {
        assert_eq!(
            progress("no-such-chapter", "anything", 0),
            Progress::default()
        );
    }

    #[test]
    fn fresh_working_copy_is_zero_of_three() {
        let body = chapter::chapter("moving-around").unwrap().body();
        assert_eq!(
            progress("moving-around", body, 0),
            Progress { total: 3, done: 0 }
        );
    }

    #[test]
    fn each_task_increments_done_independently() {
        let base = chapter::chapter("moving-around").unwrap().body();

        let t1 = base.replacen(
            ">>> Move to the end of this line, then type DONE right after it.",
            ">>> Move to the end of this line, then type DONE right after it.DONE",
            1,
        );
        assert_eq!(progress("moving-around", &t1, 0).done, 1);

        let t2 = base.replacen("DELETE THIS ENTIRE LINE\n", "", 1);
        assert_eq!(progress("moving-around", &t2, 0).done, 1);

        let t3 = base.replacen("LAST LINE:", "LAST LINE: hello", 1);
        assert_eq!(progress("moving-around", &t3, 0).done, 1);
    }

    #[test]
    fn all_three_together_is_three_of_three() {
        let base = chapter::chapter("moving-around").unwrap().body();
        let all = base
            .replacen(
                ">>> Move to the end of this line, then type DONE right after it.",
                ">>> Move to the end of this line, then type DONE right after it.DONE",
                1,
            )
            .replacen("DELETE THIS ENTIRE LINE\n", "", 1)
            .replacen("LAST LINE:", "LAST LINE: hello", 1);
        assert_eq!(
            progress("moving-around", &all, 0),
            Progress { total: 3, done: 3 }
        );
    }

    #[test]
    fn every_chapters_fresh_working_copy_is_zero_done() {
        for c in chapter::CHAPTERS {
            let p = progress(c.id, c.body(), 0);
            assert_eq!(p.done, 0, "{} should start at 0 done", c.id);
            assert!(p.total > 0, "{} should have at least one task", c.id);
        }
    }

    #[test]
    fn editing_basics_tasks() {
        let base = chapter::chapter("editing-basics").unwrap().body();
        let t1 = base.replacen("Hello,\n", "Hello, Ada\n", 1);
        assert_eq!(progress("editing-basics", &t1, 0).done, 1);
        let t2 = base.replacen("one REDUNDANT word", "one word", 1);
        assert_eq!(progress("editing-basics", &t2, 0).done, 1);
    }

    #[test]
    fn find_and_replace_tasks() {
        let base = chapter::chapter("find-and-replace").unwrap().body();
        let t1 = base.replace("cat", "dog");
        assert_eq!(progress("find-and-replace", &t1, 0).done, 1);
        let t2 = base.replacen(" teh ", " the ", 1);
        assert_eq!(progress("find-and-replace", &t2, 0).done, 1);
    }

    #[test]
    fn multi_cursor_and_selection_tasks() {
        let base = chapter::chapter("multi-cursor-and-selection")
            .unwrap()
            .body();
        let t1 = base
            .replacen("first line\n", "first line OK\n", 1)
            .replacen("second line\n", "second line OK\n", 1)
            .replacen("third line\n", "third line OK\n", 1);
        assert_eq!(progress("multi-cursor-and-selection", &t1, 0).done, 1);
        let t2 = base.replacen("REMOVE\nREMOVE\nREMOVE\n", "", 1);
        assert_eq!(progress("multi-cursor-and-selection", &t2, 0).done, 1);
    }

    #[test]
    fn files_tabs_and_palette_tasks() {
        let base = chapter::chapter("files-tabs-and-palette").unwrap().body();
        let t1 = base.replacen(">>>", ">>> CHECKED", 1);
        assert_eq!(progress("files-tabs-and-palette", &t1, 0).done, 1);
        let t2 = base.replacen(
            "Another file I opened:\n",
            "Another file I opened: notes.md\n",
            1,
        );
        assert_eq!(progress("files-tabs-and-palette", &t2, 0).done, 1);
    }

    #[test]
    fn git_basics_tasks() {
        let base = chapter::chapter("git-basics").unwrap().body();
        let t1 = base.replacen(
            "What the git status panel showed me:\n",
            "What the git status panel showed me: one modified file\n",
            1,
        );
        assert_eq!(progress("git-basics", &t1, 0).done, 1);
        let t2 = base.replacen(
            "Practiced staging a hunk:\n",
            "Practiced staging a hunk: yes\n",
            1,
        );
        assert_eq!(progress("git-basics", &t2, 0).done, 1);
    }
}
