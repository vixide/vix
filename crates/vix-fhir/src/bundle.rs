//! `Bundle` parsing: one [`Entry`] per `entry`, plus paging links.
//!
//! Parsing is deliberately lenient: it walks a generic [`serde_json::Value`] and
//! tolerates missing or wrongly typed fields (they become empty/`None`) so a
//! hostile or partial server reply can never panic the viewer. Only the
//! top-level `resourceType` is validated. The result feeds
//! [`BundlePanel`](crate::BundlePanel). Spec: `crates/vix-fhir/spec/index.md`.

use serde_json::Value;

use crate::FhirError;

/// One row of a Bundle.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// The entry's `fullUrl`, if any.
    pub full_url: Option<String>,
    /// The resource's `resourceType`; empty when the entry holds no resource
    /// (e.g. a transaction-response entry).
    pub resource_type: String,
    /// The resource's logical `id`.
    pub id: Option<String>,
    /// A short human label drawn from the resource (name, code, title, …);
    /// empty when none applies.
    pub summary: String,
    /// `response.status` of a transaction/batch response entry.
    pub status: Option<String>,
    /// `search.mode` (`match`, `include`, `outcome`) of a searchset entry.
    pub search_mode: Option<String>,
    /// The resource itself (`Value::Null` when absent).
    pub resource: Value,
}

impl Entry {
    /// `Type/id` (or whichever parts exist); falls back to `fullUrl`.
    #[must_use]
    pub fn reference(&self) -> String {
        match (self.resource_type.is_empty(), &self.id) {
            (false, Some(id)) => format!("{}/{id}", self.resource_type),
            (false, None) => self.resource_type.clone(),
            _ => self.full_url.clone().unwrap_or_default(),
        }
    }

    /// The resource pretty-printed, or `None` for an entry with no resource.
    #[must_use]
    pub fn resource_pretty(&self) -> Option<String> {
        if self.resource.is_null() {
            return None;
        }
        serde_json::to_string_pretty(&self.resource).ok()
    }
}

/// A parsed `Bundle`.
#[derive(Debug, Clone, PartialEq)]
pub struct Bundle {
    /// `Bundle.type` (`searchset`, `history`, `transaction-response`, …).
    pub bundle_type: String,
    /// `Bundle.total`, when the server reports it.
    pub total: Option<u64>,
    /// The entries, in order.
    pub entries: Vec<Entry>,
    /// The `next` page link.
    pub next: Option<String>,
    /// The `previous` page link.
    pub previous: Option<String>,
}

/// The first non-empty string found by trying each of `paths` in `v` in order.
///
/// Each path is a list of keys; a key that parses as a number indexes into an
/// array (so `["code","coding","0","display"]` is `code.coding[0].display`).
fn first_string(v: &Value, paths: &[&[&str]]) -> Option<String> {
    paths.iter().find_map(|path| {
        let mut cur = v;
        for key in *path {
            cur = match key.parse::<usize>() {
                Ok(i) => cur.get(i)?,
                Err(_) => cur.get(*key)?,
            };
        }
        cur.as_str().filter(|s| !s.is_empty()).map(String::from)
    })
}

