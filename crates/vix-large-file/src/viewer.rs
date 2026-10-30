//! [`Viewer`]: the Large File Viewer's state and key handling.

use std::io;
use std::path::{Path, PathBuf};

use vix_input::{KeyCode, KeyEvent, KeyModifiers};

use crate::index::{LineIndex, Lookup, percent_of};
use crate::search::{Direction, Found, Search};
use crate::source::{LineSpan, Source};

/// The most bytes one copy puts on the clipboard (16 MiB).
pub const COPY_LIMIT: u64 = 16 * 1024 * 1024;

/// How many columns ←/→ scroll by.
const H_STEP: u64 = 8;

/// What the host should do after the viewer handled a key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Handled; nothing further to do.
    Consumed,
    /// Close the viewer.
    Close,
    /// Put this text on the clipboard.
    Copy(String),
}

/// Which one-line input the viewer is reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    /// A goto target (see [`parse_target`]).
    Goto,
    /// Text to search forward for.
    SearchForward,
    /// Text to search backward for.
    SearchBackward,
}

/// A transient message for the host to show (translated there).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
    /// The search found nothing.
    NotFound,
    /// The search was cancelled.
    SearchCancelled,
    /// The goto line is not indexed yet; it is applied once it is (1-based).
    GotoPending(u64),
    /// The goto input did not parse.
    BadGoto,
    /// The file has only this many lines.
    BeyondEnd(u64),
    /// This many bytes were copied.
    Copied(u64),
    /// The selection is larger than [`COPY_LIMIT`].
    CopyTooLarge,
    /// A read failed (the error's text).
    ReadFailed(String),
    /// The line index stopped on a read error.
    IndexFailed,
}

/// Where a goto goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// A 1-based line number.
    Line(u64),
    /// A position through the file in hundredths of a percent (0–10,000).
    Percent(u64),
    /// A byte offset (the line holding it).
    Byte(u64),
}

/// Parse a goto input: `1234` is a line, `50%` or `12.5%` a percentage,
/// `@1234` or `0x4d2` a byte offset. Digit-group separators (`_`, `,`) and
/// surrounding spaces are ignored.
#[must_use]
pub fn parse_target(input: &str) -> Option<Target> {
    let s: String = input
        .trim()
        .chars()
        .filter(|c| !matches!(c, '_' | ','))
        .collect();
    if let Some(p) = s.strip_suffix('%') {
        let (whole, frac) = p.trim().split_once('.').unwrap_or((p.trim(), ""));
        let whole: u64 = whole.parse().ok()?;
        let frac = format!("{frac:0<2}");
        let frac: u64 = if frac.len() == 2 {
            frac.parse().ok()?
        } else {
            return None;
        };
        let bp = whole.checked_mul(100)?.checked_add(frac)?;
        return (bp <= 10_000).then_some(Target::Percent(bp));
    }
    if let Some(b) = s.strip_prefix('@') {
        return b.trim().parse().ok().map(Target::Byte);
    }
    if let Some(h) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        return u64::from_str_radix(h, 16).ok().map(Target::Byte);
    }
    s.parse().ok().map(Target::Line)
}

/// One rendered row of the viewer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewLine {
    /// Byte offset of the line's start.
    pub offset: u64,
    /// The visible text: decoded lossily (invalid UTF-8 shows as U+FFFD), a
    /// CRLF's `\r` trimmed, tabs and other control characters shown as one
    /// placeholder column each, cut to the view's width.
    pub text: String,
    /// The cursor's line.
    pub cursor: bool,
    /// Inside the selection.
    pub selected: bool,
    /// The search match on this line, as a char range within `text`.
    pub hit: Option<(usize, usize)>,
    /// The line continues past the right edge.
    pub more_right: bool,
}

