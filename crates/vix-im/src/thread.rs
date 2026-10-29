//! Threads anchored to lines of a file, and re-finding their lines after
//! the file changes.
//!
//! A thread stores the text it is attached to and a few lines of context on
//! each side, so it can be found again after edits: first the anchored text
//! verbatim, nearest the old line; then the place where the most context
//! lines still match; otherwise the thread is outdated. Threads are stored
//! one per file as `.vix/im/threads/<id>.json`.

use serde::{Deserialize, Serialize};

use crate::Message;

/// How many context lines [`Thread::new`] keeps on each side.
pub const CONTEXT_LINES: usize = 3;

/// How far (in lines) the end of an edited anchor may move when it is
/// re-found by context.
const MAX_STRETCH: usize = 20;

/// A thread of messages attached to a line range in a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Thread {
    /// A file-name-safe id (see [`is_valid_id`]).
    pub id: String,
    /// The file, relative to the workspace root, with `/` separators.
    pub path: String,
    /// The anchored lines, joined with `'\n'`.
    pub anchor_text: String,
    /// Up to [`CONTEXT_LINES`] lines just above the anchor, top first.
    #[serde(default)]
    pub context_before: Vec<String>,
    /// Up to [`CONTEXT_LINES`] lines just below the anchor, top first.
    #[serde(default)]
    pub context_after: Vec<String>,
    /// The first anchored line, 1-based.
    pub start_line: u32,
    /// The last anchored line, 1-based, at least `start_line`.
    pub end_line: u32,
    /// Whether the thread has been resolved.
    #[serde(default)]
    pub resolved: bool,
    /// The conversation, oldest first.
    #[serde(default)]
    pub messages: Vec<Message>,
}

/// Where a thread's anchor is now. Lines are 1-based.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Anchor {
    /// The anchored text is unchanged and starts at this line (which may
    /// differ from the stored one when lines were added or removed above).
    Exact(u32),
    /// The anchored text changed, but its surrounding context places it at
    /// this line.
    Moved(u32),
    /// Neither the text nor its context could be found.
    Outdated,
}

impl Anchor {
    /// The line, unless the anchor is outdated.
    #[must_use]
    pub fn line(self) -> Option<u32> {
        match self {
            Anchor::Exact(line) | Anchor::Moved(line) => Some(line),
            Anchor::Outdated => None,
        }
    }
}

