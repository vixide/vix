//! The server profile (`.vix/fhir.json`) and the token request it can produce.

use serde::Deserialize;

use crate::url::{encode, is_loopback, origin};
use crate::{FhirError, Secret};

/// How to authenticate. Secrets are *named* (by environment variable), never
/// written in the file; a literal `token` / `client_secret` key is rejected as
/// an unknown field.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Auth {
    /// No `Authorization` header (open servers, local test servers).
    #[default]
    None,
    /// A pre-issued bearer token read from the environment variable `token_env`.
    Bearer {
        /// Name of the environment variable holding the token (`FHIR_…`).
        token_env: String,
    },
    /// SMART-on-FHIR backend services: fetch an access token from `token_url`
    /// with the `OAuth2` client-credentials grant, authenticating the client
    /// with HTTP Basic (`client_secret_basic`).
    SmartBackend {
        /// The authorization server's token endpoint.
        token_url: String,
        /// Environment variable holding the client id (`FHIR_…`).
        client_id_env: String,
        /// Environment variable holding the client secret (`FHIR_…`).
        client_secret_env: String,
        /// Requested scope, e.g. `system/*.read`.
        #[serde(default)]
        scope: Option<String>,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    base_url: String,
    #[serde(default)]
    auth: Auth,
}

/// A FHIR server profile: where it is and how to authenticate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    /// The server's base URL, without a trailing slash.
    pub base_url: String,
    /// The authentication scheme.
    pub auth: Auth,
}

/// Check that `name` may be read as a secret: `FHIR_` + `[A-Z0-9_]`.
fn check_env_name(name: &str) -> Result<(), FhirError> {
    let ok = name.len() > "FHIR_".len()
        && name.starts_with("FHIR_")
        && name
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_');
    if ok {
        Ok(())
    } else {
        Err(FhirError::EnvNotAllowed(name.to_string()))
    }
}

/// Read a named secret through `env`.
fn read_env(name: &str, env: &dyn Fn(&str) -> Option<String>) -> Result<Secret, FhirError> {
    check_env_name(name)?;
    match env(name) {
        Some(v) if !v.trim().is_empty() => Ok(Secret::new(v.trim().to_string())),
        _ => Err(FhirError::EnvMissing(name.to_string())),
    }
}

impl Profile {
    /// Parse and validate the text of `.vix/fhir.json`.
    ///
    /// # Errors
    /// [`FhirError::ProfileSyntax`] for malformed JSON or unknown fields,
    /// [`FhirError::BadUrl`] / [`FhirError::InsecureUrl`] for an unusable base
    /// or token URL, and [`FhirError::EnvNotAllowed`] for a secret variable
    /// outside the `FHIR_` prefix.
    pub fn parse(text: &str) -> Result<Self, FhirError> {
        let raw: Raw =
            serde_json::from_str(text).map_err(|e| FhirError::ProfileSyntax(e.to_string()))?;
        let base_url = raw.base_url.trim().trim_end_matches('/').to_string();
        if origin(&base_url).is_none() {
            return Err(FhirError::BadUrl(base_url));
        }
        match &raw.auth {
            Auth::None => {}
            Auth::Bearer { token_env } => {
                check_env_name(token_env)?;
                require_secure(&base_url)?;
            }
            Auth::SmartBackend {
                token_url,
                client_id_env,
                client_secret_env,
                ..
            } => {
                check_env_name(client_id_env)?;
                check_env_name(client_secret_env)?;
                if origin(token_url).is_none() {
                    return Err(FhirError::BadUrl(token_url.clone()));
                }
                require_secure(token_url)?;
                require_secure(&base_url)?;
            }
        }
        Ok(Self {
            base_url,
            auth: raw.auth,
        })
    }

    /// The bearer token for [`Auth::Bearer`], read from the environment.
    /// `None` for the other schemes.
    ///
    /// # Errors
    /// [`FhirError::EnvMissing`] when the variable is unset or empty.
    pub fn bearer(
        &self,
        env: &dyn Fn(&str) -> Option<String>,
    ) -> Result<Option<Secret>, FhirError> {
        match &self.auth {
            Auth::Bearer { token_env } => read_env(token_env, env).map(Some),
            _ => Ok(None),
        }
    }

    /// The token request for [`Auth::SmartBackend`]; `None` for the other
    /// schemes.
    ///
    /// # Errors
    /// [`FhirError::EnvMissing`] when a credential variable is unset or empty.
    pub fn token_request(
        &self,
        env: &dyn Fn(&str) -> Option<String>,
    ) -> Result<Option<TokenRequest>, FhirError> {
        let Auth::SmartBackend {
            token_url,
            client_id_env,
            client_secret_env,
            scope,
        } = &self.auth
        else {
            return Ok(None);
        };
        let client_id = read_env(client_id_env, env)?.expose().to_string();
        let client_secret = read_env(client_secret_env, env)?;
        Ok(Some(TokenRequest {
            url: token_url.clone(),
            client_id,
            client_secret,
            scope: scope.clone(),
        }))
    }
}

/// Authenticated traffic must be encrypted, except to the local machine.
fn require_secure(url: &str) -> Result<(), FhirError> {
    match origin(url) {
        Some((scheme, _)) if scheme == "https" || is_loopback(url) => Ok(()),
        _ => Err(FhirError::InsecureUrl),
    }
}

/// An `OAuth2` client-credentials token request, ready to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenRequest {
    /// The token endpoint.
    pub url: String,
    /// The client id (sent as the HTTP Basic user).
    pub client_id: String,
    /// The client secret (sent as the HTTP Basic password).
    pub client_secret: Secret,
    /// The requested scope, if any.
    pub scope: Option<String>,
}