/// What the status line shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    /// The file's length in bytes.
    pub len: u64,
    /// The cursor line's byte offset.
    pub offset: u64,
    /// The cursor's position through the file, 0–100.
    pub percent: u8,
    /// The cursor's 1-based line number, once the index has reached it.
    pub line: Option<u64>,
    /// The file's line count, once fully indexed.
    pub total_lines: Option<u64>,
    /// How much of the file is indexed, 0–100.
    pub indexed: u8,
    /// A running search's progress, 0–100.
    pub searching: Option<u8>,
    /// A selection is being made.
    pub selecting: bool,
    /// The horizontal scroll, in bytes (text is hidden to the left of every
    /// line when it is not 0).
    pub hscroll: u64,
}

/// The Large File Viewer: a read-only window onto one file.
#[derive(Debug)]
pub struct Viewer {
    path: PathBuf,
    src: Source,
    index: LineIndex,
    /// Start of the top visible line.
    top: u64,
    /// Start of the cursor's line.
    cursor: u64,
    /// Start of the line a selection was begun on.
    anchor: Option<u64>,
    /// Bytes hidden to the left of every line.
    hscroll: u64,
    input: Option<(InputKind, String)>,
    search: Option<Search>,
    /// The last search's text and direction, for `n`/`N`.
    last_search: Option<(String, Direction)>,
    /// The last match: offset and length.
    hit: Option<(u64, u64)>,
    /// A 0-based goto line waiting for the index.
    pending_goto: Option<u64>,
    notice: Option<Notice>,
    index_failure_reported: bool,
    /// The view's size at the last render, for actions that need it.
    height: usize,
    width: usize,
    /// (cursor, 0-based line) from the last lookup.
    line_cache: Option<(u64, u64)>,
}

impl Viewer {
    /// Open `path` and start indexing it in the background. Reads only the
    /// first screen's worth of bytes before returning.
    ///
    /// # Errors
    ///
    /// The open error; `InvalidInput` for a non-regular file.
    pub fn open(path: &Path) -> io::Result<Self> {
        let src = Source::open(path)?;
        let index = LineIndex::build(path, src.len())?;
        Ok(Self {
            path: path.to_path_buf(),
            src,
            index,
            top: 0,
            cursor: 0,
            anchor: None,
            hscroll: 0,
            input: None,
            search: None,
            last_search: None,
            hit: None,
            pending_goto: None,
            notice: None,
            index_failure_reported: false,
            height: 24,
            width: 80,
            line_cache: None,
        })
    }

    /// The viewed file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The file's line index (for its progress, or to wait on it).
    #[must_use]
    pub fn index(&self) -> &LineIndex {
        &self.index
    }

    /// Mutable access to the line index (to [`LineIndex::wait`] on it).
    pub fn index_mut(&mut self) -> &mut LineIndex {
        &mut self.index
    }

    /// The cursor line's byte offset.
    #[must_use]
    pub fn cursor(&self) -> u64 {
        self.cursor
    }

    /// The top visible line's byte offset.
    #[must_use]
    pub fn top(&self) -> u64 {
        self.top
    }

    /// The input being typed, if any.
    #[must_use]
    pub fn input(&self) -> Option<(InputKind, &str)> {
        self.input.as_ref().map(|(k, s)| (*k, s.as_str()))
    }

    /// The current notice, if any.
    #[must_use]
    pub fn notice(&self) -> Option<&Notice> {
        self.notice.as_ref()
    }

    /// Whether background work is in flight (the host should redraw often).
    #[must_use]
    pub fn busy(&self) -> bool {
        self.search.is_some() || (!self.index.is_done() && !self.index.failed())
    }

    /// The running search, if any.
    #[must_use]
    pub fn search(&self) -> Option<&Search> {
        self.search.as_ref()
    }

    /// Run `f`, turning a read error into a notice.
    fn attempt(&mut self, f: impl FnOnce(&mut Self) -> io::Result<()>) {
        if let Err(e) = f(self) {
            self.notice = Some(Notice::ReadFailed(e.to_string()));
        }
    }

