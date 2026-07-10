//! Org-roam: networked, Zettelkasten-style note-taking over a directory of Org
//! files (<https://www.orgroam.com/>).
//!
//! A *node* is an `.org` file carrying an `:ID:` property and a `#+title:`. Nodes
//! link to one another with `[[id:<id>][Title]]` links; the set of nodes and
//! links forms a graph. This module is the pure, testable core: parsing a node's
//! title/id, building new node and daily-note skeletons, editing the file-level
//! property drawer and `#+filetags:`, and compiling cross-node views (backlinks,
//! a Mermaid graph, and a node index). The host (`app`) wires these to the
//! Org → Roam menu, prompting for input and reading/writing the files.
//!
//! All functions are pure so they can be unit-tested without a live editor or a
//! filesystem.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::fmt::Write as _;
use std::sync::LazyLock;

use regex::Regex;

/// The dailies sub-directory, matching org-roam's default `org-roam-dailies-directory`.
pub const DAILIES_DIR: &str = "daily";

/// Turn a node title into a filesystem-safe slug: lowercase, runs of
/// non-alphanumeric characters collapsed to a single `-`, with no leading or
/// trailing `-`. An all-punctuation title slugs to `"node"`.
#[must_use]
pub fn slugify(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut prev_dash = false;
    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() {
            out.extend(ch.to_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "node".to_string()
    } else {
        trimmed.to_string()
    }
}

/// The value of the first `#+title:` keyword (case-insensitive), trimmed.
#[must_use]
pub fn node_title(content: &str) -> Option<String> {
    for line in content.lines() {
        let lower = line.trim_start().to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("#+title:") {
            let start = line.len() - rest.len();
            return Some(line[start..].trim().to_string());
        }
    }
    None
}

/// The `:ID:` value from the file-level property drawer, if any.
#[must_use]
pub fn node_id(content: &str) -> Option<String> {
    property(content, "ID")
}

/// The value of a `:KEY:` line in the file-level property drawer.
fn property(content: &str, key: &str) -> Option<String> {
    let want = format!(":{}:", key.to_ascii_uppercase());
    for line in content.lines() {
        let t = line.trim();
        if t.eq_ignore_ascii_case(":END:") {
            break;
        }
        if let Some(rest) = t.get(..want.len())
            && rest.eq_ignore_ascii_case(&want)
        {
            return Some(t[want.len()..].trim().to_string());
        }
    }
    None
}

/// A fresh node file: a property drawer holding `id`, then the `#+title:`.
#[must_use]
pub fn new_node(title: &str, id: &str) -> String {
    format!(":PROPERTIES:\n:ID:       {id}\n:END:\n#+title: {title}\n")
}

/// An `[[id:…][title]]` link to a node.
#[must_use]
pub fn node_link(id: &str, title: &str) -> String {
    format!("[[id:{id}][{title}]]")
}

/// An org-transclusion directive for a node: `#+transclude: [[id:…][title]]`.
#[must_use]
pub fn transclusion(id: &str, title: &str) -> String {
    format!("#+transclude: {}", node_link(id, title))
}

/// The number of leading `*` on a headline line (followed by a space), else
/// `None`. Mirrors `org::headline_level` so this module stays self-contained.
fn headline_stars(line: &str) -> Option<usize> {
    let stars = line.len() - line.trim_start_matches('*').len();
    (stars > 0 && line[stars..].starts_with(' ')).then_some(stars)
}

/// Make the headline at `line` a node by giving it an `:ID:`. If the headline
/// already has a property drawer, the `:ID:` is inserted into it; otherwise a new
/// drawer is created directly beneath the headline. Returns `None` if `line` is
/// not a headline or already carries an `:ID:`.
#[must_use]
pub fn nodeify(text: &str, line: usize, id: &str) -> Option<String> {
    let mut lines: Vec<String> = text.split('\n').map(str::to_string).collect();
    headline_stars(lines.get(line)?)?;
    // An existing drawer must start on the very next line.
    if lines
        .get(line + 1)
        .is_some_and(|l| l.trim().eq_ignore_ascii_case(":PROPERTIES:"))
    {
        let mut i = line + 2;
        while i < lines.len() && !lines[i].trim().eq_ignore_ascii_case(":END:") {
            if lines[i].trim().to_ascii_uppercase().starts_with(":ID:") {
                return None; // already a node
            }
            i += 1;
        }
        lines.insert(line + 2, format!(":ID:       {id}"));
    } else {
        lines.insert(line + 1, ":END:".to_string());
        lines.insert(line + 1, format!(":ID:       {id}"));
        lines.insert(line + 1, ":PROPERTIES:".to_string());
    }
    Some(lines.join("\n"))
}

/// Every `:ID:` value declared anywhere in `content` (file-level or per-subtree
/// property drawers).
#[must_use]
pub fn all_ids(content: &str) -> Vec<String> {
    content
        .lines()
        .filter_map(|l| {
            let t = l.trim();
            t.get(..4)
                .filter(|p| p.eq_ignore_ascii_case(":ID:"))
                .map(|_| t[4..].trim().to_string())
        })
        .filter(|s| !s.is_empty())
        .collect()
}

/// The filename for the daily note of `date` (a `YYYY-MM-DD` string).
#[must_use]
pub fn daily_filename(date: &str) -> String {
    format!("{date}.org")
}

/// A fresh daily-note file titled with its date (no `:ID:` — the host fills it).
#[must_use]
pub fn daily_template(date: &str, id: &str) -> String {
    format!(":PROPERTIES:\n:ID:       {id}\n:END:\n#+title: {date}\n")
}

/// A timestamped daily-note entry: a `* HH:MM text` headline.
#[must_use]
pub fn daily_entry(time: &str, text: &str) -> String {
    format!("* {time} {text}\n")
}

/// Locate (start, end) line indices of the file-level property drawer — the
/// `:PROPERTIES:` … `:END:` block at the very top (allowing leading blank lines).
/// Returns the half-open line range covering both delimiters.
fn drawer_range(lines: &[&str]) -> Option<(usize, usize)> {
    let mut i = 0;
    while i < lines.len() && lines[i].trim().is_empty() {
        i += 1;
    }
    if i >= lines.len() || !lines[i].trim().eq_ignore_ascii_case(":PROPERTIES:") {
        return None;
    }
    let start = i;
    i += 1;
    while i < lines.len() {
        if lines[i].trim().eq_ignore_ascii_case(":END:") {
            return Some((start, i + 1));
        }
        i += 1;
    }
    None
}

/// Append a `:KEY: value` line to the file-level property drawer (creating the
/// drawer at the top if absent). If a line for `key` already exists, `value` is
/// appended to it separated by a space (org-roam stores multi-valued properties
/// such as `ROAM_ALIASES` / `ROAM_REFS` this way).
#[must_use]
pub fn append_property(content: &str, key: &str, value: &str) -> String {
    let key = key.to_ascii_uppercase();
    let mut lines: Vec<String> = content.split('\n').map(str::to_string).collect();
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    if let Some((start, end)) = drawer_range(&refs) {
        let want = format!(":{key}:");
        for line in lines.iter_mut().take(end).skip(start + 1) {
            if line.trim().to_ascii_uppercase().starts_with(&want) {
                let _ = write!(line, " {value}");
                return lines.join("\n");
            }
        }
        lines.insert(end - 1, format!(":{key}: {value}"));
        return lines.join("\n");
    }
    format!(":PROPERTIES:\n:{key}: {value}\n:END:\n{content}")
}

/// Add `tag` to the file's `#+filetags:` line (Org's `:tag1:tag2:` form),
/// creating the line just after `#+title:` (or at the top) if absent. A tag
/// already present is left untouched.
#[must_use]
pub fn add_filetag(content: &str, tag: &str) -> String {
    let tag = tag.trim().trim_matches(':');
    if tag.is_empty() {
        return content.to_string();
    }
    let mut lines: Vec<String> = content.split('\n').map(str::to_string).collect();
    for line in &mut lines {
        let lower = line.trim_start().to_ascii_lowercase();
        if lower.starts_with("#+filetags:") {
            if line.split(':').any(|t| t == tag) {
                return content.to_string();
            }
            if !line.trim_end().ends_with(':') {
                line.push(':');
            }
            let _ = write!(line, "{tag}:");
            return lines.join("\n");
        }
    }
    // No filetags line: add one right after #+title:, else at the very top.
    let new = format!("#+filetags: :{tag}:");
    let pos = lines
        .iter()
        .position(|l| l.trim_start().to_ascii_lowercase().starts_with("#+title:"))
        .map_or(0, |i| i + 1);
    lines.insert(pos, new);
    lines.join("\n")
}

static ID_LINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[\[id:([^\]]+)\]").expect("id link regex"));

/// Compile a **backlinks** Org buffer for the node identified by `target_id`
/// (and shown as `target_title`). Scans `files` (`(name, content)`) for
/// `[[id:<target_id>]…]` links, listing each linking file with the line that
/// contains the link. Also lists *unlinked references*: files that mention
/// `target_title` in text but do not link to it.
#[must_use]
pub fn backlinks(target_id: &str, target_title: &str, files: &[(String, String)]) -> String {
    let mut linked: Vec<(String, String)> = Vec::new();
    let mut unlinked: Vec<(String, String)> = Vec::new();
    let needle = format!("id:{target_id}]");
    let title_lc = target_title.to_ascii_lowercase();
    for (name, content) in files {
        let mut did_link = false;
        for line in content.lines() {
            if !target_id.is_empty() && line.contains(&needle) {
                linked.push((name.clone(), line.trim().to_string()));
                did_link = true;
            }
        }
        if !did_link && !title_lc.is_empty() {
            for line in content.lines() {
                if line.to_ascii_lowercase().contains(&title_lc)
                    && !line.trim_start().starts_with("#+title:")
                {
                    unlinked.push((name.clone(), line.trim().to_string()));
                }
            }
        }
    }
    let mut out = format!("#+title: Backlinks: {target_title}\n");
    let _ = write!(out, "\n* Linked references ({})\n", linked.len());
    for (name, line) in &linked {
        let _ = writeln!(out, "- [[file:{name}][{name}]] :: {line}");
    }
    let _ = write!(out, "\n* Unlinked references ({})\n", unlinked.len());
    for (name, line) in &unlinked {
        let _ = writeln!(out, "- [[file:{name}][{name}]] :: {line}");
    }
    out
}

/// Compile a **Mermaid** flowchart of the node graph: one node per file that has
/// both an `:ID:` and a `#+title:`, with an edge for every `[[id:…]]` link that
/// resolves to another node. Wrap the result in an Org `#+begin_src mermaid`
/// block so it renders in tools that support it.
#[must_use]
pub fn graph(files: &[(String, String)]) -> String {
    // Assign a stable index per id, in file order.
    let mut ids: Vec<(String, String)> = Vec::new(); // (id, title)
    for (_, content) in files {
        if let (Some(id), Some(title)) = (node_id(content), node_title(content))
            && !ids.iter().any(|(i, _)| *i == id)
        {
            ids.push((id, title));
        }
    }
    let index = |id: &str| ids.iter().position(|(i, _)| i == id);
    let mut out = String::from("#+title: Roam Graph\n\n#+begin_src mermaid\nflowchart LR\n");
    for (n, (_, title)) in ids.iter().enumerate() {
        let safe = title.replace('"', "'");
        let _ = writeln!(out, "\tn{n}[\"{safe}\"]");
    }
    for (_, content) in files {
        let Some(from) = node_id(content).and_then(|id| index(&id)) else {
            continue;
        };
        for cap in ID_LINK.captures_iter(content) {
            if let Some(to) = index(cap[1].trim())
                && to != from
            {
                let _ = writeln!(out, "\tn{from} --> n{to}");
            }
        }
    }
    out.push_str("#+end_src\n");
    out
}

/// Compile a node **index**: a sortable Org table of every node's title, file,
/// and `#+filetags:`. Files without a `#+title:` are skipped.
#[must_use]
pub fn index(files: &[(String, String)]) -> String {
    let mut rows: Vec<(String, String, String)> = Vec::new();
    for (name, content) in files {
        let Some(title) = node_title(content) else {
            continue;
        };
        let tags = content
            .lines()
            .find(|l| {
                l.trim_start()
                    .to_ascii_lowercase()
                    .starts_with("#+filetags:")
            })
            .map(|l| {
                let value = l.split_once(':').map_or("", |(_, r)| r).trim();
                value
                    .split(':')
                    .filter(|t| !t.trim().is_empty())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        rows.push((title, name.clone(), tags));
    }
    rows.sort();
    let mut out = format!(
        "#+title: Roam Nodes ({})\n\n| Title | File | Tags |\n|-+-+-|\n",
        rows.len()
    );
    for (title, name, tags) in &rows {
        let _ = writeln!(out, "| {title} | [[file:{name}][{name}]] | {tags} |");
    }
    out
}

/// Report **dead `id:` links**: `[[id:…]]` links whose target ID is not declared
/// by any node in `files`. Lists each broken link with its file and the line it
/// appears on, into an Org buffer.
#[must_use]
pub fn dead_links(files: &[(String, String)]) -> String {
    let mut defined: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (_, content) in files {
        defined.extend(all_ids(content));
    }
    let mut dead: Vec<(String, String, String)> = Vec::new(); // file, id, line
    for (name, content) in files {
        for line in content.lines() {
            for cap in ID_LINK.captures_iter(line) {
                let id = cap[1].trim().to_string();
                if !defined.contains(&id) {
                    dead.push((name.clone(), id, line.trim().to_string()));
                }
            }
        }
    }
    let mut out = format!("#+title: Dead Links ({})\n\n", dead.len());
    for (name, id, line) in &dead {
        let _ = writeln!(out, "- [[file:{name}][{name}]] :: id:{id} :: {line}");
    }
    if dead.is_empty() {
        out.push_str("No dead links.\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_normalizes() {
        assert_eq!(slugify("Hello, World!"), "hello-world");
        assert_eq!(slugify("  Spaced   Out  "), "spaced-out");
        assert_eq!(slugify("café_2024"), "caf-2024");
        assert_eq!(slugify("!!!"), "node");
    }

    #[test]
    fn nodeify_adds_id_drawer() {
        // No drawer → a fresh one is created beneath the headline.
        let out = nodeify("* Heading\nbody\n", 0, "NID").unwrap();
        assert_eq!(
            out,
            "* Heading\n:PROPERTIES:\n:ID:       NID\n:END:\nbody\n"
        );
        // Existing drawer → the ID is inserted into it.
        let with_drawer = "* H\n:PROPERTIES:\n:FOO: bar\n:END:\n";
        let out = nodeify(with_drawer, 0, "NID").unwrap();
        assert!(out.contains(":ID:       NID"));
        assert!(out.contains(":FOO: bar"));
        // Already a node, or not a headline → None.
        assert!(nodeify(&out, 0, "X").is_none());
        assert!(nodeify("plain line\n", 0, "X").is_none());
    }

    #[test]
    fn all_ids_and_dead_links() {
        let files = vec![
            (
                "a.org".to_string(),
                ":PROPERTIES:\n:ID:       G1\n:END:\n#+title: A\nsee [[id:G2][B]] and [[id:GONE][x]]\n".to_string(),
            ),
            (":b".to_string(), ":PROPERTIES:\n:ID:       G2\n:END:\n#+title: B\n".to_string()),
        ];
        assert_eq!(all_ids(&files[0].1), vec!["G1".to_string()]);
        let report = dead_links(&files);
        assert!(report.contains("Dead Links (1)"));
        assert!(report.contains(":: id:GONE ::"));
        assert!(!report.contains(":: id:G2 ::"), "G2 is defined, not dead");
    }

    #[test]
    fn transclusion_directive() {
        assert_eq!(transclusion("I1", "T"), "#+transclude: [[id:I1][T]]");
    }

    #[test]
    fn parses_title_and_id() {
        let n = new_node("My Note", "ABC-123");
        assert_eq!(node_title(&n).as_deref(), Some("My Note"));
        assert_eq!(node_id(&n).as_deref(), Some("ABC-123"));
        assert_eq!(node_title("nothing here"), None);
    }

    #[test]
    fn node_link_format() {
        assert_eq!(node_link("ID1", "Title"), "[[id:ID1][Title]]");
    }

    #[test]
    fn append_property_creates_and_extends() {
        let n = new_node("N", "I1");
        let a = append_property(&n, "ROAM_ALIASES", "\"alt\"");
        assert!(a.contains(":ROAM_ALIASES: \"alt\""));
        let b = append_property(&a, "ROAM_ALIASES", "\"second\"");
        assert!(b.contains(":ROAM_ALIASES: \"alt\" \"second\""));
        // No drawer present → one is created at the top.
        let c = append_property("#+title: X\n", "ROAM_REFS", "https://e.com");
        assert!(c.starts_with(":PROPERTIES:\n:ROAM_REFS: https://e.com\n:END:\n#+title: X"));
    }

    #[test]
    fn add_filetag_dedups_and_creates() {
        let n = "#+title: N\nbody\n";
        let a = add_filetag(n, "work");
        assert!(a.contains("#+filetags: :work:"));
        // Tag goes right after the title line.
        assert_eq!(a.lines().nth(1), Some("#+filetags: :work:"));
        let b = add_filetag(&a, "urgent");
        assert!(b.contains("#+filetags: :work:urgent:"));
        // Re-adding an existing tag is a no-op.
        assert_eq!(add_filetag(&b, "work"), b);
    }

    #[test]
    fn backlinks_separate_linked_and_unlinked() {
        let files = vec![
            (
                "a.org".to_string(),
                "#+title: A\nSee [[id:T1][Target]] here\n".to_string(),
            ),
            (
                "b.org".to_string(),
                "#+title: B\nI mention Target in prose\n".to_string(),
            ),
            ("c.org".to_string(), "#+title: C\nunrelated\n".to_string()),
        ];
        let out = backlinks("T1", "Target", &files);
        assert!(out.contains("* Linked references (1)"));
        assert!(out.contains("[[file:a.org][a.org]]"));
        assert!(out.contains("* Unlinked references (1)"));
        assert!(out.contains("[[file:b.org][b.org]]"));
        assert!(!out.contains("c.org"));
    }

    #[test]
    fn graph_emits_nodes_and_edges() {
        let files = vec![
            (
                "a.org".to_string(),
                ":PROPERTIES:\n:ID:       T1\n:END:\n#+title: Alpha\nlink [[id:T2][Beta]]\n"
                    .to_string(),
            ),
            (
                ":b".to_string(),
                ":PROPERTIES:\n:ID:       T2\n:END:\n#+title: Beta\n".to_string(),
            ),
        ];
        let g = graph(&files);
        assert!(g.contains("flowchart LR"));
        assert!(g.contains("n0[\"Alpha\"]"));
        assert!(g.contains("n1[\"Beta\"]"));
        assert!(g.contains("n0 --> n1"));
        assert!(g.contains("#+begin_src mermaid"));
    }

    #[test]
    fn index_tabulates_titles() {
        let files = vec![
            (
                "z.org".to_string(),
                "#+title: Zebra\n#+filetags: :animal:\n".to_string(),
            ),
            ("a.org".to_string(), "#+title: Apple\n".to_string()),
            ("none.org".to_string(), "no title here\n".to_string()),
        ];
        let idx = index(&files);
        assert!(idx.contains("Roam Nodes (2)"));
        // Sorted by title: Apple before Zebra.
        assert!(idx.find("Apple").unwrap() < idx.find("Zebra").unwrap());
        assert!(idx.contains("| Zebra | [[file:z.org][z.org]] | animal |"));
        assert!(!idx.contains("none.org"));
    }

    #[test]
    fn daily_helpers() {
        assert_eq!(daily_filename("2026-06-28"), "2026-06-28.org");
        assert!(daily_template("2026-06-28", "D1").contains("#+title: 2026-06-28"));
        assert_eq!(daily_entry("09:30", "stand-up"), "* 09:30 stand-up\n");
    }
}
