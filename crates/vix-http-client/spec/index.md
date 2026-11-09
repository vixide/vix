# HTTP / REST Client

A minimal HTTP client driven by a `.http`-style buffer. Editor action
`tools.http_send`.

Write a request in a `.http`-style buffer -- `METHOD url`, optional `Header: value` lines, a blank line, then the body (method defaults to GET; `#` and `//` are comments; an absolute URL is required) -- and send it. The response (status, headers, body) opens in a new tab.

From **Tools -> Send HTTP Request** or the command palette. Pure parser `crate::http_client::parse_request`; blocking `send` via `ureq` on a background thread; `App::http_send` / `poll_http`.

### REST Client layer (`rest` module)

Several requests per buffer, separated by `###` lines; the action sends the one containing the cursor (the nearest block above when the cursor is on a separator, the first block when it precedes all of them). `@name = value` lines define file variables; `{{name}}`, `{{$guid}}`, `{{$timestamp}}`, `{{$datetime iso8601|rfc1123}}`, `{{$randomInt a b}}`, `{{$processEnv N}}`, `{{$dotenv N}}` and `{{req.response.body.$.path}}` / `.headers.X` are expanded before parsing (`expand`, depth-limited to catch cycles). A `# @name` line makes the response available to later requests; `send_response` returns the structured `Response`, and `send` is its formatted text. Also: `HTTP/x` suffix ignored, `?`/`&` query continuation lines, `< file` bodies confined to the `.http` file's folder (a crafted file must not be able to send an arbitrary local file), Basic-auth shorthand, and `to_curl` export (`tools.http_copy_curl`).

#### Environments (`env` module)

`<workspace>/.vix/http-env.json` is an object of named environments plus an optional `$shared` section; each section maps variable names to scalar values. Variables resolve **environment > the `.http` file's own `@name` > `$shared`** (`Environments::variables` orders them so the last definition wins, matching `Context::vars`). The chosen environment is remembered per workspace in `.vix/http-env.active` (plain name; `read_active` / `write_active`), set from **Tools -> HTTP Environment...** (`tools.http_environment`, a chooser whose first row clears it). A malformed environments file stops the send with a message instead of sending with the wrong values. Secrets: do not write them into the file; use `{{$processEnv NAME}}` as a value, and gitignore both `.vix/` files.

#### System variables added

- `{{$localDatetime iso8601|rfc1123 [n unit]}}`: like `$datetime` in the local zone's offset (`Context::local_offset_min`, filled by the app from the system zone).
- `{{$aadToken [resource] [tenantid:ID]}}`: runs `az account get-access-token --resource R [--tenant T] --query accessToken --output tsv` as an argv array (never a shell; arguments are limited to `[A-Za-z0-9-._:/@]` and may not start with `-`). The token stays in memory: no cache, no log, no disk, and errors carry nothing the CLI printed. A missing `az` is `VarError::AzureCliMissing`, shown as a translated install hint.
- XML chaining: `{{name.response.body.//xpath}}` (any body path starting with `/`) evaluates an XPath subset with `roxmltree` (MIT OR Apache-2.0; DTDs are refused, so no entity expansion): absolute `/` and `//` steps, name tests on the local name, `*`, `.`, `text()`, `@attr`, and the predicates `[n]`, `[@a]`, `[@a='v']`. The result is the first match's text.

Not yet: `$aadToken` blocks the UI thread for the CLI call (about a second); there is no `$aadV2Token`, token caching, or full XPath (functions, axes, unions).

## Errors (T610)

`rest::BodyFileError`; `load_body_file` returns `Result<(), BodyFileError>`:

| Variant | Meaning |
| --- | --- |
| `Outside { path }` | the `< path` body file is empty, or has anything but plain names and `.` (absolute, `..`, and on Windows a root or drive such as `\x` or `C:x`, which are not absolute there but leave the folder) |
| `Read { path, source }` | the file inside the folder could not be read (`io::Error`) |

The type is `#[non_exhaustive]` with a hand-written `Display`/`Error` (no `thiserror`). `Display` is the same text the earlier `String` error carried, so the translated `msg.http_error` line is unchanged.

See `spec/index/index.md` for the project overview and `crates/vix-editor-core/spec/index.md` for the full action catalog.