    /// Route a key. `height` is the number of text rows on screen.
    pub fn handle_key(&mut self, key: KeyEvent, height: usize) -> Outcome {
        self.height = height.max(1);
        if self.input.is_some() {
            self.input_key(key);
            return Outcome::Consumed;
        }
        self.notice = None;
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        match key.code {
            KeyCode::Esc => {
                if let Some(s) = self.search.take() {
                    s.cancel();
                    self.notice = Some(Notice::SearchCancelled);
                } else if self.anchor.is_some() {
                    self.anchor = None;
                } else {
                    return Outcome::Close;
                }
            }
            KeyCode::Char('q') if !ctrl => return Outcome::Close,
            KeyCode::Char('c') if ctrl => return self.copy(),
            KeyCode::Char('c' | 'y') => return self.copy(),
            KeyCode::Char('g') if ctrl => self.begin_input(InputKind::Goto),
            KeyCode::Char(':') => self.begin_input(InputKind::Goto),
            KeyCode::Char('/') => self.begin_input(InputKind::SearchForward),
            KeyCode::Char('?') => self.begin_input(InputKind::SearchBackward),
            KeyCode::Char('n') => self.repeat_search(false),
            KeyCode::Char('N') => self.repeat_search(true),
            KeyCode::Char('m' | ' ') => {
                self.anchor = if self.anchor.is_some() {
                    None
                } else {
                    Some(self.cursor)
                };
            }
            _ => self.move_key(key.code, shift),
        }
        Outcome::Consumed
    }

    /// Cursor and scroll keys. Shift+↑/↓ extend a selection.
    fn move_key(&mut self, code: KeyCode, shift: bool) {
        let page = self.height.saturating_sub(1).max(1) as u64;
        if shift && matches!(code, KeyCode::Up | KeyCode::Down) && self.anchor.is_none() {
            self.anchor = Some(self.cursor);
        }
        self.attempt(|v| {
            match code {
                KeyCode::Down | KeyCode::Char('j') => v.cursor = v.src.forward(v.cursor, 1)?,
                KeyCode::Up | KeyCode::Char('k') => v.cursor = v.src.back(v.cursor, 1)?,
                KeyCode::PageDown => {
                    v.cursor = v.src.forward(v.cursor, page)?;
                    // Don't scroll the last line above the bottom row.
                    let last_top = v.src.back(v.src.last_line_start()?, page)?;
                    v.top = v.src.forward(v.top, page)?.min(last_top.max(v.top));
                }
                KeyCode::PageUp => {
                    v.cursor = v.src.back(v.cursor, page)?;
                    v.top = v.src.back(v.top, page)?;
                }
                KeyCode::Home | KeyCode::Char('g') => {
                    v.cursor = 0;
                    v.top = 0;
                }
                KeyCode::End | KeyCode::Char('G') => v.goto_end()?,
                KeyCode::Left | KeyCode::Char('h') => {
                    v.hscroll = v.hscroll.saturating_sub(H_STEP);
                }
                KeyCode::Right | KeyCode::Char('l') => v.hscroll += H_STEP,
                KeyCode::Char('0') => v.hscroll = 0,
                _ => return Ok(()),
            }
            v.ensure_visible()
        });
    }

    /// Jump to the last line, filling the screen above it.
    fn goto_end(&mut self) -> io::Result<()> {
        self.cursor = self.src.last_line_start()?;
        self.top = self
            .src
            .back(self.cursor, self.height.saturating_sub(1) as u64)?;
        Ok(())
    }

    fn begin_input(&mut self, kind: InputKind) {
        self.input = Some((kind, String::new()));
    }

