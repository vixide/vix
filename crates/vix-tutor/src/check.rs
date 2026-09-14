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
        _ => Progress::default(),
    }
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
}
