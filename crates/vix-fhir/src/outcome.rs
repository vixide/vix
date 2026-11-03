//! `OperationOutcome` parsing: the server's structured error/information report.
//!
//! Lenient like the Bundle parser: missing `severity` defaults to `error` and
//! missing `code` to `unknown` (the conservative reading of a malformed issue).
//! Spec: `crates/vix-fhir/spec/index.md`.

use serde_json::Value;

use crate::FhirError;

/// One issue of an `OperationOutcome`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    /// `fatal`, `error`, `warning` or `information`.
    pub severity: String,
    /// The FHIR issue-type code (`invalid`, `not-found`, `processing`, …).
    pub code: String,
    /// Human-readable detail: `diagnostics`, else `details.text`, else the
    /// first `details.coding` display; empty when the server gave none.
    pub detail: String,
    /// `FHIRPath` expressions (or legacy `location` values) the issue points at.
    pub expression: Vec<String>,
}

/// A parsed `OperationOutcome`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationOutcome {
    /// The issues, in server order.
    pub issues: Vec<Issue>,
}

/// Collect the string elements of a JSON array, ignoring non-strings; empty when
/// `v` is absent or not an array.
fn string_list(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

/// The most human-readable detail of an issue, in priority order:
/// `diagnostics`, `details.text`, first `details.coding[].display`; else empty.
fn detail_of(issue: &Value) -> String {
    if let Some(d) = issue.get("diagnostics").and_then(Value::as_str) {
        return d.to_string();
    }
    let details = issue.get("details");
    if let Some(t) = details.and_then(|d| d.get("text")).and_then(Value::as_str) {
        return t.to_string();
    }
    details
        .and_then(|d| d.get("coding"))
        .and_then(Value::as_array)
        .and_then(|c| {
            c.iter()
                .find_map(|c| c.get("display").and_then(Value::as_str))
        })
        .unwrap_or_default()
        .to_string()
}

/// Parse an `OperationOutcome` resource.
///
/// `expression` falls back to the legacy `location` field when empty.
///
/// # Examples
/// ```
/// let oo = vix_fhir::parse_operation_outcome(
///     r#"{"resourceType":"OperationOutcome","issue":[{"severity":"error","code":"invalid","diagnostics":"Bad date"}]}"#,
/// ).unwrap();
/// assert_eq!(oo.issues[0].detail, "Bad date");
/// ```
///
/// # Errors
/// [`FhirError::NotJson`], [`FhirError::NotFhir`] or
/// [`FhirError::NotOperationOutcome`].
pub fn parse_operation_outcome(text: &str) -> Result<OperationOutcome, FhirError> {
    let value: Value = serde_json::from_str(text).map_err(|_| FhirError::NotJson)?;
    match value.get("resourceType").and_then(Value::as_str) {
        Some("OperationOutcome") => {}
        Some(_) => return Err(FhirError::NotOperationOutcome),
        None => return Err(FhirError::NotFhir),
    }
    let issues = value
        .get("issue")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|i| {
                    let mut expression = string_list(i.get("expression"));
                    if expression.is_empty() {
                        expression = string_list(i.get("location"));
                    }
                    Issue {
                        severity: i
                            .get("severity")
                            .and_then(Value::as_str)
                            .unwrap_or("error")
                            .to_string(),
                        code: i
                            .get("code")
                            .and_then(Value::as_str)
                            .unwrap_or("unknown")
                            .to_string(),
                        detail: detail_of(i),
                        expression,
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(OperationOutcome { issues })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_issues() {
        let oo = parse_operation_outcome(
            r#"{"resourceType":"OperationOutcome","issue":[
              {"severity":"error","code":"invalid","diagnostics":"Bad date","expression":["Patient.birthDate"]},
              {"severity":"warning","code":"informational","details":{"text":"Careful"},"location":["Patient"]},
              {"severity":"information","code":"informational","details":{"coding":[{"display":"All ok"}]}},
              {}
            ]}"#,
        )
        .unwrap();
        assert_eq!(oo.issues.len(), 4);
        assert_eq!(oo.issues[0].detail, "Bad date");
        assert_eq!(oo.issues[0].expression, ["Patient.birthDate"]);
        assert_eq!(oo.issues[1].detail, "Careful");
        assert_eq!(oo.issues[1].expression, ["Patient"]);
        assert_eq!(oo.issues[2].detail, "All ok");
        assert_eq!(oo.issues[3].severity, "error");
        assert_eq!(oo.issues[3].code, "unknown");
    }

    #[test]
    fn wrong_kinds_are_typed_errors() {
        assert_eq!(
            parse_operation_outcome(r#"{"resourceType":"Patient"}"#),
            Err(FhirError::NotOperationOutcome)
        );
        assert_eq!(parse_operation_outcome("{}"), Err(FhirError::NotFhir));
        assert_eq!(parse_operation_outcome("x"), Err(FhirError::NotJson));
    }
}
