# FHIR Client

Talk to an HL7 FHIR server from the editor: build a request from a one-line
command, read the result as pretty-printed JSON, and browse search results
one row per Bundle entry. It builds on the [HTTP client](../http-client/index.md)
and, like it, is part of the `http` capability.

Menu: **Tools → FHIR Request…** and **Tools → FHIR Bundle…**.

## 1. Server profile — `.vix/fhir.json`

Create `.vix/fhir.json` in the workspace root:

```json
{
  "base_url": "https://fhir.example.com/r4",
  "auth": { "type": "bearer", "token_env": "FHIR_TOKEN" }
}
```

`auth` is optional (default: no authentication) and is one of:

| `type`          | Fields                                                        | Behavior                                                                                     |
| --------------- | ------------------------------------------------------------- | -------------------------------------------------------------------------------------------- |
| `none`          | —                                                             | No `Authorization` header.                                                                   |
| `bearer`        | `token_env`                                                   | Sends the token in that environment variable.                                                |
| `smart_backend` | `token_url`, `client_id_env`, `client_secret_env`, `scope`    | SMART-on-FHIR backend services: fetches a token with the OAuth2 client-credentials grant (HTTP Basic client authentication) before each request. |

**Secrets never live in the file.** The `*_env` fields name environment
variables, and each must look like `FHIR_[A-Z0-9_]+`. A literal `"token"` is
rejected as an unknown field, and a name outside the `FHIR_` prefix is
refused, so a repository you clone cannot make Vix send `AWS_SECRET_ACCESS_KEY`
(or any other variable) to its server. An authenticated profile must use
`https` (loopback hosts such as `http://localhost:8080` are allowed for
development). Start Vix from a shell that has the variables set:

```sh
FHIR_TOKEN=… vix .
```

Not supported: `private_key_jwt` (signed client assertion) authentication; only
`client_secret_basic`.

## 2. Tools → FHIR Request

Opens a prompt. Type one line:

```text
<interaction> [Type[/id]] [name=value&name=value…]
```

| Line                                          | Request                                       |
| --------------------------------------------- | --------------------------------------------- |
| `capabilities` (or `metadata`)                | `GET [base]/metadata`                         |
| `read Patient/123`                            | `GET [base]/Patient/123`                      |
| `search Patient name=smith&_count=20`         | `GET [base]/Patient?name=smith&_count=20`     |
| `create Patient`                              | `POST [base]/Patient`, body = the active buffer |
| `update Patient/123`                          | `PUT [base]/Patient/123`, body = the active buffer |
| `delete Patient/123`                          | `DELETE [base]/Patient/123`                   |
| `history Patient/123` / `history Patient`     | `GET [base]/Patient/123/_history` / `…/Patient/_history` |

Every request sends `Accept: application/fhir+json`; create and update also
send `Content-Type: application/fhir+json`. Write search parameters
unencoded; Vix percent-encodes them (a comma, which means "or" in FHIR, and a
`:modifier` such as `status:not` stay as typed). The resource type and id are
validated, and a create/update body must be a JSON object whose `resourceType`
matches the line (and, for update, whose `id` matches the URL) before anything
is sent.

The request is sent on a background thread. The response opens in a new
**untitled JSON tab**:

- a resource is pretty-printed;
- a Bundle is pretty-printed and the status line reports its entry count;
- an **OperationOutcome** (any status, usually 4xx/5xx) starts with one
  readable line per issue — `error (not-found): Resource Patient/zz is not
  known` plus `at <expression>` lines — followed by the JSON;
- an empty body (for example `204` after a delete) only reports the status.

## 3. Tools → FHIR Bundle

Opens the Bundle viewer on the **active buffer** if it holds a Bundle, else on
the **last FHIR response**. One row per entry: the `Type/id` reference, the
search mode or transaction-response status, and a short label (a patient's name,
an observation's code, a title…).

| Key             | Action                                                              |
| --------------- | ------------------------------------------------------------------- |
| `↑` `↓` `PgUp` `PgDn` | Move the highlight (a click selects; click again to open)     |
| `Enter`         | Open the entry's resource, pretty-printed, in a JSON tab            |
| `n`             | Follow the Bundle's `next` link and replace the list with that page |
| `Esc`           | Close                                                               |

A `next` link is followed only if it is on the **same origin as `base_url`**
(scheme, host and port); otherwise Vix refuses, so a hostile server cannot make
the editor send your credential elsewhere.

## Privacy

FHIR data is protected health information, so by default Vix:

- **never logs or caches it to disk** — responses live in unsaved tabs and in
  one in-memory slot (the last Bundle, for the viewer); sessions store file
  paths only;
- keeps **tokens in memory for one request** and never writes them anywhere;
  the `Secret` type prints as `Secret(***)`;
- puts **no URL, body or token in a status or error message** — a network
  failure just says the server could not be reached, because a search URL can
  contain a patient's name.

Anything you save from a tab (Save As) is, of course, then on disk.

## See also

- [HTTP Client](../http-client/index.md) — the `.http` request buffers this reuses.
- [`crates/vix-fhir/spec`](../../crates/vix-fhir/spec/index.md) — the crate's specification.
