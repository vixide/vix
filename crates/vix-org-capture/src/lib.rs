#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(clippy::pedantic)]

//! Org-capture: template-driven capture, in the shape of Emacs
//! `org-capture-templates` — a template with `%^{Prompt}`-style placeholders,
//! wrapped as a headline/item/checkbox/table-row, and filed at a target
//! location (the cursor, a node by `:ID:`, a file, a headline within a file,
//! or a date-tree). See `crates/vix-org-capture/spec/index.md` for the design.
//!
//! This crate is pure: it has no notion of "now", the active editor, or the
//! filesystem. The host collects prompt answers, builds a [`Context`], reads
//! and writes files, and calls into these functions to do the text shaping.

use serde::{Deserialize, Serialize};

// ----- Template shape --------------------------------------------------------

/// How the expanded template is wrapped when inserted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EntryType {
    /// A headline (`* ...`); the default.
    #[default]
    Entry,
    /// Raw text, inserted verbatim with no wrapper.
    Plain,
    /// A plain list item (`- ...`).
    Item,
    /// A checkbox list item (`- [ ] ...`).
    CheckItem,
    /// A table row, appended as `| ... |`.
    TableLine,
}

/// Where a captured entry is filed, parsed from a template's `target` string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// Insert at the cursor in the active buffer (the default).
    Cursor,
    /// File under the headline/node carrying `:ID: <id>` (`id:<ID>`).
    Id(String),
    /// Append to the end of a file (`file:<path>`).
    File(String),
    /// File under a specific headline in a file, creating it if absent
    /// (`file+headline:<path>#<Headline>`).
    FileHeadline(String, String),
    /// File under today's `Year > Month > Day` outline tree in a file
    /// (`file+datetree:<path>`).
    FileDatetree(String),
}

impl Target {
    /// Parse a template's `target` string. Anything unrecognized (including
    /// `"cursor"` and the empty string) is [`Target::Cursor`].
    #[must_use]
    pub fn parse(raw: &str) -> Target {
        let raw = raw.trim();
        if let Some(id) = raw.strip_prefix("id:") {
            return Target::Id(id.trim().to_string());
        }
        if let Some(rest) = raw.strip_prefix("file+headline:") {
            return rest.split_once('#').map_or_else(
                || Target::File(rest.trim().to_string()),
                |(path, headline)| {
                    Target::FileHeadline(path.trim().to_string(), headline.trim().to_string())
                },
            );
        }
        if let Some(path) = raw.strip_prefix("file+datetree:") {
            return Target::FileDatetree(path.trim().to_string());
        }
        if let Some(path) = raw.strip_prefix("file:") {
            return Target::File(path.trim().to_string());
        }
        Target::Cursor
    }
}

/// A user-configured capture template (an entry in the `org_capture_templates`
/// setting).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureTemplate {
    /// Single-character (by convention) key identifying this template.
    pub key: String,
    /// Human-readable label shown in the capture menu/chooser.
    pub description: String,
    /// How the expanded template is wrapped.
    #[serde(default)]
    pub entry_type: EntryType,
    /// Where the entry is filed; see [`Target::parse`].
    #[serde(default = "default_target")]
    pub target: String,
    /// The template body: literal text plus placeholders (`%^{Prompt}`, `%t`,
    /// `%a`, …).
    pub template: String,
    /// Insert at the top of the target (headline/file) instead of the bottom.
    #[serde(default)]
    pub prepend: bool,
    /// Blank lines to pad before and after the inserted entry.
    #[serde(default)]
    pub empty_lines: u8,
    /// Skip the review step and file immediately once every `%^{}` prompt
    /// (and the tag prompt, if any) is answered.
    #[serde(default)]
    pub immediate_finish: bool,
    /// Start a `CLOCK:` entry on the newly captured headline.
    #[serde(default)]
    pub clock_in: bool,
}

fn default_target() -> String {
    "cursor".to_string()
}

// ----- Placeholder extraction ------------------------------------------------

/// One `%^{Label}` (optionally `%^{Label|Default}`) field prompt, in the order
/// it appears in the template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldPrompt {
    /// The prompt's label (shown as the prompt title).
    pub label: String,
    /// The prompt's pre-filled default text (empty if none given) — the
    /// first entry of `choices`, if any.
    pub default: String,
    /// The full `|`-delimited candidate list after the label, in order, e.g.
    /// `%^{Priority|0|0|1|2}` yields `["0", "0", "1", "2"]`. Empty when the
    /// template gave no `|choices` at all (a plain free-text prompt).
    pub choices: Vec<String>,
}

