//! A small `XPath` subset for `{{name.response.body.//xpath}}` chaining.
//!
//! Supported: absolute paths of `/` (child) and `//` (descendant) steps; name
//! tests (matched on the local name, so `ns:item` and `item` both match), `*`,
//! `.`, `text()`, `@attr`; and the predicates `[n]` (1-based position among
//! the step's matches from one context node), `[@a]` and `[@a='v']`. The result is
//! the first match's text (an element's text is all its descendant text).
//! DTDs are refused by the parser, so entity expansion cannot be abused.
//!
//! Used only by [`crate::rest`] when a `{{req.response.body./path}}` reference
//! has a body path starting with `/`. Deliberately not a full `XPath` engine:
//! no axes other than child/descendant, no functions besides `text()`, no
//! boolean operators. Unsupported syntax makes [`query`] return `None`.
//!
//! Owning spec: `crates/vix-http-client/spec/index.md`.

use roxmltree::{Document, Node};

/// How a step walks from its context node: `/` or `//`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Axis {
    /// `/`: direct children.
    Child,
    /// `//`: all descendants (excluding the context node itself).
    Descendant,
}

/// What a step selects.
#[derive(Debug, PartialEq, Eq)]
enum Test {
    /// An element, by local name (namespace prefixes ignored).
    Name(String),
    /// `*`: any element.
    Any,
    /// `text()`: text nodes.
    Text,
    /// `@attr`: an attribute's value.
    Attr(String),
    /// `.`: the context node itself.
    Current,
}

/// A bracketed filter on a step.
#[derive(Debug, PartialEq, Eq)]
enum Pred {
    /// `[n]`: the n-th match, 1-based (0 is rejected at parse time).
    Index(usize),
    /// `[@a]`: has attribute.
    HasAttr(String),
    /// `[@a='v']`: attribute equals value.
    AttrEq(String, String),
}

/// One parsed path step: axis, node test, and predicates.
#[derive(Debug)]
struct Step {
    /// Child or descendant.
    axis: Axis,
    /// What the step selects.
    test: Test,
    /// Filters applied in order (only the first `[n]` is honoured).
    preds: Vec<Pred>,
}

/// A selected node, or a computed string (attribute / text node).
enum Item<'a, 'i> {
    Node(Node<'a, 'i>),
    Value(String),
}

/// Evaluate `path` against the XML `body`: the first match's text.
pub(crate) fn query(body: &str, path: &str) -> Option<String> {
    // `roxmltree` refuses DTDs by default, which is what makes entity-expansion
    // ("billion laughs") attacks impossible here.
    let doc = Document::parse(body).ok()?;
    let steps = parse(path)?;
    let mut items = vec![Item::Node(doc.root())];
    for step in &steps {
        let mut next = Vec::new();
        for item in &items {
            let Item::Node(n) = item else { continue };
            apply(*n, step, &mut next);
        }
        items = next;
    }
    match items.into_iter().next()? {
        Item::Value(v) => Some(v),
        Item::Node(n) => Some(
            n.descendants()
                .filter(Node::is_text)
                .filter_map(|t| t.text())
                .collect(),
        ),
    }
}

/// The local part of a possibly prefixed name (`ns:item` -> `item`).
fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

/// Apply one `step` to context node `ctx`, appending its matches to `out`.
fn apply<'a, 'i>(ctx: Node<'a, 'i>, step: &Step, out: &mut Vec<Item<'a, 'i>>) {
    let mut found: Vec<Item<'a, 'i>> = Vec::new();
    let candidates: Vec<Node<'a, 'i>> = match step.axis {
        Axis::Child => ctx.children().collect(),
        Axis::Descendant => ctx.descendants().skip(1).collect(),
    };
    match &step.test {
        Test::Current => found.push(Item::Node(ctx)),
        Test::Attr(a) => {
            // `//@id` looks at every descendant element; `/@id` at the context.
            let holders: Vec<Node<'a, 'i>> = match step.axis {
                Axis::Child => vec![ctx],
                Axis::Descendant => candidates.clone(),
            };
            for h in holders {
                if let Some(v) = h.attribute(a.as_str()) {
                    found.push(Item::Value(v.to_string()));
                }
            }
        }
        Test::Text => {
            for c in candidates.iter().filter(|c| c.is_text()) {
                found.push(Item::Value(c.text().unwrap_or("").to_string()));
            }
        }
        Test::Name(n) => {
            for c in candidates
                .iter()
                .filter(|c| c.is_element() && c.tag_name().name() == local(n))
                .filter(|c| step.preds.iter().all(|p| attr_pred(c, p)))
            {
                found.push(Item::Node(*c));
            }
            positional(step, &mut found);
        }
        Test::Any => {
            for c in candidates
                .iter()
                .filter(|c| c.is_element())
                .filter(|c| step.preds.iter().all(|p| attr_pred(c, p)))
            {
                found.push(Item::Node(*c));
            }
            positional(step, &mut found);
        }
    }
    out.extend(found);
}

/// Whether element `n` passes the attribute predicates; `Index` is ignored
/// here and handled afterwards by [`positional`].
fn attr_pred(n: &Node<'_, '_>, p: &Pred) -> bool {
    match p {
        Pred::Index(_) => true,
        Pred::HasAttr(a) => n.attribute(a.as_str()).is_some(),
        Pred::AttrEq(a, v) => n.attribute(a.as_str()) == Some(v.as_str()),
    }
}