    /// A key while an input line is open.
    fn input_key(&mut self, key: KeyEvent) {
        let Some((kind, text)) = self.input.as_mut() else {
            return;
        };
        match key.code {
            KeyCode::Esc => self.input = None,
            KeyCode::Backspace => {
                text.pop();
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => text.push(c),
            KeyCode::Enter => {
                let kind = *kind;
                let text = std::mem::take(text);
                self.input = None;
                match kind {
                    InputKind::Goto => match parse_target(&text) {
                        Some(t) => self.goto(t),
                        None => self.notice = Some(Notice::BadGoto),
                    },
                    InputKind::SearchForward => self.start_search(text, Direction::Forward),
                    InputKind::SearchBackward => self.start_search(text, Direction::Backward),
                }
            }
            _ => {}
        }
    }

    /// Go to `target`. A line the index has not reached yet is remembered and
    /// applied by [`Viewer::poll`] once it has.
    pub fn goto(&mut self, target: Target) {
        self.notice = None;
        self.pending_goto = None;
        let len = self.src.len();
        self.attempt(|v| match target {
            Target::Line(n) => v.goto_line(n.saturating_sub(1)),
            Target::Percent(bp) => {
                let byte = u64::try_from(u128::from(len) * u128::from(bp) / 10_000).unwrap_or(len);
                v.goto_byte(byte)
            }
            Target::Byte(b) => v.goto_byte(b),
        });
    }

    /// Go to 0-based `line`, or remember it until the index reaches it.
    fn goto_line(&mut self, line: u64) -> io::Result<()> {
        match self.index.line_start(&self.src, line)? {
            Lookup::Found(off) => self.jump_to(off),
            Lookup::Pending => {
                self.pending_goto = Some(line);
                self.notice = Some(Notice::GotoPending(line + 1));
                Ok(())
            }
            Lookup::BeyondEnd => {
                self.notice = Some(Notice::BeyondEnd(self.index.total_lines().unwrap_or(0)));
                Ok(())
            }
        }
    }

    /// Go to the line holding byte `byte`.
    fn goto_byte(&mut self, byte: u64) -> io::Result<()> {
        let len = self.src.len();
        let off = if byte >= len {
            self.src.last_line_start()?
        } else {
            self.src.line_start_containing(byte)?
        };
        self.jump_to(off)
    }

    /// Put the cursor on the line starting at `off`, a third of the way down.
    fn jump_to(&mut self, off: u64) -> io::Result<()> {
        self.cursor = off;
        self.top = self.src.back(off, (self.height / 3) as u64)?;
        Ok(())
    }

    /// Scroll so the cursor's line is on screen: by a line or two when it
    /// just left the screen, otherwise by recentering on it.
    fn ensure_visible(&mut self) -> io::Result<()> {
        let h = self.height.max(1);
        if self.cursor < self.top {
            if self.src.back(self.top, h as u64)? <= self.cursor {
                self.top = self.cursor;
                return Ok(());
            }
            return self.jump_to(self.cursor);
        }
        let ahead = self.src.lines_from(self.top, 2 * h)?;
        match ahead.iter().position(|s| s.start == self.cursor) {
            Some(i) if i < h => Ok(()),
            Some(i) => {
                self.top = ahead[i + 1 - h].start;
                Ok(())
            }
            None => self.jump_to(self.cursor),
        }
    }

    /// Start a search for `text` from the cursor's line (or just past the
    /// last match, so repeating finds the next one).
    fn start_search(&mut self, text: String, direction: Direction) {
        if text.is_empty() {
            return;
        }
        if let Some(s) = self.search.take() {
            s.cancel();
        }
        // Past (or before) the last match when it is on the cursor's line;
        // otherwise from the cursor's line: its start going forward, its end
        // going backward, so a match on that line is found either way.
        let hit_here = self
            .hit
            .filter(|(at, _)| self.src.line_start_containing(*at).ok() == Some(self.cursor));
        let from = match (direction, hit_here) {
            (Direction::Forward, Some((at, _))) => at + 1,
            (Direction::Backward, Some((at, _))) => at,
            (Direction::Forward, None) => self.cursor,
            (Direction::Backward, None) => self
                .src
                .forward_exact(self.cursor, 1)
                .ok()
                .flatten()
                .unwrap_or(self.src.len()),
        };
        match Search::start(&self.path, self.src.len(), text.as_bytes(), from, direction) {
            Ok(s) => self.search = Some(s),
            Err(e) => self.notice = Some(Notice::ReadFailed(e.to_string())),
        }
        self.last_search = Some((text, direction));
    }

    /// `n` / `N`: repeat the last search, the same way or the other way.
    fn repeat_search(&mut self, reverse: bool) {
        let Some((text, dir)) = self.last_search.clone() else {
            return;
        };
        let dir = match (dir, reverse) {
            (d, false) => d,
            (Direction::Forward, true) => Direction::Backward,
            (Direction::Backward, true) => Direction::Forward,
        };
        self.start_search(text, dir);
        // Keep `n` meaning the original direction.
        if let Some(last) = self.last_search.as_mut()
            && reverse
        {
            last.1 = match dir {
                Direction::Forward => Direction::Backward,
                Direction::Backward => Direction::Forward,
            };
        }
    }

    /// Copy the selected lines (or the cursor's line).
    fn copy(&mut self) -> Outcome {
        let mut text = None;
        self.attempt(|v| {
            let a = v.anchor.unwrap_or(v.cursor);
            let (lo, hi) = (a.min(v.cursor), a.max(v.cursor));
            let end = v.src.forward_exact(hi, 1)?.unwrap_or(v.src.len());
            if end - lo > COPY_LIMIT {
                v.notice = Some(Notice::CopyTooLarge);
                return Ok(());
            }
            let bytes = v.src.read(lo, end - lo)?;
            v.notice = Some(Notice::Copied(bytes.len() as u64));
            v.anchor = None;
            text = Some(String::from_utf8_lossy(&bytes).into_owned());
            Ok(())
        });
        text.map_or(Outcome::Consumed, Outcome::Copy)
    }

    /// Collect finished background work: a search result, a goto line the
    /// index has now reached, an index failure. Returns whether anything
    /// changed.
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        if let Some(found) = self.search.as_ref().and_then(Search::poll) {
            let len = self.search.take().map_or(0, |s| s.needle_len());
            self.finish_search(found, len);
            changed = true;
        }
        if let Some(line) = self.pending_goto {
            match self.index.line_start(&self.src, line) {
                Ok(Lookup::Found(off)) => {
                    self.pending_goto = None;
                    self.notice = None;
                    self.attempt(|v| v.jump_to(off));
                    changed = true;
                }
                Ok(Lookup::BeyondEnd) => {
                    self.pending_goto = None;
                    self.notice = Some(Notice::BeyondEnd(self.index.total_lines().unwrap_or(0)));
                    changed = true;
                }
                Ok(Lookup::Pending) => {}
                Err(e) => {
                    self.pending_goto = None;
                    self.notice = Some(Notice::ReadFailed(e.to_string()));
                    changed = true;
                }
            }
        }
        if self.index.failed() && !self.index_failure_reported {
            self.index_failure_reported = true;
            self.notice = Some(Notice::IndexFailed);
            changed = true;
        }
        changed
    }

