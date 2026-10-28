//! Pure logic for talking to an HL7 FHIR server (T684).
//!
//! Everything here is terminal- and network-independent and unit-tested:
//!
//! - [`Profile`] — a server profile read from `.vix/fhir.json` (base URL plus
//!   how to authenticate). Secrets are never stored in the file: it names the
//!   *environment variables* that hold them.
//! - [`Spec`] / [`BuiltRequest`] — a `Read/Search/Create/Update/Delete/History/
//!   Capabilities` interaction and the HTTP request it becomes.
//! - [`Bundle`] / [`BundlePanel`] — a parsed Bundle (one row per entry, the
//!   `next` paging link) and the list-panel state that browses it.
//! - [`OperationOutcome`] — a server's error report as structured issues.
//!
//! The optional `net` feature adds `net`, the blocking layer that fetches a
//! token and performs a request through `vix-http-client`.

mod bundle;
mod outcome;
mod panel;
mod profile;
mod request;
mod secret;
mod url;

#[cfg(feature = "net")]
pub mod net;

pub use bundle::{Bundle, Entry, parse_bundle};
pub use outcome::{Issue, OperationOutcome, parse_operation_outcome};
pub use panel::BundlePanel;
pub use profile::{Auth, Profile, TokenRequest, parse_token_response};
pub use request::{BuiltRequest, Interaction, Spec, pretty};
pub use secret::Secret;

/// What a FHIR response body holds, by its `resourceType`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Content {
    /// A `Bundle` (search results, history, a transaction response, …).
    Bundle,
    /// An `OperationOutcome` (the server's error or information report).
    OperationOutcome,
    /// Any other resource; holds its `resourceType`.
    Resource(String),
}

/// Why a FHIR operation could not be set up or understood. Messages never
/// include response bodies or secrets, so they are safe to show and log.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum FhirError {
    /// The profile file is not valid JSON of the expected shape.
    ProfileSyntax(String),
    /// A profile field is missing or unusable; holds the field name.
    ProfileField(&'static str),
    /// A URL is not `http`/`https`, or carries credentials in its authority.
    BadUrl(String),
    /// An authenticated profile points at plain `http` on a non-loopback host.
    InsecureUrl,
    /// A secret-bearing environment variable name outside the `FHIR_` prefix
    /// (a project file must not be able to name `AWS_SECRET_ACCESS_KEY`).
    EnvNotAllowed(String),
    /// A named environment variable is unset or empty.
    EnvMissing(String),
    /// The command line named no known interaction; holds the word.
    BadInteraction(String),
    /// The resource type is not a FHIR type name (`[A-Z][A-Za-z]*`).
    BadResourceType(String),
    /// The id is not a FHIR id (`[A-Za-z0-9.-]{1,64}`).
    BadId(String),
    /// The interaction needs a resource type.
    MissingType,
    /// The interaction needs a resource id.
    MissingId,
    /// Create/Update needs a resource body.
    MissingBody,
    /// The body is not a JSON object with a `resourceType`.
    BodyNotResource,
    /// The body's `resourceType` differs from the one requested.
    BodyTypeMismatch,
    /// An Update body names a different `id` than the URL.
    BodyIdMismatch,
    /// The text is not JSON.
    NotJson,
    /// The text is JSON but not a FHIR resource (no `resourceType`).
    NotFhir,
    /// The text is a FHIR resource but not a `Bundle`.
    NotBundle,
    /// The text is a FHIR resource but not an `OperationOutcome`.
    NotOperationOutcome,
    /// A paging link points at a different origin than the profile's server;
    /// following it would send the credential to a third party.
    ForeignLink,
    /// The token endpoint's reply holds no `access_token`.
    BadTokenResponse,
    /// The server could not be reached (no detail: URLs can carry PHI).
    Transport,
}

impl std::fmt::Display for FhirError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ProfileSyntax(why) => write!(f, "invalid FHIR profile: {why}"),
            Self::ProfileField(name) => write!(f, "FHIR profile field missing or invalid: {name}"),
            Self::BadUrl(url) => write!(f, "not an http(s) URL: {url}"),
            Self::InsecureUrl => {
                f.write_str("an authenticated FHIR server must use https (or a loopback host)")
            }
            Self::EnvNotAllowed(name) => {
                write!(f, "environment variable {name} must start with FHIR_")
            }
            Self::EnvMissing(name) => write!(f, "environment variable {name} is not set"),
            Self::BadInteraction(word) => write!(
                f,
                "unknown interaction {word:?} (read, search, create, update, delete, history, capabilities)"
            ),
            Self::BadResourceType(t) => write!(f, "not a FHIR resource type: {t}"),
            Self::BadId(id) => write!(f, "not a FHIR id: {id}"),
            Self::MissingType => f.write_str("this interaction needs a resource type"),
            Self::MissingId => f.write_str("this interaction needs a resource id (Type/id)"),
            Self::MissingBody => f.write_str("this interaction needs a resource body (the buffer)"),
            Self::BodyNotResource => f.write_str("the body is not a JSON FHIR resource"),
            Self::BodyTypeMismatch => {
                f.write_str("the body's resourceType differs from the request")
            }
            Self::BodyIdMismatch => f.write_str("the body's id differs from the request URL"),
            Self::NotJson => f.write_str("not valid JSON"),
            Self::NotFhir => f.write_str("not a FHIR resource (no resourceType)"),
            Self::NotBundle => f.write_str("not a FHIR Bundle"),
            Self::NotOperationOutcome => f.write_str("not a FHIR OperationOutcome"),
            Self::ForeignLink => f.write_str("paging link points at a different server"),
            Self::BadTokenResponse => f.write_str("the token endpoint returned no access_token"),
            Self::Transport => f.write_str("the server could not be reached"),
        }
    }
}

impl std::error::Error for FhirError {}

/// Classify a response body by its `resourceType`.
///
/// # Errors
/// [`FhirError::NotJson`] when `text` is not JSON and [`FhirError::NotFhir`]
/// when it has no string `resourceType`.
pub fn classify(text: &str) -> Result<Content, FhirError> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|_| FhirError::NotJson)?;
    match value
        .get("resourceType")
        .and_then(serde_json::Value::as_str)
    {
        Some("Bundle") => Ok(Content::Bundle),
        Some("OperationOutcome") => Ok(Content::OperationOutcome),
        Some(other) => Ok(Content::Resource(other.to_string())),
        None => Err(FhirError::NotFhir),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_by_resource_type() {
        assert_eq!(
            classify(r#"{"resourceType":"Bundle"}"#),
            Ok(Content::Bundle)
        );
        assert_eq!(
            classify(r#"{"resourceType":"OperationOutcome"}"#),
            Ok(Content::OperationOutcome)
        );
        assert_eq!(
            classify(r#"{"resourceType":"Patient"}"#),
            Ok(Content::Resource("Patient".into()))
        );
        assert_eq!(classify("{}"), Err(FhirError::NotFhir));
        assert_eq!(classify("nope"), Err(FhirError::NotJson));
    }
}