/// Apply a `[n]` predicate: keep only the n-th match.
fn positional(step: &Step, found: &mut Vec<Item<'_, '_>>) {
    let index = step.preds.iter().find_map(|p| match p {
        Pred::Index(i) => Some(*i),
        _ => None,
    });
    if let Some(i) = index {
        let keep = found.drain(..).nth(i - 1);
        found.extend(keep);
    }
}

/// Parse an absolute path into steps; `None` for anything unsupported.
fn parse(path: &str) -> Option<Vec<Step>> {
    if !path.starts_with('/') {
        return None;
    }
    let mut steps = Vec::new();
    let mut rest = path;
    while !rest.is_empty() {
        let (axis, after) = match rest.strip_prefix("//") {
            Some(a) => (Axis::Descendant, a),
            None => (Axis::Child, rest.strip_prefix('/')?),
        };
        let end = step_end(after);
        let (text, tail) = after.split_at(end);
        steps.push(parse_step(axis, text)?);
        rest = tail;
    }
    (!steps.is_empty()).then_some(steps)
}

/// Index of the `/` ending a step, ignoring any inside `[...]` or quotes.
fn step_end(s: &str) -> usize {
    let (mut depth, mut quote) = (0_u32, None);
    for (i, c) in s.char_indices() {
        match (c, quote) {
            ('\'' | '"', None) => quote = Some(c),
            (q, Some(open)) if q == open => quote = None,
            ('[', None) => depth += 1,
            (']', None) => depth = depth.saturating_sub(1),
            ('/', None) if depth == 0 => return i,
            _ => {}
        }
    }
    s.len()
}

/// Parse one step's text, e.g. `item[@id='1'][2]`, into a [`Step`].
fn parse_step(axis: Axis, text: &str) -> Option<Step> {
    let (head, mut preds_text) = match text.find('[') {
        Some(i) => text.split_at(i),
        None => (text, ""),
    };
    let kind = match head {
        "" => return None,
        "*" => Test::Any,
        "." => Test::Current,
        "text()" => Test::Text,
        h => match h.strip_prefix('@') {
            Some(a) => Test::Attr(local(a).to_string()),
            None => Test::Name(h.to_string()),
        },
    };
    let mut preds = Vec::new();
    while !preds_text.is_empty() {
        let inner = preds_text.strip_prefix('[')?;
        let close = inner.find(']')?;
        preds.push(parse_pred(inner[..close].trim())?);
        preds_text = &inner[close + 1..];
    }
    Some(Step {
        axis,
        test: kind,
        preds,
    })
}

/// Parse the inside of one `[...]` predicate.
fn parse_pred(p: &str) -> Option<Pred> {
    if let Ok(n) = p.parse::<usize>() {
        return (n >= 1).then_some(Pred::Index(n));
    }
    let a = p.strip_prefix('@')?;
    match a.split_once('=') {
        None => Some(Pred::HasAttr(a.trim().to_string())),
        Some((name, value)) => {
            let v = value.trim();
            let v = v
                .strip_prefix('\'')
                .and_then(|v| v.strip_suffix('\''))
                .or_else(|| v.strip_prefix('"').and_then(|v| v.strip_suffix('"')))?;
            Some(Pred::AttrEq(name.trim().to_string(), v.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::query;

    const XML: &str = r#"<?xml version="1.0"?>
<r:root xmlns:r="urn:x">
  <items>
    <item id="a" kind="x">one</item>
    <item id="b">two <b>bold</b></item>
  </items>
  <token>t0k</token>
</r:root>"#;

    #[test]
    fn selects_by_path_descendant_index_and_attribute() {
        assert_eq!(query(XML, "/root/token").as_deref(), Some("t0k"));
        assert_eq!(query(XML, "//token").as_deref(), Some("t0k"));
        assert_eq!(query(XML, "//item[2]").as_deref(), Some("two bold"));
        assert_eq!(query(XML, "//item[@id='b']/b").as_deref(), Some("bold"));
        assert_eq!(query(XML, "//item[@kind]").as_deref(), Some("one"));
        assert_eq!(query(XML, "/root/items/item[2]/@id").as_deref(), Some("b"));
        assert_eq!(query(XML, "//item/@id").as_deref(), Some("a"));
        assert_eq!(query(XML, "//token/text()").as_deref(), Some("t0k"));
        assert_eq!(query(XML, "/root/*/item").as_deref(), Some("one"));
    }

    #[test]
    fn misses_and_bad_input_are_none() {
        assert_eq!(query(XML, "//nope"), None);
        assert_eq!(query(XML, "//item[3]"), None);
        assert_eq!(query(XML, "//item[0]"), None);
        assert_eq!(query(XML, "token"), None);
        assert_eq!(query(XML, "//item[@id=b]"), None);
        assert_eq!(query("not xml", "//a"), None);
        assert_eq!(query(XML, "//"), None);
    }

    #[test]
    fn a_doctype_is_refused() {
        let bomb = "<!DOCTYPE a [<!ENTITY e \"x\">]><a>&e;</a>";
        assert_eq!(query(bomb, "/a"), None);
    }

    #[test]
    fn a_slash_inside_a_quoted_predicate_does_not_split_the_step() {
        let xml = r#"<a><u href="x/y">hit</u></a>"#;
        assert_eq!(query(xml, "//u[@href='x/y']").as_deref(), Some("hit"));
    }
}
