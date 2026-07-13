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

/// Upper bound on the total number of elements a single abbreviation may expand
/// to. A multiply operator (`*N`) — and nested multiplies, which compound —
/// would otherwise let a tiny input (e.g. `div*900000000` or `a*1e5>b*1e5`)
/// force gigabytes of string growth and a multi-minute hang. Expansion beyond
/// this budget returns `None` rather than attempting the allocation.
const MAX_NODES: usize = 100_000;

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
    let mut p = Parser {
        chars,
        pos: 0,
        nodes: Vec::new(),
    };
    let roots = p.parse()?;
    if p.pos != p.chars.len() {
        return None; // unparsed remainder (e.g. unsupported `()`)
    }
    let mut out = String::new();
    let mut budget = MAX_NODES;
    for r in roots {
        p.render(r, 0, &mut out, &mut budget)?;
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
        let mut node = Node {
            tag,
            id: None,
            classes: Vec::new(),
            text: None,
            count: 1,
            parent: None,
            children: Vec::new(),
        };
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
                    // Reject an obviously explosive per-node count up front; the
                    // render budget also bounds the compounded (nested) total.
                    if node.count > MAX_NODES {
                        return None;
                    }
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
    /// `budget` is the number of elements still allowed to be emitted; it is
    /// decremented per rendered element and rendering aborts with `None` once it
    /// is exhausted, bounding the compounded cost of nested multiplies.
    fn render(&self, idx: usize, depth: usize, out: &mut String, budget: &mut usize) -> Option<()> {
        use std::fmt::Write as _;
        let node = &self.nodes[idx];
        for i in 1..=node.count.max(1) {
            *budget = budget.checked_sub(1)?;
            let indent = "  ".repeat(depth);
            let id = node
                .id
                .as_ref()
                .map(|v| format!(" id=\"{}\"", number(v, i)))
                .unwrap_or_default();
            let class = if node.classes.is_empty() {
                String::new()
            } else {
                format!(
                    " class=\"{}\"",
                    node.classes
                        .iter()
                        .map(|c| number(c, i))
                        .collect::<Vec<_>>()
                        .join(" ")
                )
            };
            let open = format!("<{}{id}{class}>", node.tag);
            if node.children.is_empty() {
                let text = node.text.as_ref().map(|t| number(t, i)).unwrap_or_default();
                let _ = writeln!(out, "{indent}{open}{text}</{}>", node.tag);
            } else {
                let _ = writeln!(out, "{indent}{open}");
                for &c in &node.children {
                    self.render(c, depth + 1, out, budget)?;
                }
                let _ = writeln!(out, "{indent}</{}>", node.tag);
            }
        }
        Some(())
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
        assert_eq!(
            expand("a#home.nav.big{Home}").unwrap(),
            "<a id=\"home\" class=\"nav big\">Home</a>\n"
        );
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

    #[test]
    fn rejects_explosive_multiply() {
        // Single huge count, compounded nested counts, and an overflow-shaped
        // literal must all be refused rather than attempt a giant allocation.
        for abbr in [
            "div*900000000",
            "span*100000>b*100000",
            "a*999999999999999999999",
        ] {
            assert!(expand(abbr).is_none(), "should refuse: {abbr}");
        }
        // Reasonable expansions right at/under the surface still work.
        assert_eq!(expand("ul>li*3").unwrap().matches("<li>").count(), 3);
    }

    #[test]
    fn expansion_stays_within_the_node_budget() {
        // Just over the budget when compounded (400*300 = 120_000 > 100_000).
        assert!(expand("div*400>span*300").is_none());
        // Comfortably under it succeeds.
        let html = expand("div*10>span*10").unwrap();
        assert_eq!(html.matches("<span>").count(), 100);
    }

    // ---- property-based ("fuzz") tests ------------------------------------

    use proptest::prelude::*;

    proptest! {
        // No abbreviation, however malformed, may panic or blow the node budget.
        #[test]
        fn expand_never_panics_and_is_bounded(abbr in ".*") {
            if let Some(html) = expand(&abbr) {
                // Total emitted open-tags cannot exceed the budget.
                let opens = html.matches('<').count();
                prop_assert!(opens <= MAX_NODES * 2, "elements exceeded budget: {opens}");
            }
        }

        // Digit-heavy multiply strings (the DoS vector) never hang or panic.
        #[test]
        fn multiply_counts_are_safe(tag in "[a-z]{1,4}", n in 0u64..u64::MAX) {
            let _ = expand(&format!("{tag}*{n}"));
        }
    }
}
