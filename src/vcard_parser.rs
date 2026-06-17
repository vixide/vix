#![allow(clippy::pedantic)] // folded subcrate: kept at its original (non-pedantic) lint level
//! A small, dependency-free vCard 4.0 ([RFC 6350]) parser.
//!
//! [`parse`] turns vCard text into a [`Vcard`] — a flat list of [`Property`]s,
//! each with a name, parameters, and an unescaped value. It handles the parts of
//! the grammar that matter for displaying a contact: line **unfolding** (a line
//! starting with a space or tab continues the previous one), the
//! `name;PARAM=value:VALUE` shape (including group prefixes like `item1.EMAIL`
//! and legacy bare `TYPE` parameters), and value **unescaping** (`\\`, `\n`,
//! `\,`, `\;`). It is pure: the host reads the `.vcf` files.
//!
//! [RFC 6350]: https://www.rfc-editor.org/info/rfc6350

#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// One vCard property: a name, its parameters, and its (unescaped) value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Property {
    /// Uppercased property name with any group prefix stripped (e.g. `EMAIL`).
    pub name: String,
    /// Parameters as `(key, value)` pairs; bare `TYPE` values use key `TYPE`.
    pub params: Vec<(String, String)>,
    /// The unescaped value text.
    pub value: String,
}

impl Property {
    /// The first parameter value for `key` (case-insensitive).
    #[must_use]
    pub fn param(&self, key: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }

    /// The `TYPE` parameter values joined with `/` (e.g. `work/voice`), if any.
    #[must_use]
    pub fn types(&self) -> Option<String> {
        let types: Vec<&str> = self
            .params
            .iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case("TYPE"))
            .map(|(_, v)| v.as_str())
            .collect();
        (!types.is_empty()).then(|| types.join("/"))
    }
}

/// A parsed vCard: its properties in document order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Vcard {
    /// All properties (excluding `BEGIN`, `END`, and `VERSION`).
    pub properties: Vec<Property>,
}

/// Parse vCard text into a [`Vcard`].
#[must_use]
pub fn parse(text: &str) -> Vcard {
    let mut properties = Vec::new();
    for line in unfold(text) {
        if line.trim().is_empty() {
            continue;
        }
        let Some(prop) = parse_line(&line) else { continue };
        if matches!(prop.name.as_str(), "BEGIN" | "END" | "VERSION") {
            continue;
        }
        properties.push(prop);
    }
    Vcard { properties }
}

/// Unfold the content lines: a line beginning with a space or tab is a
/// continuation of the previous one (its leading whitespace removed).
fn unfold(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in text.split('\n') {
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        if let Some(rest) = raw.strip_prefix([' ', '\t']) {
            if let Some(last) = out.last_mut() {
                last.push_str(rest);
                continue;
            }
        }
        out.push(raw.to_string());
    }
    out
}

/// Parse a single unfolded content line into a [`Property`].
fn parse_line(line: &str) -> Option<Property> {
    let colon = line.find(':')?;
    let (left, value) = line.split_at(colon);
    let value = unescape(&value[1..]);

    let mut parts = left.split(';');
    let raw_name = parts.next()?;
    // Strip a group prefix: "item1.EMAIL" -> "EMAIL".
    let name = raw_name.rsplit('.').next().unwrap_or(raw_name).to_ascii_uppercase();

    let mut params = Vec::new();
    for seg in parts {
        if let Some((k, v)) = seg.split_once('=') {
            // A parameter may carry a comma-separated value list (e.g.
            // TYPE=work,voice); split it into one pair each.
            for v in v.split(',') {
                params.push((k.trim().to_ascii_uppercase(), v.trim().to_string()));
            }
        } else if !seg.is_empty() {
            // Legacy bare type, e.g. "TEL;WORK:".
            params.push(("TYPE".to_string(), seg.trim().to_string()));
        }
    }
    Some(Property { name, params, value })
}

/// Unescape a vCard text value (`\\`, `\n`/`\N`, `\,`, `\;`).
fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n' | 'N') => out.push('\n'),
                Some('\\') => out.push('\\'),
                Some(',') => out.push(','),
                Some(';') => out.push(';'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

impl Vcard {
    /// The first property named `name` (case-insensitive).
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Property> {
        self.properties.iter().find(|p| p.name.eq_ignore_ascii_case(name))
    }

    /// Every property named `name` (case-insensitive), in document order.
    #[must_use]
    pub fn all(&self, name: &str) -> Vec<&Property> {
        self.properties
            .iter()
            .filter(|p| p.name.eq_ignore_ascii_case(name))
            .collect()
    }

    /// The value of the first property named `name`, if present.
    #[must_use]
    pub fn value(&self, name: &str) -> Option<&str> {
        self.get(name).map(|p| p.value.as_str())
    }

    /// A human display name: the `FN`, else a `Given Family` derived from `N`,
    /// else `"(unnamed)"`.
    #[must_use]
    pub fn display_name(&self) -> String {
        if let Some(fnv) = self.value("FN") {
            if !fnv.trim().is_empty() {
                return fnv.trim().to_string();
            }
        }
        if let Some(n) = self.value("N") {
            // N = Family;Given;Additional;Prefix;Suffix
            let f: Vec<&str> = n.split(';').collect();
            let given = f.get(1).copied().unwrap_or("").trim();
            let family = f.first().copied().unwrap_or("").trim();
            let joined = format!("{given} {family}").trim().to_string();
            if !joined.is_empty() {
                return joined;
            }
        }
        "(unnamed)".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "BEGIN:VCARD\r\nVERSION:4.0\r\nFN:Ada Lovelace\r\nN:Lovelace;Ada;;;\r\nEMAIL;TYPE=work:ada@example.com\r\nTEL;TYPE=work,voice:+1-555-0100\r\nORG:Analytical Engines\r\nNOTE:First\\nprogrammer\\, arguably.\r\nEND:VCARD\r\n";

    #[test]
    fn parses_common_properties() {
        let v = parse(SAMPLE);
        assert_eq!(v.value("FN"), Some("Ada Lovelace"));
        assert_eq!(v.display_name(), "Ada Lovelace");
        assert_eq!(v.value("ORG"), Some("Analytical Engines"));
        assert!(v.get("BEGIN").is_none(), "BEGIN/END/VERSION are dropped");
        assert!(v.get("VERSION").is_none());
    }

    #[test]
    fn unescapes_values() {
        let v = parse(SAMPLE);
        assert_eq!(v.value("NOTE"), Some("First\nprogrammer, arguably."));
    }

    #[test]
    fn params_and_types() {
        let v = parse(SAMPLE);
        let tel = v.get("TEL").unwrap();
        assert_eq!(tel.value, "+1-555-0100");
        assert_eq!(tel.types().as_deref(), Some("work/voice"));
        assert_eq!(v.get("EMAIL").unwrap().param("TYPE"), Some("work"));
    }

    #[test]
    fn unfolds_continuation_lines() {
        let folded = "BEGIN:VCARD\nFN:Very Long\n  Name Here\nEND:VCARD\n";
        assert_eq!(parse(folded).value("FN"), Some("Very Long Name Here"));
    }

    #[test]
    fn strips_group_prefix() {
        let v = parse("item1.EMAIL:x@y.z\n");
        assert_eq!(v.get("EMAIL").unwrap().value, "x@y.z");
    }

    #[test]
    fn display_name_falls_back_to_structured_n() {
        let v = parse("N:Hopper;Grace;;;\n");
        assert_eq!(v.display_name(), "Grace Hopper");
        assert_eq!(parse("").display_name(), "(unnamed)");
    }
}