    /// Act on a finished search.
    fn finish_search(&mut self, found: Found, needle_len: u64) {
        match found {
            Found::At(at) => {
                self.hit = Some((at, needle_len));
                self.attempt(|v| {
                    let start = v.src.line_start_containing(at)?;
                    v.jump_to(start)?;
                    let col = at - start;
                    let w = v.width.max(1) as u64;
                    if col < v.hscroll || col + needle_len > v.hscroll + w {
                        v.hscroll = col.saturating_sub(w / 4);
                    }
                    Ok(())
                });
            }
            Found::NotFound => self.notice = Some(Notice::NotFound),
            Found::Cancelled => self.notice = Some(Notice::SearchCancelled),
            Found::Failed(e) => self.notice = Some(Notice::ReadFailed(e)),
        }
    }

    /// The rows to draw in a `height` × `width` view. Reads only the visible
    /// lines, and of each only the bytes that fit.
    pub fn view(&mut self, height: usize, width: usize) -> Vec<ViewLine> {
        self.height = height.max(1);
        self.width = width.max(1);
        let mut rows = Vec::new();
        self.attempt(|v| {
            v.ensure_visible()?;
            for span in v.src.lines_from(v.top, v.height)? {
                rows.push(v.render(span)?);
            }
            Ok(())
        });
        rows
    }

