//! Building FHIR REST requests from an interaction description.
//!
//! The pipeline is `text command -> Spec -> BuiltRequest`: [`Spec::parse_command`]
//! turns the user's one-line command into a validated [`Spec`], and
//! [`Spec::build`] resolves it against a [`Profile`]. Everything user-supplied
//! that lands in the URL is either validated against a strict grammar (type, id)
//! or percent-encoded (search parameters), so a command cannot inject path
//! segments or extra query parameters. The credential is intentionally absent
//! from [`BuiltRequest`]; the `net` layer adds it. Spec:
//! `crates/vix-fhir/spec/index.md`.

use crate::url::{encode, origin};
use crate::{FhirError, Profile};

/// The FHIR `Accept` / `Content-Type` media type.
const FHIR_JSON: &str = "application/fhir+json";

/// A FHIR `RESTful` interaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interaction {
    /// `GET [base]/metadata` — the server's `CapabilityStatement`.
    Capabilities,
    /// `GET [base]/Type/id`.
    Read,
    /// `GET [base]/Type?params`.
    Search,
    /// `POST [base]/Type` with a resource body.
    Create,
    /// `PUT [base]/Type/id` with a resource body.
    Update,
    /// `DELETE [base]/Type/id`.
    Delete,
    /// `GET [base]/Type/id/_history` (or `[base]/Type/_history` without an id).
    History,
}

/// An interaction plus its target — what the user asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spec {
    /// The interaction.
    pub interaction: Interaction,
    /// The resource type (`Patient`); empty for [`Interaction::Capabilities`].
    pub resource_type: String,
    /// The logical id, when the interaction addresses one resource.
    pub id: Option<String>,
    /// Search parameters as unencoded `(name, value)` pairs.
    pub params: Vec<(String, String)>,
    /// The resource body for Create/Update.
    pub body: Option<String>,
}

/// An HTTP request ready to send, minus the credential (added by the network
/// layer so it is never part of a value that might be printed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltRequest {
    /// HTTP method.
    pub method: &'static str,
    /// Absolute URL.
    pub url: String,
    /// `(name, value)` headers: `Accept`, and `Content-Type` for writes.
    pub headers: Vec<(String, String)>,
    /// Request body; empty for reads.
    pub body: String,
}

/// Whether `t` is acceptable as a resource type: at most 64 ASCII alphanumerics
/// starting with an uppercase letter (this also admits digits, which the
/// FHIR type grammar itself never uses; harmless since it is path-safe).
fn valid_type(t: &str) -> bool {
    t.len() <= 64
        && t.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && t.chars().all(|c| c.is_ascii_alphanumeric())
}

/// Whether `id` matches the FHIR `id` grammar `[A-Za-z0-9.-]{1,64}`.
fn valid_id(id: &str) -> bool {
    (1..=64).contains(&id.len())
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
}

/// Split `a=b&c=d` into unencoded pairs; a bare `a` becomes `("a", "")`.
fn parse_params(text: &str) -> Vec<(String, String)> {
    text.trim()
        .trim_start_matches('?')
        .split('&')
        .filter(|p| !p.trim().is_empty())
        .map(|p| match p.split_once('=') {
            Some((k, v)) => (k.trim().to_string(), v.trim().to_string()),
            None => (p.trim().to_string(), String::new()),
        })
        .collect()
}

