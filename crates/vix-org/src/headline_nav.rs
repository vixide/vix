//! Headline location and structure editing: finding the headline that
//! governs a cursor line, navigating between headlines (parent/next/prev/
//! forward-sibling/backward-sibling), inserting a new heading, sorting a
//! subtree's children, and refiling/pasting a subtree elsewhere. Extracted
//! from `lib.rs` (T516) as its own module -- despite being one of the most
//! depended-upon sections in the crate (`governing` and `relevel` back
//! several other sections' own edits), it's a cohesive feature in its own
//! right, matching how every other topic here gets its own file.

#![cfg_attr(
    not(test),
    deny(
        clippy::arithmetic_side_effects,
        clippy::integer_division,
        clippy::modulo_arithmetic,
        clippy::as_conversions,
        clippy::indexing_slicing,
        clippy::string_slice
    )
)]

use super::{add_or, headline_level, line_at, next, sub_or, subtree_range};

// ----- Headline location & structure editing --------------------------------

/// The line of the headline governing `line`: the nearest headline at or above
/// it. `None` when the cursor sits before the first headline.
pub(crate) fn governing(lines: &[&str], line: usize) -> Option<usize> {
    let last = lines.len().checked_sub(1)?;
    (0..=line.min(last))
        .rev()
        .find(|&i| headline_level(line_at(lines, i)).is_some())
}

/// Rewrite a headline's stars to shift its level by `delta` (clamped to ≥ 1).
/// Non-headline lines pass through unchanged.
pub(crate) fn relevel(line: &str, delta: i64) -> String {
    match headline_level(line) {
        Some(level) => {
            let shifted = i64::try_from(level)
                .unwrap_or(i64::MAX)
                .saturating_add(delta);
            let new = usize::try_from(shifted.max(1)).unwrap_or(1);
            format!(
                "{} {}",
                "*".repeat(new),
                line.get(level..).unwrap_or_default().trim_start()
            )
        }
        None => line.to_string(),
    }
}

/// Insert a sibling headline after the cursor line (Org `M-RET`): same level as
/// the governing headline, or level 1 outside any subtree. Returns the new text
/// and the line of the inserted headline (its stars and trailing space, ready to
/// type the title).
#[must_use]
pub fn new_heading(text: &str, line: usize) -> (String, usize) {
    let lines: Vec<&str> = text.split('\n').collect();
    let level = governing(&lines, line)
        .and_then(|h| headline_level(line_at(&lines, h)))
        .unwrap_or(1);
    let mut out: Vec<String> = lines.iter().map(|s| (*s).to_string()).collect();
    let at = next(line).min(out.len());
    out.insert(at, format!("{} ", "*".repeat(level)));
    (out.join("\n"), at)
}

/// Org `C-c C-u`: the governing headline when the cursor is in a body, else the
/// parent headline (nearest above with a smaller level). `None` at top level.
#[must_use]
pub fn nav_parent(text: &str, line: usize) -> Option<usize> {
    let lines: Vec<&str> = text.split('\n').collect();
    let h = governing(&lines, line)?;
    if h != line {
        return Some(h);
    }
    let level = headline_level(line_at(&lines, h))?;
    (0..h)
        .rev()
        .find(|&i| headline_level(line_at(&lines, i)).is_some_and(|l| l < level))
}

/// The next headline after `line` (any level), like Org `C-c C-n`.
#[must_use]
pub fn nav_next(text: &str, line: usize) -> Option<usize> {
    let lines: Vec<&str> = text.split('\n').collect();
    (next(line)..lines.len()).find(|&i| headline_level(line_at(&lines, i)).is_some())
}

/// The previous headline before `line` (any level), like Org `C-c C-p`.
#[must_use]
pub fn nav_prev(text: &str, line: usize) -> Option<usize> {
    let lines: Vec<&str> = text.split('\n').collect();
    (0..line.min(lines.len()))
        .rev()
        .find(|&i| headline_level(line_at(&lines, i)).is_some())
}

/// The next sibling headline (same level, within the same parent), like Org
/// `C-c C-f`. Stops at the parent boundary.
#[must_use]
pub fn nav_forward_same(text: &str, line: usize) -> Option<usize> {
    let lines: Vec<&str> = text.split('\n').collect();
    let h = governing(&lines, line)?;
    let level = headline_level(line_at(&lines, h))?;
    for (i, l) in lines.iter().enumerate().skip(next(h)) {
        match headline_level(l) {
            Some(l) if l < level => return None,
            Some(l) if l == level => return Some(i),
            _ => {}
        }
    }
    None
}

/// The previous sibling headline (same level, within the same parent), like Org
/// `C-c C-b`. Stops at the parent boundary.
#[must_use]
pub fn nav_backward_same(text: &str, line: usize) -> Option<usize> {
    let lines: Vec<&str> = text.split('\n').collect();
    let h = governing(&lines, line)?;
    let level = headline_level(line_at(&lines, h))?;
    for (i, l) in lines.iter().enumerate().take(h).rev() {
        match headline_level(l) {
            Some(l) if l < level => return None,
            Some(l) if l == level => return Some(i),
            _ => {}
        }
    }
    None
}

