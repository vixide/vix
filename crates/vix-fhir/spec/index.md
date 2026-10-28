# FHIR Client

HL7 FHIR client logic: server profiles, request building, and Bundle and OperationOutcome parsing. This is the pure core behind **Tools -> FHIR Request** and **Tools -> FHIR Bundle** (T684); it builds on the REST client's `vix-http-client` for the wire and does no I/O of its own except behind the optional `net` feature.

## Server profile (`Profile`)

Read from `.vix/fhir.json` in the workspace root:

```json
{
  "base_url": "https://fhir.example.com/r4",
  "auth": { "type": "bearer", "token_env": "FHIR_TOKEN" }
}
```

`auth` is one of `{"type":"none"}` (default), `{"type":"bearer","token_env":"…"}`, or `{"type":"smart_backend","token_url":"…","client_id_env":"…","client_secret_env":"…","scope":"system/*.read"}` (SMART-on-FHIR backend services, OAuth2 client-credentials with HTTP Basic client authentication; `private_key_jwt` assertions are not implemented). Unknown fields are rejected, so a literal `"token": "…"` cannot be written by mistake.

Security rules enforced at parse time:

- Secrets are *named*, never stored: every `*_env` must match `FHIR_[A-Z0-9_]+`, so a project file cannot make Vix send an unrelated variable (`AWS_SECRET_ACCESS_KEY`) to its server.
- An authenticated profile (bearer or SMART) must use `https`, except for loopback hosts (`localhost`, `127.*`, `::1`).
- URLs with `user:pass@` authorities are refused.
- A Bundle `next` link is followed only when it is on the profile's own origin (`Profile::link_request` -> `FhirError::ForeignLink`), so a hostile server cannot redirect the credential elsewhere.
- `Secret` has a redacted `Debug` and no `Display`; `FhirError` messages never contain bodies, secrets or request URLs.

## Requests (`Spec`, `BuiltRequest`)

`Spec::parse_command("<interaction> [Type[/id]] [name=value&…]")` accepts `read`, `search`, `create`, `update`, `delete`, `history`, `capabilities` (alias `metadata`). `Spec::build(&Profile)` yields method, URL and headers: `GET [base]/Type/id`, `GET [base]/Type?params`, `POST [base]/Type`, `PUT [base]/Type/id`, `DELETE [base]/Type/id`, `GET [base]/Type[/id]/_history`, `GET [base]/metadata`. Every request sends `Accept: application/fhir+json`; writes add the same `Content-Type`. Search parameters are written unencoded and percent-encoded on build (`,` and a `:modifier` are kept literal). Type and id are validated (`[A-Z][A-Za-z0-9]*`, `[A-Za-z0-9.-]{1,64}`); a Create/Update body must be a JSON object whose `resourceType` matches (and, on Update, whose `id` matches the URL).

## Responses

- `classify(text)` -> `Content::{Bundle, OperationOutcome, Resource(type)}`; `pretty(text)` pretty-prints JSON.
- `parse_bundle` -> `Bundle { bundle_type, total, entries, next, previous }`; each `Entry` has `reference()` (`Type/id`), a `summary` label (name, `code.text`, title, …), `status` (transaction responses), `search_mode` and the resource.
- `BundlePanel` is the viewer's list state (`selected`/`scroll`, `vix_list_state` navigation, `selected_entry`, `next_link`).
- `parse_operation_outcome` -> `OperationOutcome { issues }`, each `Issue` with `severity`, `code`, `detail` (diagnostics, else `details.text`, else a coding display) and `expression`s. The host formats them for display.

## Network (`net` feature)

`net::execute(profile, request, env)` fetches a token when the profile is `smart_backend` (a fresh token per request, kept only for the call), adds `Authorization: Bearer`, and sends through `vix-http-client`. An HTTP error status is an `Ok` response. `FhirError::Transport` carries no detail because request URLs can contain PHI. Tests use a loopback `TcpListener`; no real server is contacted.

## Privacy

Nothing is logged, cached to disk or written to the session. Responses open in unsaved tabs; the last response is held in memory only, for the Bundle viewer.