impl Spec {
    /// Parse a one-line command: `<interaction> [Type[/id]] [name=value&…]`.
    ///
    /// Interactions are `read`, `search`, `create`, `update`, `delete`,
    /// `history`, and `capabilities` (alias `metadata`), case-insensitive.
    /// The body for create/update is attached separately ([`Spec::with_body`]).
    ///
    /// # Errors
    /// [`FhirError::BadInteraction`], [`FhirError::MissingType`],
    /// [`FhirError::MissingId`], [`FhirError::BadResourceType`] or
    /// [`FhirError::BadId`].
    pub fn parse_command(input: &str) -> Result<Self, FhirError> {
        let input = input.trim();
        let (word, rest) = input
            .split_once(char::is_whitespace)
            .map_or((input, ""), |(w, r)| (w, r.trim_start()));
        let interaction = match word.to_ascii_lowercase().as_str() {
            "read" => Interaction::Read,
            "search" => Interaction::Search,
            "create" => Interaction::Create,
            "update" => Interaction::Update,
            "delete" => Interaction::Delete,
            "history" => Interaction::History,
            "capabilities" | "metadata" => Interaction::Capabilities,
            _ => return Err(FhirError::BadInteraction(word.to_string())),
        };
        let (target, params) = rest
            .split_once(char::is_whitespace)
            .map_or((rest, ""), |(t, p)| (t, p));
        let (resource_type, id) = match target.split_once('/') {
            Some((t, i)) => (t.to_string(), Some(i.to_string())),
            None => (target.to_string(), None),
        };
        let spec = Self {
            interaction,
            resource_type,
            id,
            params: parse_params(params),
            body: None,
        };
        spec.validate()?;
        Ok(spec)
    }

    /// Attach the resource body (Create/Update).
    #[must_use]
    pub fn with_body(mut self, body: String) -> Self {
        self.body = Some(body);
        self
    }

    /// Whether the interaction sends a resource (so the caller must supply one).
    #[must_use]
    pub fn needs_body(&self) -> bool {
        matches!(self.interaction, Interaction::Create | Interaction::Update)
    }

    /// Check the target is well-formed for the interaction: a valid type (except
    /// for Capabilities), an id where Read/Update/Delete require one, and a valid
    /// id whenever one is present. Run by both parsing and [`Spec::build`], since
    /// a `Spec` can also be constructed field by field.
    fn validate(&self) -> Result<(), FhirError> {
        if self.interaction == Interaction::Capabilities {
            return Ok(());
        }
        if self.resource_type.is_empty() {
            return Err(FhirError::MissingType);
        }
        if !valid_type(&self.resource_type) {
            return Err(FhirError::BadResourceType(self.resource_type.clone()));
        }
        match (&self.id, self.interaction) {
            (None, Interaction::Read | Interaction::Update | Interaction::Delete) => {
                Err(FhirError::MissingId)
            }
            (Some(id), _) if !valid_id(id) => Err(FhirError::BadId(id.clone())),
            _ => Ok(()),
        }
    }

    /// Build the HTTP request against `profile`.
    ///
    /// Search parameter names keep `:` literal (modifiers like `name:exact`) and
    /// values keep `,` literal (OR lists); everything else is percent-encoded. A
    /// stray `id` on Search/Create is ignored (the URL addresses the type only).
    ///
    /// # Examples
    /// ```
    /// use vix_fhir::{Profile, Spec};
    /// let p = Profile::parse(r#"{"base_url":"https://f.example/r4"}"#).unwrap();
    /// let r = Spec::parse_command("search Patient name=John Smith").unwrap().build(&p).unwrap();
    /// assert_eq!(r.method, "GET");
    /// assert!(r.url.starts_with("https://f.example/r4/Patient?name="));
    /// ```
    ///
    /// # Errors
    /// The validation errors of [`Spec::parse_command`], plus
    /// [`FhirError::MissingBody`], [`FhirError::BodyNotResource`],
    /// [`FhirError::BodyTypeMismatch`] and [`FhirError::BodyIdMismatch`] for a
    /// Create/Update whose body is absent or does not match the target.
    pub fn build(&self, profile: &Profile) -> Result<BuiltRequest, FhirError> {
        self.validate()?;
        let base = &profile.base_url;
        let type_path = format!("{base}/{}", self.resource_type);
        let (method, mut url) = match (self.interaction, &self.id) {
            (Interaction::Capabilities, _) => ("GET", format!("{base}/metadata")),
            (Interaction::Read, Some(id)) => ("GET", format!("{type_path}/{id}")),
            (Interaction::Search | Interaction::Create, _) => (
                if self.interaction == Interaction::Create {
                    "POST"
                } else {
                    "GET"
                },
                type_path,
            ),
            (Interaction::Update, Some(id)) => ("PUT", format!("{type_path}/{id}")),
            (Interaction::Delete, Some(id)) => ("DELETE", format!("{type_path}/{id}")),
            (Interaction::History, Some(id)) => ("GET", format!("{type_path}/{id}/_history")),
            (Interaction::History, None) => ("GET", format!("{type_path}/_history")),
            (Interaction::Read | Interaction::Update | Interaction::Delete, None) => {
                return Err(FhirError::MissingId);
            }
        };
        if !self.params.is_empty() {
            let query: Vec<String> = self
                .params
                .iter()
                .map(|(k, v)| format!("{}={}", encode(k, ":"), encode(v, ",")))
                .collect();
            url.push('?');
            url.push_str(&query.join("&"));
        }
        let mut headers = vec![("Accept".to_string(), FHIR_JSON.to_string())];
        let body = if self.needs_body() {
            headers.push(("Content-Type".to_string(), FHIR_JSON.to_string()));
            self.checked_body()?
        } else {
            String::new()
        };
        Ok(BuiltRequest {
            method,
            url,
            headers,
            body,
        })
    }

