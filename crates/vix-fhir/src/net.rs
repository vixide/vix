//! The blocking network layer (feature `net`): token fetch and request
//! execution over `vix-http-client`. Call from a background thread.
//!
//! Nothing here logs, caches or persists anything: a token lives only for the
//! duration of one [`execute`] call, and error values carry no URL or body.

use vix_http_client::{Request, Response, rest, send_response};

use crate::{BuiltRequest, FhirError, Profile, Secret, parse_token_response};

/// Fetch an access token with the client-credentials grant, when the profile
/// asks for one. `None` for the other authentication schemes.
///
/// # Errors
/// [`FhirError::EnvMissing`] for an unset credential variable,
/// [`FhirError::Transport`] when the token endpoint cannot be reached, and
/// [`FhirError::BadTokenResponse`] when its reply holds no token.
pub fn fetch_token(
    profile: &Profile,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<Option<Secret>, FhirError> {
    let Some(tr) = profile.token_request(env)? else {
        return Ok(None);
    };
    let mut req = Request {
        method: "POST".to_string(),
        url: tr.url.clone(),
        headers: vec![
            (
                "Content-Type".to_string(),
                "application/x-www-form-urlencoded".to_string(),
            ),
            ("Accept".to_string(), "application/json".to_string()),
            (
                "Authorization".to_string(),
                format!("Basic {}:{}", tr.client_id, tr.client_secret.expose()),
            ),
        ],
        body: tr.form_body(),
    };
    rest::encode_basic_auth(&mut req);
    let resp = send_response(&req).map_err(|_| FhirError::Transport)?;
    if !(200..300).contains(&resp.status) {
        return Err(FhirError::BadTokenResponse);
    }
    parse_token_response(&resp.body).map(Some)
}

/// Perform `built` against the profile's server, authenticating as the profile
/// says. An HTTP error *status* is returned as `Ok` (it carries an
/// `OperationOutcome` worth showing).
///
/// # Errors
/// Any error of [`fetch_token`] / [`Profile::bearer`], and
/// [`FhirError::Transport`] when the server cannot be reached.
pub fn execute(
    profile: &Profile,
    built: &BuiltRequest,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<Response, FhirError> {
    let token = match profile.bearer(env)? {
        Some(t) => Some(t),
        None => fetch_token(profile, env)?,
    };
    let mut headers = built
        .headers
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect::<Vec<_>>();
    if let Some(t) = token {
        headers.push((
            "Authorization".to_string(),
            format!("Bearer {}", t.expose()),
        ));
    }
    let req = Request {
        method: built.method.to_string(),
        url: built.url.clone(),
        headers,
        body: built.body.clone(),
    };
    send_response(&req).map_err(|_| FhirError::Transport)
}