    /// One line, cut to the view.
    fn render(&self, span: LineSpan) -> io::Result<ViewLine> {
        let a = self.anchor.unwrap_or(self.cursor);
        let selected = self.anchor.is_some()
            && span.start >= a.min(self.cursor)
            && span.start <= a.max(self.cursor);
        let mut line = ViewLine {
            offset: span.start,
            text: String::new(),
            cursor: span.start == self.cursor,
            selected,
            hit: None,
            more_right: false,
        };
        let mut from = span.start.saturating_add(self.hscroll);
        if from >= span.end {
            return Ok(line);
        }
        // Up to 4 bytes a column, plus a few to spare for skipped bytes.
        let want = self.width as u64 * 4 + 4;
        let to = span.end.min(from + want);
        let mut bytes = self.src.read(from, to - from)?;
        if to == span.end && bytes.last() == Some(&b'\r') {
            bytes.pop();
        }
        // Don't start mid-character after a horizontal scroll.
        let skip = if self.hscroll > 0 {
            bytes
                .iter()
                .take(3)
                .take_while(|b| (**b & 0xC0) == 0x80)
                .count()
        } else {
            0
        };
        from += skip as u64;
        let bytes = &bytes[skip..];
        let shown: String = String::from_utf8_lossy(bytes)
            .chars()
            .map(|c| if c.is_control() { '·' } else { c })
            .collect();
        let count = shown.chars().count();
        line.more_right = count > self.width || to < span.end;
        line.text = shown.chars().take(self.width).collect();
        if let Some((at, n)) = self.hit
            && at >= from
            && at < from + bytes.len() as u64
        {
            let rel = usize::try_from(at - from).unwrap_or(0);
            let end = rel
                .saturating_add(usize::try_from(n).unwrap_or(0))
                .min(bytes.len());
            let start_col = String::from_utf8_lossy(&bytes[..rel]).chars().count();
            let end_col = String::from_utf8_lossy(&bytes[..end]).chars().count();
            if start_col < self.width {
                line.hit = Some((start_col, end_col.min(self.width)));
            }
        }
        Ok(line)
    }

    /// The status line's numbers.
    pub fn status(&mut self) -> Status {
        let line = match self.line_cache {
            Some((at, line)) if at == self.cursor => Some(line),
            _ => match self.index.line_of(&self.src, self.cursor) {
                Ok(Some(line)) => {
                    self.line_cache = Some((self.cursor, line));
                    Some(line)
                }
                _ => None,
            },
        };
        Status {
            len: self.src.len(),
            offset: self.cursor,
            percent: percent_of(self.cursor, self.src.len().saturating_sub(1).max(1)),
            line: line.map(|l| l + 1),
            total_lines: self.index.total_lines(),
            indexed: self.index.percent(),
            searching: self.search.as_ref().map(Search::percent),
            selecting: self.anchor.is_some(),
            hscroll: self.hscroll,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_goto_targets() {
        assert_eq!(parse_target("42"), Some(Target::Line(42)));
        assert_eq!(parse_target(" 1,000 "), Some(Target::Line(1000)));
        assert_eq!(parse_target("50%"), Some(Target::Percent(5000)));
        assert_eq!(parse_target("12.5%"), Some(Target::Percent(1250)));
        assert_eq!(parse_target("100%"), Some(Target::Percent(10_000)));
        assert_eq!(parse_target("101%"), None);
        assert_eq!(parse_target("1.234%"), None);
        assert_eq!(parse_target("@1024"), Some(Target::Byte(1024)));
        assert_eq!(parse_target("0x400"), Some(Target::Byte(1024)));
        assert_eq!(parse_target("nope"), None);
        assert_eq!(parse_target(""), None);
    }
}