    /// The body, checked to be a resource of the right type (and id on Update).
    /// An Update body with no `id` is accepted: the URL id is authoritative.
    fn checked_body(&self) -> Result<String, FhirError> {
        let body = self.body.as_deref().ok_or(FhirError::MissingBody)?;
        let value: serde_json::Value =
            serde_json::from_str(body).map_err(|_| FhirError::BodyNotResource)?;
        let rtype = value
            .get("resourceType")
            .and_then(serde_json::Value::as_str)
            .ok_or(FhirError::BodyNotResource)?;
        if rtype != self.resource_type {
            return Err(FhirError::BodyTypeMismatch);
        }
        if self.interaction == Interaction::Update
            && let Some(body_id) = value.get("id").and_then(serde_json::Value::as_str)
            && Some(body_id) != self.id.as_deref()
        {
            return Err(FhirError::BodyIdMismatch);
        }
        Ok(body.to_string())
    }
}

impl Profile {
    /// A GET for a server-supplied paging `link` (a Bundle's `next` URL).
    ///
    /// # Errors
    /// [`FhirError::ForeignLink`] when `link` is not on the profile's origin:
    /// the credential must never be sent to a host the profile does not name.
    pub fn link_request(&self, link: &str) -> Result<BuiltRequest, FhirError> {
        let link = link.trim();
        match (origin(link), origin(&self.base_url)) {
            (Some(a), Some(b)) if a == b => Ok(BuiltRequest {
                method: "GET",
                url: link.to_string(),
                headers: vec![("Accept".to_string(), FHIR_JSON.to_string())],
                body: String::new(),
            }),
            _ => Err(FhirError::ForeignLink),
        }
    }
}