/// Every `%^{...}` field prompt in `template`, in order.
#[must_use]
pub fn extract_prompts(template: &str) -> Vec<FieldPrompt> {
    let chars: Vec<char> = template.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '%'
            && chars.get(i + 1) == Some(&'^')
            && chars.get(i + 2) == Some(&'{')
            && let Some(close) = find_char(&chars, i + 3, '}')
        {
            let inner: String = chars[i + 3..close].iter().collect();
            let mut parts = inner.split('|');
            let label = parts.next().unwrap_or_default().to_string();
            let choices: Vec<String> = parts.map(str::to_string).collect();
            let default = choices.first().cloned().unwrap_or_default();
            out.push(FieldPrompt {
                label,
                default,
                choices,
            });
            i = close + 1;
            continue;
        }
        i += 1;
    }
    out
}

/// Whether `template` contains a `%^g`/`%^G` tag prompt.
#[must_use]
pub fn wants_tags(template: &str) -> bool {
    let chars: Vec<char> = template.chars().collect();
    chars
        .windows(3)
        .any(|w| w[0] == '%' && w[1] == '^' && (w[2] == 'g' || w[2] == 'G'))
}

fn find_char(chars: &[char], from: usize, target: char) -> Option<usize> {
    (from..chars.len()).find(|&i| chars[i] == target)
}

// ----- Expansion --------------------------------------------------------------

/// Point-in-time values available for placeholder expansion, built by the host
/// from the active editor state and the current date/time.
#[derive(Debug, Clone, Default)]
pub struct Context {
    /// Four-digit year.
    pub year: i32,
    /// Month, 1-12.
    pub month: u32,
    /// Day of month, 1-31.
    pub day: u32,
    /// Hour, 0-23.
    pub hour: u32,
    /// Minute, 0-59.
    pub minute: u32,
    /// Short weekday name (`"Mon"`..`"Sun"`).
    pub weekday: String,
    /// `%a`: a link back to where capture was invoked.
    pub annotation: String,
    /// `%i`: the active selection, if any.
    pub initial: String,
    /// `%f`: the active file's name.
    pub file_name: String,
    /// `%F`: the active file's full path.
    pub file_path: String,
    /// `%c`: the system clipboard's contents.
    pub clipboard: String,
}

impl Context {
    fn date(&self) -> String {
        format!(
            "{:04}-{:02}-{:02} {}",
            self.year, self.month, self.day, self.weekday
        )
    }

    fn date_time(&self) -> String {
        format!("{} {:02}:{:02}", self.date(), self.hour, self.minute)
    }
}

