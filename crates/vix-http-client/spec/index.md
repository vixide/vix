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

## Timeouts (T848)

`ureq` sets no timeouts by default, so a server that accepted the
connection and never answered used to keep a send waiting forever. Every
request now has two bounds (`Timeouts`, `#[non_exhaustive]`, built with
`Timeouts::new` or `Timeouts::DEFAULT`):

| Timeout | Default | Covers |
| --- | --- | --- |
| `connect` | `DEFAULT_CONNECT_TIMEOUT` = 10 s | resolving the host and opening the connection, TLS included |
| `read` | `DEFAULT_READ_TIMEOUT` = 60 s | how long the server may stay silent: each transport read or write (request, response headers, every piece of the body) restarts it, so a download that keeps flowing is not cut off |

`send_response` (and `send`) use the defaults; `send_response_with(req,
timeouts)` takes explicit ones. A timeout that runs out is
`HttpError::Timeout(stage)`. Per request, `rest::timeouts(block)` reads
`# @timeout N` (read) and `# @connection-timeout N` comment lines, the
JetBrains HTTP client's spelling: `N` is a positive whole number of seconds,
or of `ms`, `s` or `m` after it (`500 ms`, `2m`); `//` comments work,
and the last line of each kind wins. A malformed value is
`rest::BadTimeout` and stops the send with `msg.http_error`. The FHIR
client (`vix-fhir`) goes through `send_response`, so it gets the defaults
too.

## Secret variables wait for a masked preview (T848)

A `.http` file from a cloned repository chooses its own URL, so without a
check it could put `{{$processEnv NAME}}`, `{{$dotenv NAME}}` or
`{{$aadToken}}` values (directly, or through a file or environment
variable) into a request to a host of its choosing. Those three are
`rest::SECRET_VARIABLES`. `rest::expand_masked(text, ctx)` expands a block
without reading any of them: each becomes `rest::MASK` and is listed once,
in first-use order, as written (`$processEnv TOKEN`), in `Masked::secrets`;
no environment variable or `.env` file is read and `az` is not run.

`tools.http_send` (`App::http_send`) expands the block masked first. When
it uses no secret variable it is sent as before. Otherwise a confirmation
(`ConfirmKind::HttpSendSecrets`) shows the masked request line (method and
URL) and the secret variables it fills, with the values hidden:
`confirm.http_secrets_untrusted` in a workspace that is not trusted (no
T132 trust decision, or a remote workspace not trusted under T714),
`confirm.http_secrets` in a trusted one. `y` expands it for real and
sends it; `n`/Esc sends nothing (`status.http_send_cancelled`). Nothing
secret is read before the answer.

Decision (the task left it open; closest to the "ask before running
something a cloned project chose" model the workspace-trust prompt already
uses): in an **untrusted** workspace every send that uses a secret variable
asks; in a **trusted** workspace the answer is remembered for the session per
`.http` file (none for an unsaved buffer) and origin (`scheme://host:port`,
lower-cased), so the first send from a newly opened file, or to another
origin, asks, and later ones do not. Nothing is persisted. **Copy as curl**
(`tools.http_copy_curl`) still expands real values, since it sends nothing.

## Errors (T610)

`rest::BodyFileError`; `load_body_file` returns `Result<(), BodyFileError>`:

| Variant | Meaning |
| --- | --- |
| `Outside { path }` | the `< path` body file is empty, or has anything but plain names and `.` (absolute, `..`, and on Windows a root or drive such as `\x` or `C:x`, which are not absolute there but leave the folder) |
| `Read { path, source }` | the file inside the folder could not be read (`io::Error`) |

The type is `#[non_exhaustive]` with a hand-written `Display`/`Error` (no `thiserror`). `Display` is the same text the earlier `String` error carried, so the translated `msg.http_error` line is unchanged.

See `spec/index/index.md` for the project overview and `crates/vix-editor-core/spec/index.md` for the full action catalog.

## Checked arithmetic

The crate follows the `rust-checked-arithmetic` spec and, outside tests, denies `arithmetic_side_effects`, `integer_division`, `modulo_arithmetic`, `as_conversions`, `indexing_slicing` and `string_slice` (RCA-ENF-3). Request-block splitting, `{{variable}}` expansion, JSON and XPath lookups and the date variables use `split_once`, `get`, `checked_*` and `div_euclid`. **Changed:** an XPath position of 0 (`[0]`, positions count from 1) selects nothing; it used to subtract below zero. An offset or instant that overflows (`{{$timestamp}}`, `{{$datetime}}`, `{{$randomInt}}`) is `BadArguments`. The date helpers return `None` rather than wrapping for a day count too large for the calendar. **Declared (HTTP-F1):** a stray `]` in an XPath step with nothing open leaves the depth at 0; a failed OS random source still degrades to a fixed value rather than aborting a send.