/// Pretty-print JSON text (two-space indent) for display in a read-only buffer.
///
/// # Errors
/// [`FhirError::NotJson`] when `text` is not JSON.
pub fn pretty(text: &str) -> Result<String, FhirError> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|_| FhirError::NotJson)?;
    serde_json::to_string_pretty(&value).map_err(|_| FhirError::NotJson)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> Profile {
        Profile::parse(r#"{"base_url":"https://fhir.example.com/r4/"}"#).unwrap()
    }

    fn build(cmd: &str) -> Result<BuiltRequest, FhirError> {
        Spec::parse_command(cmd)?.build(&profile())
    }

    #[test]
    fn read_search_history_capabilities() {
        let r = build("read Patient/123").unwrap();
        assert_eq!(r.method, "GET");
        assert_eq!(r.url, "https://fhir.example.com/r4/Patient/123");
        assert_eq!(
            r.headers,
            vec![("Accept".to_string(), "application/fhir+json".to_string())]
        );
        assert_eq!(r.body, "");

        let s = build("search Patient name=John Smith&_count=5&birthdate=ge1990-01-01").unwrap();
        assert_eq!(
            s.url,
            "https://fhir.example.com/r4/Patient?name=John%20Smith&_count=5&birthdate=ge1990-01-01"
        );
        let s = build("SEARCH Observation code=http://loinc.org|8867-4&status:not=final,amended")
            .unwrap();
        assert!(
            s.url
                .ends_with("?code=http%3A%2F%2Floinc.org%7C8867-4&status:not=final,amended")
        );

        assert_eq!(
            build("history Patient/1").unwrap().url,
            "https://fhir.example.com/r4/Patient/1/_history"
        );
        assert_eq!(
            build("history Patient").unwrap().url,
            "https://fhir.example.com/r4/Patient/_history"
        );
        assert_eq!(
            build("capabilities").unwrap().url,
            "https://fhir.example.com/r4/metadata"
        );
        assert_eq!(build("metadata").unwrap().method, "GET");
        assert_eq!(build("delete Patient/9").unwrap().method, "DELETE");
    }

    #[test]
    fn create_and_update_check_the_body() {
        let body = r#"{"resourceType":"Patient","id":"7"}"#;
        let create = Spec::parse_command("create Patient")
            .unwrap()
            .with_body(body.into())
            .build(&profile())
            .unwrap();
        assert_eq!(create.method, "POST");
        assert_eq!(create.url, "https://fhir.example.com/r4/Patient");
        assert_eq!(create.body, body);
        assert!(create.headers.contains(&(
            "Content-Type".to_string(),
            "application/fhir+json".to_string()
        )));

        let update = Spec::parse_command("update Patient/7")
            .unwrap()
            .with_body(body.into())
            .build(&profile())
            .unwrap();
        assert_eq!(update.method, "PUT");

        let wrong_id = Spec::parse_command("update Patient/8")
            .unwrap()
            .with_body(body.into())
            .build(&profile());
        assert_eq!(wrong_id, Err(FhirError::BodyIdMismatch));
        let wrong_type = Spec::parse_command("create Observation")
            .unwrap()
            .with_body(body.into())
            .build(&profile());
        assert_eq!(wrong_type, Err(FhirError::BodyTypeMismatch));
        assert_eq!(
            Spec::parse_command("create Patient")
                .unwrap()
                .build(&profile()),
            Err(FhirError::MissingBody)
        );
        assert_eq!(
            Spec::parse_command("create Patient")
                .unwrap()
                .with_body("hello".into())
                .build(&profile()),
            Err(FhirError::BodyNotResource)
        );
    }

    #[test]
    fn bad_commands_are_typed_errors() {
        assert!(matches!(
            Spec::parse_command("fetch Patient/1"),
            Err(FhirError::BadInteraction(_))
        ));
        assert_eq!(Spec::parse_command("read"), Err(FhirError::MissingType));
        assert_eq!(
            Spec::parse_command("read Patient"),
            Err(FhirError::MissingId)
        );
        assert!(matches!(
            Spec::parse_command("read patient/1"),
            Err(FhirError::BadResourceType(_))
        ));
        assert!(matches!(
            Spec::parse_command("read Patient/../etc"),
            Err(FhirError::BadId(_))
        ));
        assert!(matches!(
            Spec::parse_command("read Patient/a/b"),
            Err(FhirError::BadId(_))
        ));
    }

    #[test]
    fn paging_links_must_stay_on_the_servers_origin() {
        let p = profile();
        let ok = p
            .link_request("https://fhir.example.com/r4?_getpages=abc&_getpagesoffset=10")
            .unwrap();
        assert_eq!(ok.method, "GET");
        assert_eq!(
            p.link_request("https://evil.example.org/r4?x=1"),
            Err(FhirError::ForeignLink)
        );
        assert_eq!(
            p.link_request("http://fhir.example.com/r4"),
            Err(FhirError::ForeignLink)
        );
        assert_eq!(p.link_request("/r4?x"), Err(FhirError::ForeignLink));
    }

    #[test]
    fn pretty_prints_json() {
        assert_eq!(
            pretty(r#"{"a":[1]}"#).unwrap(),
            "{\n  \"a\": [\n    1\n  ]\n}"
        );
        assert_eq!(pretty("nope"), Err(FhirError::NotJson));
    }
}