/// `strftime`-lite: `%Y` `%m` `%d` `%H` `%M` `%a` `%%`. Unrecognized `%x`
/// sequences pass through unchanged.
fn strftime(fmt: &str, ctx: &Context) -> String {
    use std::fmt::Write as _;
    let chars: Vec<char> = fmt.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '%' && i + 1 < chars.len() {
            match chars[i + 1] {
                'Y' => {
                    let _ = write!(out, "{:04}", ctx.year);
                }
                'm' => {
                    let _ = write!(out, "{:02}", ctx.month);
                }
                'd' => {
                    let _ = write!(out, "{:02}", ctx.day);
                }
                'H' => {
                    let _ = write!(out, "{:02}", ctx.hour);
                }
                'M' => {
                    let _ = write!(out, "{:02}", ctx.minute);
                }
                'a' => out.push_str(&ctx.weekday),
                '%' => out.push('%'),
                other => {
                    out.push('%');
                    out.push(other);
                }
            }
            i += 2;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// The result of expanding a template.
#[derive(Debug, Clone)]
pub struct Expansion {
    /// The expanded text.
    pub text: String,
    /// Char offset into `text` where `%?` appeared, if the template had one —
    /// where the cursor should land once the entry is filed.
    pub cursor_offset: Option<usize>,
}

/// Expand `template`: `%^{}` placeholders are replaced by `answers` in the
/// order [`extract_prompts`] returns them; `%^g`/`%^G` is replaced by `tags`
/// (formatted by the caller, e.g. `":work:urgent:"`); `%t`/`%T`/`%u`/`%U`,
/// `%<...>`, `%a`, `%i`, `%f`, `%F`, `%c` come from `ctx`; `%?` is not
/// inserted but recorded as `cursor_offset`; `%%` is a literal `%`.
#[must_use]
pub fn expand(template: &str, answers: &[String], tags: Option<&str>, ctx: &Context) -> Expansion {
    let chars: Vec<char> = template.chars().collect();
    let mut out = String::new();
    let mut cursor_offset = None;
    let mut answer_idx = 0;
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '%' {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let Some(&next) = chars.get(i + 1) else {
            out.push('%');
            i += 1;
            continue;
        };
        if let Some(text) = simple_placeholder(next, ctx) {
            out.push_str(&text);
            i += 2;
            continue;
        }
        match next {
            '%' => {
                out.push('%');
                i += 2;
            }
            '?' => {
                cursor_offset = Some(out.chars().count());
                i += 2;
            }
            '<' => {
                if let Some(close) = find_char(&chars, i + 2, '>') {
                    let fmt: String = chars[i + 2..close].iter().collect();
                    out.push_str(&strftime(&fmt, ctx));
                    i = close + 1;
                } else {
                    out.push('%');
                    i += 1;
                }
            }
            '^' => {
                i = expand_caret(&chars, i, answers, &mut answer_idx, tags, &mut out);
            }
            _ => {
                out.push('%');
                i += 1;
            }
        }
    }
    Expansion {
        text: out,
        cursor_offset,
    }
}

/// The substitution for one of the simple, no-argument placeholders
/// (`%t`/`%T`/`%u`/`%U`/`%a`/`%i`/`%f`/`%F`/`%c`), or `None` if `c` isn't one
/// of them (the caller falls through to the remaining `%`-sequences).
fn simple_placeholder(c: char, ctx: &Context) -> Option<String> {
    Some(match c {
        't' => format!("<{}>", ctx.date()),
        'T' => format!("<{}>", ctx.date_time()),
        'u' => format!("[{}]", ctx.date()),
        'U' => format!("[{}]", ctx.date_time()),
        'a' => ctx.annotation.clone(),
        'i' => ctx.initial.clone(),
        'f' => ctx.file_name.clone(),
        'F' => ctx.file_path.clone(),
        'c' => ctx.clipboard.clone(),
        _ => return None,
    })
}

/// Handle a `%^...` sequence starting at `i` (`%^{Prompt}` or `%^g`/`%^G`),
/// appending its substitution to `out` and returning the index to resume
/// scanning from.
fn expand_caret(
    chars: &[char],
    i: usize,
    answers: &[String],
    answer_idx: &mut usize,
    tags: Option<&str>,
    out: &mut String,
) -> usize {
    match chars.get(i + 2) {
        Some('{') => {
            if let Some(close) = find_char(chars, i + 3, '}') {
                out.push_str(answers.get(*answer_idx).map_or("", String::as_str));
                *answer_idx += 1;
                close + 1
            } else {
                out.push('%');
                i + 1
            }
        }
        Some('g' | 'G') => {
            out.push_str(tags.unwrap_or(""));
            i + 3
        }
        _ => {
            out.push('%');
            i + 1
        }
    }
}

/// A live preview of `template` while its `%^{}`/`%^g` prompts are still
/// being answered — the shape shown to the user filling in a capture, so
/// they can see the whole template instead of an isolated prompt box.
/// Answered fields (the first `answers.len()` of `prompts`, in order) are
/// substituted for real; the field at `current` (if any) renders as
/// `‹Label›`; every other not-yet-answered field renders as `[Label]`.
/// `%^g`/`%^G` renders as `tags` if given, `‹Tags›` if `tags_current`, else
/// `[Tags]` (only when the template actually has one — see [`wants_tags`]).
/// Every other placeholder (`%t`, `%a`, `%i`, …) is fully expanded from
/// `ctx`, same as [`expand`]. `%?` is dropped, same as final filing.
#[must_use]
pub fn preview(
    template: &str,
    prompts: &[FieldPrompt],
    answers: &[String],
    current: Option<usize>,
    tags: Option<&str>,
    tags_current: bool,
    ctx: &Context,
) -> String {
    let placeholders: Vec<String> = prompts
        .iter()
        .enumerate()
        .map(|(i, p)| {
            answers.get(i).cloned().unwrap_or_else(|| {
                if Some(i) == current {
                    format!("\u{2039}{}\u{203a}", p.label)
                } else {
                    format!("[{}]", p.label)
                }
            })
        })
        .collect();
    let tag_display = tags.map(str::to_string).or_else(|| {
        wants_tags(template).then(|| {
            if tags_current {
                "\u{2039}Tags\u{203a}".to_string()
            } else {
                "[Tags]".to_string()
            }
        })
    });
    expand(template, &placeholders, tag_display.as_deref(), ctx).text
}

// ----- Wrapping -----------------------------------------------------------

/// Wrap `expanded` as `entry_type` — e.g. prefix `* ` for [`EntryType::Entry`]
/// when its first line isn't already a headline. A no-op when the first line
/// already has the expected shape.
#[must_use]
pub fn wrap_entry(entry_type: EntryType, expanded: &str) -> String {
    let mut lines: Vec<String> = expanded.split('\n').map(str::to_string).collect();
    if let Some(first) = lines.first_mut() {
        match entry_type {
            EntryType::Entry if headline_stars(first).is_none() => *first = format!("* {first}"),
            EntryType::Item if !first.trim_start().starts_with(['-', '+']) => {
                *first = format!("- {first}");
            }
            EntryType::CheckItem if !first.trim_start().starts_with("- [") => {
                *first = format!("- [ ] {first}");
            }
            EntryType::TableLine if !first.trim().starts_with('|') => {
                *first = format!("| {first} |");
            }
            EntryType::Entry
            | EntryType::Item
            | EntryType::CheckItem
            | EntryType::TableLine
            | EntryType::Plain => {}
        }
    }
    lines.join("\n")
}

// ----- Placement helpers ----------------------------------------------------

/// The number of leading `*` on a headline line (followed by a space), else
/// `None`.
fn headline_stars(line: &str) -> Option<usize> {
    let stars = line.len() - line.trim_start_matches('*').len();
    (stars > 0 && line[stars..].starts_with(' ')).then_some(stars)
}

/// Strip a trailing ` :tag:tag:` block from a headline's text, if present.
fn strip_tags(text: &str) -> &str {
    if let Some(idx) = text.rfind(" :") {
        let tail = &text[idx + 1..];
        if tail.len() > 2
            && tail.starts_with(':')
            && tail.ends_with(':')
            && tail[1..tail.len() - 1]
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == ':')
        {
            return text[..idx].trim_end();
        }
    }
    text
}

/// Locate a headline titled `title` (case-insensitive, tags ignored) anywhere
/// in `lines`, returning its `(index, level)`.
fn find_headline(lines: &[String], title: &str) -> Option<(usize, usize)> {
    let want = title.trim().to_ascii_lowercase();
    lines.iter().enumerate().find_map(|(i, l)| {
        let stars = headline_stars(l)?;
        (strip_tags(l[stars..].trim()).to_ascii_lowercase() == want).then_some((i, stars))
    })
}

/// The end of the subtree starting at `lines[start]` (a headline at `level`):
/// the index of the next headline at `level` or shallower, or `lines.len()`.
fn subtree_end(lines: &[String], start: usize, level: usize) -> usize {
    (start + 1..lines.len())
        .find(|&i| headline_stars(&lines[i]).is_some_and(|s| s <= level))
        .unwrap_or(lines.len())
}

/// Where a new child of the headline at `headline_idx` should start: right
/// after the headline, skipping its property drawer if one immediately
/// follows.
fn child_insert_point(lines: &[String], headline_idx: usize) -> usize {
    let mut i = headline_idx + 1;
    if lines
        .get(i)
        .is_some_and(|l| l.trim().eq_ignore_ascii_case(":PROPERTIES:"))
    {
        i += 1;
        while i < lines.len() && !lines[i].trim().eq_ignore_ascii_case(":END:") {
            i += 1;
        }
        i = if i < lines.len() { i + 1 } else { i };
    }
    i
}

/// The end of the file's front matter: leading blank lines, an optional
/// file-level `:PROPERTIES:`/`:END:` drawer, then any run of `#+` lines.
fn front_matter_end(lines: &[String]) -> usize {
    let mut i = 0;
    while i < lines.len() && lines[i].trim().is_empty() {
        i += 1;
    }
    if i < lines.len() && lines[i].trim().eq_ignore_ascii_case(":PROPERTIES:") {
        i += 1;
        while i < lines.len() && !lines[i].trim().eq_ignore_ascii_case(":END:") {
            i += 1;
        }
        i = if i < lines.len() { i + 1 } else { i };
    }
    while i < lines.len() && (lines[i].trim_start().starts_with("#+") || lines[i].trim().is_empty())
    {
        i += 1;
    }
    i
}

/// Insert `block` at line index `at`, padded by `empty_lines` blank lines on
/// both sides.
fn splice(lines: &mut Vec<String>, at: usize, block: Vec<String>, empty_lines: u8) {
    let pad = vec![String::new(); empty_lines as usize];
    let mut insert = pad.clone();
    insert.extend(block);
    insert.extend(pad);
    for (offset, line) in insert.into_iter().enumerate() {
        lines.insert(at + offset, line);
    }
}

/// Shift every headline's star count in `entry_lines` so its first line sits
/// one level deeper than `target_level` (a no-op if the first line isn't a
/// headline — plain/item/table entries have no notion of level).
fn bump_stars(entry_lines: &[String], target_level: usize) -> Vec<String> {
    let Some(first_stars) = entry_lines.first().and_then(|l| headline_stars(l)) else {
        return entry_lines.to_vec();
    };
    let delta =
        i64::try_from(target_level + 1).unwrap_or(0) - i64::try_from(first_stars).unwrap_or(0);
    entry_lines
        .iter()
        .map(|l| match headline_stars(l) {
            Some(s) => {
                let new_s = (i64::try_from(s).unwrap_or(0) + delta).max(1);
                let new_s = usize::try_from(new_s).unwrap_or(1);
                format!("{}{}", "*".repeat(new_s), &l[s..])
            }
            None => l.clone(),
        })
        .collect()
}

/// Insert `entry` at the top or bottom of `file_text` (a `file:` target),
/// after/before any leading front matter, padded by `empty_lines`.
#[must_use]
pub fn insert_top_level(file_text: &str, entry: &str, prepend: bool, empty_lines: u8) -> String {
    let mut lines: Vec<String> = file_text.split('\n').map(str::to_string).collect();
    let entry_lines: Vec<String> = entry.split('\n').map(str::to_string).collect();
    let at = if prepend {
        front_matter_end(&lines)
    } else {
        lines.len()
    };
    splice(&mut lines, at, entry_lines, empty_lines);
    lines.join("\n")
}

/// Insert `entry` as a child of the headline titled `headline` in
/// `file_text` (a `file+headline:` target), creating that headline at the
/// end of the file first if it doesn't already exist.
#[must_use]
pub fn insert_under_headline(
    file_text: &str,
    headline: &str,
    entry: &str,
    prepend: bool,
    empty_lines: u8,
) -> String {
    let mut lines: Vec<String> = file_text.split('\n').map(str::to_string).collect();
    let title = headline.trim();
    let h_idx = find_headline(&lines, title).map_or_else(
        || create_child_headline(&mut lines, None, 1, title),
        |(i, _)| i,
    );
    let level = headline_stars(&lines[h_idx]).unwrap_or(1);
    let entry_lines: Vec<String> = entry.split('\n').map(str::to_string).collect();
    let bumped = bump_stars(&entry_lines, level);
    let at = if prepend {
        child_insert_point(&lines, h_idx)
    } else {
        subtree_end(&lines, h_idx, level)
    };
    splice(&mut lines, at, bumped, empty_lines);
    lines.join("\n")
}

/// Insert `entry` as a child of the headline/file-level node carrying
/// `:ID: id` (an `id:` target). Returns `None` if no such id is present in
/// `file_text`.
#[must_use]
pub fn insert_under_id(
    file_text: &str,
    id: &str,
    entry: &str,
    prepend: bool,
    empty_lines: u8,
) -> Option<String> {
    let mut lines: Vec<String> = file_text.split('\n').map(str::to_string).collect();
    let id = id.trim();
    let id_line = lines.iter().position(|l| {
        let t = l.trim();
        t.get(..4).is_some_and(|p| p.eq_ignore_ascii_case(":ID:")) && t[4..].trim() == id
    })?;
    let owner = (0..id_line)
        .rev()
        .find_map(|i| headline_stars(&lines[i]).map(|s| (i, s)));
    let entry_lines: Vec<String> = entry.split('\n').map(str::to_string).collect();
    if let Some((h_idx, level)) = owner {
        let bumped = bump_stars(&entry_lines, level);
        let at = if prepend {
            child_insert_point(&lines, h_idx)
        } else {
            subtree_end(&lines, h_idx, level)
        };
        splice(&mut lines, at, bumped, empty_lines);
    } else {
        let at = if prepend {
            front_matter_end(&lines)
        } else {
            lines.len()
        };
        splice(&mut lines, at, entry_lines, empty_lines);
    }
    Some(lines.join("\n"))
}

// ----- Date tree --------------------------------------------------------------

const MONTH_NAMES: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// The `Year` / `Year-Month MonthName` / `Year-Month-Day Weekday` headline
/// titles for a date tree, from `ctx`'s date.
#[must_use]
pub fn datetree_path(ctx: &Context) -> [String; 3] {
    let month_name = ctx
        .month
        .checked_sub(1)
        .and_then(|i| MONTH_NAMES.get(i as usize))
        .copied()
        .unwrap_or("");
    [
        format!("{:04}", ctx.year),
        format!("{:04}-{:02} {month_name}", ctx.year, ctx.month),
        format!(
            "{:04}-{:02}-{:02} {}",
            ctx.year, ctx.month, ctx.day, ctx.weekday
        ),
    ]
}

/// Find a direct child of `parent` (or, if `parent` is `None`, a top-level
/// headline) at `level` whose title matches `title` exactly.
fn find_child_headline(
    lines: &[String],
    parent: Option<usize>,
    level: usize,
    title: &str,
) -> Option<usize> {
    let (start, end) = match parent {
        None => (0, lines.len()),
        Some(p) => (p + 1, subtree_end(lines, p, level - 1)),
    };
    (start..end)
        .find(|&i| headline_stars(&lines[i]) == Some(level) && lines[i][level..].trim() == title)
}

/// Append a new headline at `level` as the last child of `parent` (or at the
/// end of the file if `parent` is `None`), returning its line index.
fn create_child_headline(
    lines: &mut Vec<String>,
    parent: Option<usize>,
    level: usize,
    title: &str,
) -> usize {
    let at = match parent {
        None if lines.iter().all(|l| l.trim().is_empty()) => {
            lines.clear();
            0
        }
        None => lines.len(),
        Some(p) => subtree_end(lines, p, level - 1),
    };
    // Only pad top-level appends with a separating blank line; a freshly
    // created child headline nests directly under its parent, no gap.
    let need_blank =
        parent.is_none() && at > 0 && lines.get(at - 1).is_some_and(|l| !l.trim().is_empty());
    let mut block = Vec::new();
    if need_blank {
        block.push(String::new());
    }
    block.push(format!("{} {title}", "*".repeat(level)));
    for (offset, line) in block.into_iter().enumerate() {
        lines.insert(at + offset, line);
    }
    at + usize::from(need_blank)
}

/// Insert `entry` under the dated subtree for `ctx`'s date in `file_text` (a
/// `file+datetree:` target), creating the `Year > Month > Day` headline chain
/// as needed.
#[must_use]
pub fn insert_datetree(
    file_text: &str,
    ctx: &Context,
    entry: &str,
    prepend: bool,
    empty_lines: u8,
) -> String {
    let mut lines: Vec<String> = file_text.split('\n').map(str::to_string).collect();
    let path = datetree_path(ctx);
    let mut parent: Option<usize> = None;
    for (level_idx, title) in path.iter().enumerate() {
        let level = level_idx + 1;
        let found = find_child_headline(&lines, parent, level, title);
        parent =
            Some(found.unwrap_or_else(|| create_child_headline(&mut lines, parent, level, title)));
    }
    let day_idx = parent.unwrap_or(0);
    let entry_lines: Vec<String> = entry.split('\n').map(str::to_string).collect();
    let bumped = bump_stars(&entry_lines, 3);
    let at = if prepend {
        child_insert_point(&lines, day_idx)
    } else {
        subtree_end(&lines, day_idx, 3)
    };
    splice(&mut lines, at, bumped, empty_lines);
    lines.join("\n")
}

// ----- Built-in templates ---------------------------------------------------

/// The built-in templates seeded into `org_capture_templates` by default:
/// `Anything` (a quick `* TODO`), `Todo` (a blank `* TODO` to fill in), and
/// `Contact` (an org-contacts-style entry with a property drawer).
#[must_use]
pub fn defaults() -> Vec<CaptureTemplate> {
    vec![
        CaptureTemplate {
            key: "a".to_string(),
            description: "Anything".to_string(),
            entry_type: EntryType::Entry,
            target: default_target(),
            template: "* TODO %^{Task}".to_string(),
            prepend: false,
            empty_lines: 0,
            immediate_finish: true,
            clock_in: false,
        },
        CaptureTemplate {
            key: "t".to_string(),
            description: "Task".to_string(),
            entry_type: EntryType::Entry,
            target: default_target(),
            template: "* TODO ".to_string(),
            prepend: false,
            empty_lines: 0,
            immediate_finish: false,
            clock_in: false,
        },
        CaptureTemplate {
            key: "b".to_string(),
            description: "Babel".to_string(),
            entry_type: EntryType::Plain,
            target: default_target(),
            template: "#+begin_src %^{Language}\n%?\n#+end_src".to_string(),
            prepend: false,
            empty_lines: 0,
            immediate_finish: false,
            clock_in: false,
        },
        CaptureTemplate {
            key: "n".to_string(),
            description: "Note".to_string(),
            entry_type: EntryType::Entry,
            target: default_target(),
            template: "* %^{Note}\n  %U".to_string(),
            prepend: false,
            empty_lines: 0,
            immediate_finish: true,
            clock_in: false,
        },
        CaptureTemplate {
            key: "c".to_string(),
            description: "Contact".to_string(),
            entry_type: EntryType::Entry,
            target: default_target(),
            template: concat!(
                "* %^{Name}\n",
                "  :PROPERTIES:\n",
                "  :EMAIL: %^{Email}\n",
                "  :PHONE: %^{Phone}\n",
                "  :ADDRESS: %^{Address}\n",
                "  :BIRTHDAY: %^{Birthday}\n",
                "  :END:",
            )
            .to_string(),
            prepend: false,
            empty_lines: 0,
            immediate_finish: true,
            clock_in: false,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> Context {
        Context {
            year: 2026,
            month: 7,
            day: 22,
            hour: 14,
            minute: 30,
            weekday: "Wed".to_string(),
            annotation: "[[file:foo.org::5][foo.org:5]]".to_string(),
            initial: "selected text".to_string(),
            file_name: "foo.org".to_string(),
            file_path: "/tmp/foo.org".to_string(),
            clipboard: "clip".to_string(),
        }
    }

    #[test]
    fn target_parse_covers_every_shape() {
        assert_eq!(Target::parse("cursor"), Target::Cursor);
        assert_eq!(Target::parse(""), Target::Cursor);
        assert_eq!(
            Target::parse("id:abc-123"),
            Target::Id("abc-123".to_string())
        );
        assert_eq!(
            Target::parse("file:journal.org"),
            Target::File("journal.org".to_string())
        );
        assert_eq!(
            Target::parse("file+headline:work.org#Inbox"),
            Target::FileHeadline("work.org".to_string(), "Inbox".to_string())
        );
        assert_eq!(
            Target::parse("file+datetree:journal.org"),
            Target::FileDatetree("journal.org".to_string())
        );
    }

    #[test]
    fn extract_prompts_reads_label_and_default_in_order() {
        let prompts = extract_prompts("* %^{Name} likes %^{Food|pizza}");
        assert_eq!(
            prompts,
            vec![
                FieldPrompt {
                    label: "Name".to_string(),
                    default: String::new(),
                    choices: vec![]
                },
                FieldPrompt {
                    label: "Food".to_string(),
                    default: "pizza".to_string(),
                    choices: vec!["pizza".to_string()],
                },
            ]
        );
    }

    #[test]
    fn extract_prompts_reads_the_full_pipe_delimited_choice_list() {
        // The org-priority capture idiom: default "0", full candidate list 0-9.
        let prompts = extract_prompts("[#%^{Priority|0|0|1|2|3|4|5|6|7|8|9}]");
        assert_eq!(prompts.len(), 1);
        assert_eq!(prompts[0].label, "Priority");
        assert_eq!(prompts[0].default, "0");
        assert_eq!(
            prompts[0].choices,
            vec!["0", "0", "1", "2", "3", "4", "5", "6", "7", "8", "9"]
        );
    }

    #[test]
    fn wants_tags_detects_lowercase_and_uppercase() {
        assert!(wants_tags("* Task %^g"));
        assert!(wants_tags("* Task %^G"));
        assert!(!wants_tags("* Task %^{Tag}"));
    }

    #[test]
    fn expand_substitutes_answers_in_order() {
        let answers = vec!["Alice".to_string(), "pizza".to_string()];
        let e = expand("* %^{Name} likes %^{Food}", &answers, None, &ctx());
        assert_eq!(e.text, "* Alice likes pizza");
        assert_eq!(e.cursor_offset, None);
    }

    #[test]
    fn expand_handles_timestamps_and_annotation_and_percent_escape() {
        let e = expand("%t / %u / %a / 100%%", &[], None, &ctx());
        assert_eq!(
            e.text,
            "<2026-07-22 Wed> / [2026-07-22 Wed] / [[file:foo.org::5][foo.org:5]] / 100%"
        );
    }

    #[test]
    fn expand_handles_time_variants_and_custom_format() {
        let e = expand("%T %U %<%Y/%m/%d>", &[], None, &ctx());
        assert_eq!(
            e.text,
            "<2026-07-22 Wed 14:30> [2026-07-22 Wed 14:30] 2026/07/22"
        );
    }

    #[test]
    fn expand_records_cursor_marker_offset_and_drops_it() {
        let e = expand("* TODO %?", &[], None, &ctx());
        assert_eq!(e.text, "* TODO ");
        assert_eq!(e.cursor_offset, Some(7));
    }

    #[test]
    fn expand_fills_tags_and_file_and_clipboard_placeholders() {
        let e = expand(
            "* Task %^g\n  %f %F %c %i",
            &[],
            Some(":work:urgent:"),
            &ctx(),
        );
        assert_eq!(
            e.text,
            "* Task :work:urgent:\n  foo.org /tmp/foo.org clip selected text"
        );
    }

    #[test]
    fn preview_shows_answered_current_and_pending_fields_distinctly() {
        let template = "* %^{Name}\n  :EMAIL: %^{Email}\n  :PHONE: %^{Phone}";
        let prompts = extract_prompts(template);
        // Nothing answered yet; "Name" is the field about to be asked.
        let p = preview(template, &prompts, &[], Some(0), None, false, &ctx());
        assert_eq!(
            p,
            "* \u{2039}Name\u{203a}\n  :EMAIL: [Email]\n  :PHONE: [Phone]"
        );
        // "Name" answered, now on "Email".
        let answers = vec!["Alice".to_string()];
        let p = preview(template, &prompts, &answers, Some(1), None, false, &ctx());
        assert_eq!(
            p,
            "* Alice\n  :EMAIL: \u{2039}Email\u{203a}\n  :PHONE: [Phone]"
        );
    }

    #[test]
    fn preview_shows_tags_placeholder_only_when_template_wants_tags() {
        let e = preview("* Task %^g", &[], &[], None, None, true, &ctx());
        assert_eq!(e, "* Task \u{2039}Tags\u{203a}");
        let e = preview("* Task %^g", &[], &[], None, Some(":work:"), false, &ctx());
        assert_eq!(e, "* Task :work:");
        let e = preview("* Task", &[], &[], None, None, false, &ctx());
        assert_eq!(e, "* Task", "no %^g in the template, no [Tags] placeholder");
    }

    #[test]
    fn preview_still_expands_static_placeholders_immediately() {
        let e = preview(
            "%t %^{Name}",
            &extract_prompts("%t %^{Name}"),
            &[],
            Some(0),
            None,
            false,
            &ctx(),
        );
        assert_eq!(e, "<2026-07-22 Wed> \u{2039}Name\u{203a}");
    }

    #[test]
    fn wrap_entry_adds_marker_only_when_missing() {
        assert_eq!(wrap_entry(EntryType::Entry, "Buy milk"), "* Buy milk");
        assert_eq!(wrap_entry(EntryType::Entry, "* Buy milk"), "* Buy milk");
        assert_eq!(wrap_entry(EntryType::Item, "Buy milk"), "- Buy milk");
        assert_eq!(
            wrap_entry(EntryType::CheckItem, "Buy milk"),
            "- [ ] Buy milk"
        );
        assert_eq!(wrap_entry(EntryType::TableLine, "a | b"), "| a | b |");
        assert_eq!(wrap_entry(EntryType::Plain, "raw text"), "raw text");
    }

    #[test]
    fn insert_top_level_appends_and_prepends() {
        let file = "#+title: Notes\n\n* Existing\n";
        let appended = insert_top_level(file, "* New", false, 0);
        assert!(appended.ends_with("* Existing\n\n* New"));
        let prepended = insert_top_level(file, "* New", true, 0);
        assert!(prepended.starts_with("#+title: Notes\n\n* New"));
        assert!(prepended.contains("* Existing"));
    }

    #[test]
    fn insert_top_level_pads_with_empty_lines() {
        let out = insert_top_level("* Existing", "* New", false, 1);
        assert_eq!(out, "* Existing\n\n* New\n");
    }

    #[test]
    fn insert_under_headline_finds_existing_and_bumps_level() {
        let file = "* Inbox\n** Old child\n* Other\n";
        let out = insert_under_headline(file, "Inbox", "* New task", false, 0);
        assert_eq!(out, "* Inbox\n** Old child\n** New task\n* Other\n");
    }

    #[test]
    fn insert_under_headline_prepends_after_existing_children_start() {
        let file = "* Inbox\n** Old child\n* Other\n";
        let out = insert_under_headline(file, "Inbox", "* New task", true, 0);
        assert_eq!(out, "* Inbox\n** New task\n** Old child\n* Other\n");
    }

    #[test]
    fn insert_under_headline_creates_missing_headline() {
        let file = "* Existing\n";
        let out = insert_under_headline(file, "Inbox", "* New task", false, 0);
        assert_eq!(out, "* Existing\n\n* Inbox\n** New task");
    }

    #[test]
    fn insert_under_id_finds_owning_headline() {
        let file = "* Project\n  :PROPERTIES:\n  :ID: abc\n  :END:\n** Existing\n* Other\n";
        let out = insert_under_id(file, "abc", "* New task", false, 0).unwrap();
        assert_eq!(
            out,
            "* Project\n  :PROPERTIES:\n  :ID: abc\n  :END:\n** Existing\n** New task\n* Other\n"
        );
    }

    #[test]
    fn insert_under_id_missing_id_returns_none() {
        assert_eq!(
            insert_under_id("* Project\n", "abc", "* New task", false, 0),
            None
        );
    }

    #[test]
    fn insert_under_id_at_file_level_appends_to_whole_file() {
        let file = ":PROPERTIES:\n:ID: root\n:END:\n#+title: Node\n\nSome text.\n";
        let out = insert_under_id(file, "root", "* New task", false, 0).unwrap();
        assert!(out.ends_with("Some text.\n\n* New task"));
    }

    #[test]
    fn datetree_path_builds_year_month_day_titles() {
        let path = datetree_path(&ctx());
        assert_eq!(path, ["2026", "2026-07 July", "2026-07-22 Wed"]);
    }

    #[test]
    fn insert_datetree_creates_full_chain_then_reuses_it() {
        let out = insert_datetree("", &ctx(), "* Entry one", false, 0);
        assert_eq!(
            out,
            "* 2026\n** 2026-07 July\n*** 2026-07-22 Wed\n**** Entry one"
        );
        let out2 = insert_datetree(&out, &ctx(), "* Entry two", false, 0);
        assert_eq!(
            out2,
            "* 2026\n** 2026-07 July\n*** 2026-07-22 Wed\n**** Entry one\n**** Entry two"
        );
    }

    #[test]
    fn defaults_seed_five_built_in_templates_with_distinct_keys() {
        let templates = defaults();
        assert_eq!(templates.len(), 5);
        let keys: Vec<&str> = templates.iter().map(|t| t.key.as_str()).collect();
        assert_eq!(keys, vec!["a", "t", "b", "n", "c"]);
        for t in &templates {
            assert_eq!(Target::parse(&t.target), Target::Cursor);
        }
    }
}