impl TokenRequest {
    /// The `application/x-www-form-urlencoded` body.
    #[must_use]
    pub fn form_body(&self) -> String {
        let mut body = String::from("grant_type=client_credentials");
        if let Some(scope) = &self.scope {
            body.push_str("&scope=");
            body.push_str(&encode(scope, ""));
        }
        body
    }
}

/// Extract the `access_token` from a token endpoint's JSON reply.
///
/// # Errors
/// [`FhirError::BadTokenResponse`] when the reply is not JSON or has no
/// non-empty string `access_token`.
pub fn parse_token_response(body: &str) -> Result<Secret, FhirError> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|_| FhirError::BadTokenResponse)?;
    match value
        .get("access_token")
        .and_then(serde_json::Value::as_str)
    {
        Some(t) if !t.is_empty() => Ok(Secret::new(t.to_string())),
        _ => Err(FhirError::BadTokenResponse),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(name: &str) -> Option<String> {
        match name {
            "FHIR_TOKEN" => Some("tok".into()),
            "FHIR_ID" => Some("cid".into()),
            "FHIR_SECRET" => Some("sec".into()),
            _ => None,
        }
    }

    #[test]
    fn minimal_profile_is_open() {
        let p = Profile::parse(r#"{"base_url":"http://example.com/fhir/"}"#).unwrap();
        assert_eq!(p.base_url, "http://example.com/fhir");
        assert_eq!(p.auth, Auth::None);
        assert_eq!(p.bearer(&env), Ok(None));
    }

    #[test]
    fn bearer_reads_env() {
        let p = Profile::parse(
            r#"{"base_url":"https://e.com/fhir","auth":{"type":"bearer","token_env":"FHIR_TOKEN"}}"#,
        )
        .unwrap();
        assert_eq!(p.bearer(&env).unwrap().unwrap().expose(), "tok");
        let missing = Profile::parse(
            r#"{"base_url":"https://e.com/fhir","auth":{"type":"bearer","token_env":"FHIR_NOPE"}}"#,
        )
        .unwrap();
        assert_eq!(
            missing.bearer(&env),
            Err(FhirError::EnvMissing("FHIR_NOPE".into()))
        );
    }

    #[test]
    fn literal_secrets_and_foreign_env_names_are_rejected() {
        assert!(matches!(
            Profile::parse(
                r#"{"base_url":"https://e.com","auth":{"type":"bearer","token":"abc"}}"#
            ),
            Err(FhirError::ProfileSyntax(_))
        ));
        assert_eq!(
            Profile::parse(
                r#"{"base_url":"https://e.com","auth":{"type":"bearer","token_env":"AWS_SECRET_ACCESS_KEY"}}"#
            ),
            Err(FhirError::EnvNotAllowed("AWS_SECRET_ACCESS_KEY".into()))
        );
        assert_eq!(
            Profile::parse(
                r#"{"base_url":"https://e.com","auth":{"type":"bearer","token_env":"FHIR_a"}}"#
            ),
            Err(FhirError::EnvNotAllowed("FHIR_a".into()))
        );
    }

    #[test]
    fn authenticated_http_needs_loopback() {
        let auth = r#""auth":{"type":"bearer","token_env":"FHIR_TOKEN"}"#;
        assert_eq!(
            Profile::parse(&format!(r#"{{"base_url":"http://e.com",{auth}}}"#)),
            Err(FhirError::InsecureUrl)
        );
        assert!(Profile::parse(&format!(r#"{{"base_url":"http://127.0.0.1:9",{auth}}}"#)).is_ok());
    }

    #[test]
    fn bad_urls_are_rejected() {
        assert!(matches!(
            Profile::parse(r#"{"base_url":"ftp://e.com"}"#),
            Err(FhirError::BadUrl(_))
        ));
        assert!(matches!(
            Profile::parse(r#"{"base_url":"https://u:p@e.com"}"#),
            Err(FhirError::BadUrl(_))
        ));
        assert!(matches!(
            Profile::parse("not json"),
            Err(FhirError::ProfileSyntax(_))
        ));
    }

    #[test]
    fn smart_backend_builds_a_token_request() {
        let p = Profile::parse(
            r#"{"base_url":"https://e.com/fhir","auth":{"type":"smart_backend",
            "token_url":"https://auth.e.com/token","client_id_env":"FHIR_ID",
            "client_secret_env":"FHIR_SECRET","scope":"system/*.read"}}"#,
        )
        .unwrap();
        let t = p.token_request(&env).unwrap().unwrap();
        assert_eq!(t.url, "https://auth.e.com/token");
        assert_eq!(t.client_id, "cid");
        assert_eq!(t.client_secret.expose(), "sec");
        assert_eq!(
            t.form_body(),
            "grant_type=client_credentials&scope=system%2F%2A.read"
        );
        assert!(!format!("{t:?}").contains("sec\""));
        assert_eq!(p.bearer(&env), Ok(None));
    }

    #[test]
    fn token_response_parsing() {
        assert_eq!(
            parse_token_response(r#"{"access_token":"abc","expires_in":300}"#)
                .unwrap()
                .expose(),
            "abc"
        );
        assert_eq!(
            parse_token_response(r#"{"error":"invalid_client"}"#),
            Err(FhirError::BadTokenResponse)
        );
        assert_eq!(parse_token_response("x"), Err(FhirError::BadTokenResponse));
    }
}