/// `family, given given` from a `HumanName` list, or a plain `name` string.
fn name_of(resource: &Value) -> Option<String> {
    let name = resource.get("name")?;
    if let Some(s) = name.as_str() {
        return Some(s.to_string());
    }
    let first = name.as_array()?.first()?;
    if let Some(text) = first.get("text").and_then(Value::as_str) {
        return Some(text.to_string());
    }
    let family = first.get("family").and_then(Value::as_str).unwrap_or("");
    let given: Vec<&str> = first
        .get("given")
        .and_then(Value::as_array)
        .map(|g| g.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let given = given.join(" ");
    match (family.is_empty(), given.is_empty()) {
        (true, true) => None,
        (false, true) => Some(family.to_string()),
        (true, false) => Some(given),
        (false, false) => Some(format!("{family}, {given}")),
    }
}

/// A short human label for a resource: its name if it has one, else the first
/// of a fixed list of descriptive fields (code text, coding display, title,
/// description, type text, status); empty when none apply. Drives the `summary`
/// column of the Bundle list.
fn summary_of(resource: &Value) -> String {
    name_of(resource)
        .or_else(|| {
            first_string(
                resource,
                &[
                    &["code", "text"],
                    &["code", "coding", "0", "display"],
                    &["title"],
                    &["description"],
                    &["type", "text"],
                    &["status"],
                ],
            )
        })
        .unwrap_or_default()
}

/// The `url` of the first Bundle `link` whose `relation` matches (`next`,
/// `previous`, ...).
fn link(links: &[Value], relation: &str) -> Option<String> {
    links.iter().find_map(|l| {
        (l.get("relation").and_then(Value::as_str) == Some(relation))
            .then(|| l.get("url").and_then(Value::as_str).map(String::from))
            .flatten()
    })
}

/// Build an [`Entry`] from one `Bundle.entry` object. Never fails: absent parts
/// become empty strings, `None`, or `Value::Null`.
fn parse_entry(e: &Value) -> Entry {
    let resource = e.get("resource").cloned().unwrap_or(Value::Null);
    let text = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(String::from);
    Entry {
        full_url: text(e, "fullUrl"),
        resource_type: text(&resource, "resourceType").unwrap_or_default(),
        id: text(&resource, "id"),
        summary: summary_of(&resource),
        status: e.get("response").and_then(|r| text(r, "status")),
        search_mode: e.get("search").and_then(|s| text(s, "mode")),
        resource,
    }
}

/// Parse a `Bundle` resource.
///
/// The paging links are returned as-is; this function does not check that they
/// stay on the profile's server (see [`FhirError::ForeignLink`] and the request
/// layer for that).
///
/// # Examples
/// ```
/// let b = vix_fhir::parse_bundle(
///     r#"{"resourceType":"Bundle","type":"searchset","total":1,
///         "entry":[{"resource":{"resourceType":"Patient","id":"7"}}]}"#,
/// ).unwrap();
/// assert_eq!(b.entries[0].reference(), "Patient/7");
/// ```
///
/// # Errors
/// [`FhirError::NotJson`], [`FhirError::NotFhir`] or [`FhirError::NotBundle`].
pub fn parse_bundle(text: &str) -> Result<Bundle, FhirError> {
    let value: Value = serde_json::from_str(text).map_err(|_| FhirError::NotJson)?;
    match value.get("resourceType").and_then(Value::as_str) {
        Some("Bundle") => {}
        Some(_) => return Err(FhirError::NotBundle),
        None => return Err(FhirError::NotFhir),
    }
    let links = value
        .get("link")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    Ok(Bundle {
        bundle_type: value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        total: value.get("total").and_then(Value::as_u64),
        entries: value
            .get("entry")
            .and_then(Value::as_array)
            .map(|a| a.iter().map(parse_entry).collect())
            .unwrap_or_default(),
        next: link(links, "next"),
        previous: link(links, "previous"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEARCHSET: &str = r#"{
      "resourceType":"Bundle","type":"searchset","total":3,
      "link":[{"relation":"self","url":"https://s/Patient"},
              {"relation":"next","url":"https://s/Patient?page=2"}],
      "entry":[
        {"fullUrl":"https://s/Patient/1","resource":{"resourceType":"Patient","id":"1",
          "name":[{"family":"Smith","given":["John","Q"]}]},"search":{"mode":"match"}},
        {"resource":{"resourceType":"Observation","id":"o1","code":{"coding":[{"display":"Heart rate"}]}}},
        {"resource":{"resourceType":"Organization","id":"org","name":"Acme"}}
      ]}"#;

    #[test]
    fn parses_entries_and_links() {
        let b = parse_bundle(SEARCHSET).unwrap();
        assert_eq!(b.bundle_type, "searchset");
        assert_eq!(b.total, Some(3));
        assert_eq!(b.next.as_deref(), Some("https://s/Patient?page=2"));
        assert_eq!(b.previous, None);
        assert_eq!(b.entries.len(), 3);
        assert_eq!(b.entries[0].reference(), "Patient/1");
        assert_eq!(b.entries[0].summary, "Smith, John Q");
        assert_eq!(b.entries[0].search_mode.as_deref(), Some("match"));
        assert_eq!(b.entries[1].summary, "Heart rate");
        assert_eq!(b.entries[2].summary, "Acme");
        assert!(b.entries[0].resource_pretty().unwrap().contains("\n  "));
    }

    #[test]
    fn transaction_response_entries_have_no_resource() {
        let b = parse_bundle(
            r#"{"resourceType":"Bundle","type":"transaction-response",
            "entry":[{"fullUrl":"urn:uuid:1","response":{"status":"201 Created"}}]}"#,
        )
        .unwrap();
        assert_eq!(b.entries[0].status.as_deref(), Some("201 Created"));
        assert_eq!(b.entries[0].reference(), "urn:uuid:1");
        assert_eq!(b.entries[0].resource_pretty(), None);
        assert_eq!(b.next, None);
    }

    #[test]
    fn empty_bundle_and_wrong_kinds() {
        assert_eq!(
            parse_bundle(r#"{"resourceType":"Bundle"}"#)
                .unwrap()
                .entries
                .len(),
            0
        );
        assert_eq!(
            parse_bundle(r#"{"resourceType":"Patient"}"#),
            Err(FhirError::NotBundle)
        );
        assert_eq!(parse_bundle("[]"), Err(FhirError::NotFhir));
        assert_eq!(parse_bundle("{"), Err(FhirError::NotJson));
    }

    #[test]
    fn hostile_shapes_never_panic() {
        for text in [
            r#"{"resourceType":"Bundle","entry":"x"}"#,
            r#"{"resourceType":"Bundle","entry":[1,null,[],{"resource":5}]}"#,
            r#"{"resourceType":"Bundle","link":[{"relation":1}],"total":-1}"#,
            r#"{"resourceType":"Bundle","entry":[{"resource":{"name":[[]]}}]}"#,
        ] {
            let _ = parse_bundle(text);
        }
    }
}