/// Every headline in `text` as `(line, level, title)` (title = text after the
/// stars, trimmed). Used by refile target matching and link following.
#[must_use]
pub fn headlines(text: &str) -> Vec<(usize, usize, String)> {
    text.split('\n')
        .enumerate()
        .filter_map(|(i, l)| {
            headline_level(l).map(|lv| (i, lv, l.get(lv..).unwrap_or_default().trim().to_string()))
        })
        .collect()
}

/// The `[start, end)` line range of the subtree governing `line` (walking up to
/// the nearest headline first). `None` before the first headline.
#[must_use]
pub fn governing_subtree(text: &str, line: usize) -> Option<(usize, usize)> {
    let lines: Vec<&str> = text.split('\n').collect();
    subtree_range(&lines, governing(&lines, line)?)
}

/// Sort the children of the subtree governing `line` alphabetically by headline
/// text (case-insensitive): direct children when inside a subtree, the top-level
/// trees when before the first headline. `None` with fewer than two children.
#[must_use]
pub fn sort_children(text: &str, line: usize) -> Option<String> {
    let lines: Vec<&str> = text.split('\n').collect();
    let (start, end, child_level) = match governing(&lines, line) {
        Some(h) => {
            let (s, e) = subtree_range(&lines, h)?;
            (next(s), e, next(headline_level(line_at(&lines, h))?))
        }
        None => (0, lines.len(), 1),
    };
    let mut blocks: Vec<(usize, usize)> = Vec::new();
    let mut i = start;
    while i < end {
        if headline_level(line_at(&lines, i)) == Some(child_level) {
            let (s, e) = subtree_range(&lines, i)?;
            let e = e.min(end);
            blocks.push((s, e));
            i = e;
        } else {
            i = next(i);
        }
    }
    if blocks.len() < 2 {
        return None;
    }
    let first = blocks.first()?.0;
    let mut sorted = blocks.clone();
    sorted.sort_by_key(|&(s, _)| {
        let l = line_at(&lines, s);
        let lv = headline_level(l).unwrap_or(0);
        l.get(lv..).unwrap_or_default().trim().to_ascii_lowercase()
    });
    let mut out: Vec<&str> = lines.get(..first)?.to_vec();
    for &(s, e) in &sorted {
        out.extend_from_slice(lines.get(s..e)?);
    }
    out.extend_from_slice(lines.get(end..)?);
    Some(out.join("\n"))
}

/// Refile the subtree governing `line` to the end of the subtree at
/// `target_line` (releveled to one deeper than the target), like Org `C-c C-w`.
/// Returns the new text and the moved headline's line. `None` when either side
/// is not a headline or the target lies inside the source subtree.
#[must_use]
pub fn refile(text: &str, line: usize, target_line: usize) -> Option<(String, usize)> {
    let lines: Vec<&str> = text.split('\n').collect();
    let src = governing(&lines, line)?;
    let (ss, se) = subtree_range(&lines, src)?;
    if (ss..se).contains(&target_line) {
        return None;
    }
    let target_level = headline_level(lines.get(target_line)?)?;
    let (_, te) = subtree_range(&lines, target_line)?;
    let src_level = headline_level(line_at(&lines, ss))?;
    let delta = i64::try_from(next(target_level))
        .ok()?
        .checked_sub(i64::try_from(src_level).ok()?)?;
    let block: Vec<String> = lines
        .get(ss..se)?
        .iter()
        .map(|l| relevel(l, delta))
        .collect();
    let mut out: Vec<String> = lines
        .iter()
        .enumerate()
        .filter(|(i, _)| !(ss..se).contains(i))
        .map(|(_, s)| (*s).to_string())
        .collect();
    let insert_at = if te > ss {
        sub_or(te, sub_or(se, ss, 0), 0)
    } else {
        te
    };
    for (k, l) in block.into_iter().enumerate() {
        out.insert(add_or(insert_at, k, usize::MAX).min(out.len()), l);
    }
    Some((out.join("\n"), insert_at))
}

/// Paste a cut/copied subtree after the subtree governing `line`, releveled to
/// match it as a sibling (level 1 outside any subtree), like Org `C-c C-x C-y`.
/// Returns the new text and the pasted headline's line. `None` when `clip` does
/// not start with a headline.
#[must_use]
pub fn paste_subtree(text: &str, line: usize, clip: &str) -> Option<(String, usize)> {
    let clip = clip.trim_end_matches('\n');
    let clip_lines: Vec<&str> = clip.split('\n').collect();
    let clip_level = headline_level(clip_lines.first()?)?;
    let lines: Vec<&str> = text.split('\n').collect();
    let (at, target_level) = match governing(&lines, line) {
        Some(h) => {
            let (_, e) = subtree_range(&lines, h)?;
            (e, headline_level(line_at(&lines, h))?)
        }
        None => (next(line).min(lines.len()), 1),
    };
    let delta = i64::try_from(target_level)
        .ok()?
        .checked_sub(i64::try_from(clip_level).ok()?)?;
    let mut out: Vec<String> = lines.iter().map(|s| (*s).to_string()).collect();
    for (k, l) in clip_lines.iter().enumerate() {
        out.insert(add_or(at, k, usize::MAX).min(out.len()), relevel(l, delta));
    }
    Some((out.join("\n"), at))
}