/// Whether `id` is safe as a file name: 1 to 64 ASCII letters, digits, `-`
/// or `_`.
#[must_use]
pub fn is_valid_id(id: &str) -> bool {
    (1..=64).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

impl Thread {
    /// A new, empty thread on `start_line..=end_line` (1-based) of
    /// `file_lines`, capturing the anchored text and its context. Lines past
    /// the end of the file are clamped; `end_line` before `start_line` is
    /// taken as `start_line`.
    #[must_use]
    pub fn new<S: AsRef<str>>(
        id: impl Into<String>,
        path: impl Into<String>,
        file_lines: &[S],
        start_line: u32,
        end_line: u32,
    ) -> Thread {
        let len = file_lines.len();
        let start = (to_index(start_line)).min(len.saturating_sub(1));
        let end = to_index(end_line).max(start).min(len.saturating_sub(1));
        let get = |range: std::ops::Range<usize>| -> Vec<String> {
            file_lines
                .get(range)
                .unwrap_or_default()
                .iter()
                .map(|l| l.as_ref().to_string())
                .collect()
        };
        let anchor = if len == 0 {
            Vec::new()
        } else {
            get(start..end + 1)
        };
        Thread {
            id: id.into(),
            path: path.into(),
            anchor_text: anchor.join("\n"),
            context_before: get(start.saturating_sub(CONTEXT_LINES)..start),
            context_after: get((end + 1).min(len)..(end + 1 + CONTEXT_LINES).min(len)),
            start_line: to_line(start),
            end_line: to_line(end),
            resolved: false,
            messages: Vec::new(),
        }
    }

    /// The thread as pretty-printed JSON, for `.vix/im/threads/<id>.json`.
    ///
    /// # Errors
    ///
    /// Only if serialization fails, which a well-formed thread never does.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Read a thread file. Its messages and path are sanitized, since the
    /// file may have arrived through Git from someone else. The anchor and
    /// context stay verbatim: they are compared with the file's own lines,
    /// and the App sanitizes them before drawing.
    ///
    /// # Errors
    ///
    /// When the text is not a thread in JSON.
    pub fn from_json(text: &str) -> Result<Thread, serde_json::Error> {
        let mut thread: Thread = serde_json::from_str(text)?;
        for message in &mut thread.messages {
            message.sanitize();
        }
        thread.path = crate::sanitize::sanitize(&thread.path);
        Ok(thread)
    }

    /// The thread's file name: `<id>.json`, or `None` for an unsafe id.
    #[must_use]
    pub fn file_name(&self) -> Option<String> {
        is_valid_id(&self.id).then(|| format!("{}.json", self.id))
    }

    /// Add a reply (sanitized).
    pub fn reply(&mut self, mut message: Message) {
        message.sanitize();
        self.messages.push(message);
    }

    /// Move the stored range to where `anchor` found it, keeping its length.
    /// An outdated anchor changes nothing.
    pub fn apply(&mut self, anchor: Anchor) {
        if let Some(line) = anchor.line() {
            let span = self.end_line.saturating_sub(self.start_line);
            self.start_line = line;
            self.end_line = line.saturating_add(span);
        }
    }

    fn anchor_lines(&self) -> Vec<&str> {
        self.anchor_text.split('\n').collect()
    }
}

fn to_index(line: u32) -> usize {
    usize::try_from(line.max(1) - 1).unwrap_or(usize::MAX)
}

fn to_line(index: usize) -> u32 {
    u32::try_from(index).map_or(u32::MAX, |i| i.saturating_add(1))
}

/// Where `thread`'s anchor is in the file's current lines.
///
/// 1. The anchored lines verbatim (ignoring trailing whitespace), the
///    occurrence nearest the stored start line: [`Anchor::Exact`].
/// 2. Otherwise, the start line whose surrounding lines best match the
///    stored context (non-blank lines only; the end may have moved by a few
///    lines if the anchored text grew or shrank), needing at least two
///    matching lines, or one when only one non-blank context line was
///    stored; ties go to the nearest: [`Anchor::Moved`].
/// 3. Otherwise [`Anchor::Outdated`].
#[must_use]
pub fn reanchor<S: AsRef<str>>(thread: &Thread, file_lines: &[S]) -> Anchor {
    let lines: Vec<&str> = file_lines.iter().map(|l| l.as_ref().trim_end()).collect();
    let anchor: Vec<&str> = thread.anchor_lines().iter().map(|l| l.trim_end()).collect();
    let old = to_index(thread.start_line);
    let n = anchor.len();
    if n <= lines.len()
        && let Some(at) = nearest(
            (0..=lines.len() - n).filter(|&i| lines[i..i + n] == anchor[..]),
            old,
        )
    {
        return Anchor::Exact(to_line(at));
    }
    by_context(thread, &lines, n, old).map_or(Anchor::Outdated, |at| Anchor::Moved(to_line(at)))
}

/// The candidate closest to `old`, earlier first on a tie.
fn nearest(candidates: impl Iterator<Item = usize>, old: usize) -> Option<usize> {
    candidates.min_by_key(|&i| (i.abs_diff(old), i))
}

fn by_context(thread: &Thread, lines: &[&str], n: usize, old: usize) -> Option<usize> {
    let before: Vec<&str> = thread.context_before.iter().map(|l| l.trim_end()).collect();
    let after: Vec<&str> = thread.context_after.iter().map(|l| l.trim_end()).collect();
    let available = before
        .iter()
        .chain(after.iter())
        .filter(|l| !l.trim().is_empty())
        .count();
    if available == 0 {
        return None;
    }
    let needed = available.min(2);
    let stretch = MAX_STRETCH;
    let mut best: Option<(usize, usize)> = None; // (score, start)
    for start in 0..=lines.len() {
        let above = matching(lines, &before, start.checked_sub(before.len()));
        let below = (n.saturating_sub(stretch)..=n + stretch)
            .map(|len| matching(lines, &after, Some(start + len)))
            .max()
            .unwrap_or(0);
        let score = above + below;
        if score < needed || start >= lines.len() {
            continue;
        }
        let better = best.is_none_or(|(s, at)| {
            score > s || (score == s && start.abs_diff(old) < at.abs_diff(old))
        });
        if better {
            best = Some((score, start));
        }
    }
    best.map(|(_, at)| at)
}

/// How many non-blank `context` lines match `lines` placed from `from`.
/// Context partly above the top of the file (`from` = `None`) matches
/// nothing.
fn matching(lines: &[&str], context: &[&str], from: Option<usize>) -> usize {
    let Some(from) = from else {
        return 0;
    };
    context
        .iter()
        .enumerate()
        .filter(|&(k, want)| !want.trim().is_empty() && lines.get(from + k) == Some(want))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &[&str] = &[
        "fn main() {",          // 1
        "    let a = 1;",       // 2
        "    let b = 2;",       // 3
        "    println!(a + b);", // 4  <- anchored
        "    let c = 3;",       // 5
        "    drop(c);",         // 6
        "}",                    // 7
    ];

    fn thread() -> Thread {
        Thread::new("t1", "src/main.rs", FILE, 4, 4)
    }

    #[test]
    fn captures_anchor_and_context() {
        let t = thread();
        assert_eq!(t.anchor_text, "    println!(a + b);");
        assert_eq!(t.context_before, FILE[0..3]);
        assert_eq!(t.context_after, FILE[4..7]);
        assert_eq!((t.start_line, t.end_line), (4, 4));
        let edge = Thread::new("t2", "a", FILE, 1, 99);
        assert_eq!((edge.start_line, edge.end_line), (1, 7));
        assert!(edge.context_before.is_empty() && edge.context_after.is_empty());
    }

    #[test]
    fn unchanged_file_is_exact() {
        assert_eq!(reanchor(&thread(), FILE), Anchor::Exact(4));
    }

    #[test]
    fn edit_above_moves_exact() {
        let mut lines = FILE.to_vec();
        lines.insert(1, "    // a comment");
        lines.insert(1, "    // another");
        assert_eq!(reanchor(&thread(), &lines), Anchor::Exact(6));
        lines.drain(0..4);
        assert_eq!(reanchor(&thread(), &lines), Anchor::Exact(2));
    }

    #[test]
    fn edit_below_keeps_exact() {
        let mut lines = FILE.to_vec();
        lines.insert(5, "    extra();");
        lines.pop();
        assert_eq!(reanchor(&thread(), &lines), Anchor::Exact(4));
    }

    #[test]
    fn nearest_duplicate_wins() {
        let mut lines = FILE.to_vec();
        lines.insert(0, "    println!(a + b);");
        assert_eq!(reanchor(&thread(), &lines), Anchor::Exact(5));
    }

    #[test]
    fn edit_inside_found_by_context() {
        let mut lines = FILE.to_vec();
        lines[3] = "    println!(\"{}\", a + b);";
        assert_eq!(reanchor(&thread(), &lines), Anchor::Moved(4));
        lines.insert(0, "// header");
        assert_eq!(reanchor(&thread(), &lines), Anchor::Moved(5));
        // The anchored text grows by two lines.
        lines.insert(5, "    // why");
        lines.insert(5, "    // more");
        assert_eq!(reanchor(&thread(), &lines), Anchor::Moved(5));
    }

    #[test]
    fn rewritten_file_is_outdated() {
        let lines = ["completely", "different", "content"];
        assert_eq!(reanchor(&thread(), &lines), Anchor::Outdated);
        assert_eq!(reanchor(&thread(), &[] as &[&str]), Anchor::Outdated);
    }

    #[test]
    fn apply_moves_range() {
        let mut t = Thread::new("t", "a", FILE, 2, 3);
        t.apply(Anchor::Exact(5));
        assert_eq!((t.start_line, t.end_line), (5, 6));
        t.apply(Anchor::Outdated);
        assert_eq!((t.start_line, t.end_line), (5, 6));
    }

    #[test]
    fn json_round_trip_and_tolerance() {
        let mut t = thread();
        t.reply(crate::Message::text(
            "ana",
            1_700_000_000_000,
            "why \u{1b}[31mthis?",
        ));
        let json = t.to_json().unwrap();
        let back = Thread::from_json(&json).unwrap();
        assert_eq!(back, t);
        assert_eq!(back.messages[0].body, "why this?");
        // Minimal file from another tool: optional fields default, unknown
        // fields are ignored.
        let minimal = r#"{"id":"x","path":"a.rs","anchor_text":"a","start_line":1,
            "end_line":1,"line_comment_extra":true}"#;
        let t = Thread::from_json(minimal).unwrap();
        assert!(!t.resolved && t.messages.is_empty());
        assert!(Thread::from_json("{").is_err());
    }

    #[test]
    fn ids() {
        assert!(is_valid_id("t-01_ab"));
        assert!(!is_valid_id(""));
        assert!(!is_valid_id("../x"));
        assert!(!is_valid_id("a.b"));
        assert_eq!(thread().file_name().as_deref(), Some("t1.json"));
    }
}
