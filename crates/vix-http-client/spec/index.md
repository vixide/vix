# HTTP / REST Client

A minimal HTTP client driven by a `.http`-style buffer. Editor action
`tools.http_send`.

Write a request in a `.http`-style buffer -- `METHOD url`, optional `Header: value` lines, a blank line, then the body (method defaults to GET; `#` and `//` are comments; an absolute URL is required) -- and send it. The response (status, headers, body) opens in a new tab.

From **Tools -> Send HTTP Request** or the command palette. Pure parser `crate::http_client::parse_request`; blocking `send` via `ureq` on a background thread; `App::http_send` / `poll_http`.

### REST Client layer (`rest` module)

Several requests per buffer, separated by `###` lines; the action sends the one containing the cursor (the nearest block above when the cursor is on a separator, the first block when it precedes all of them). `@name = value` lines define file variables; `{{name}}`, `{{$guid}}`, `{{$timestamp}}`, `{{$datetime iso8601|rfc1123}}`, `{{$randomInt a b}}`, `{{$processEnv N}}`, `{{$dotenv N}}` and `{{req.response.body.$.path}}` / `.headers.X` are expanded before parsing (`expand`, depth-limited to catch cycles). A `# @name` line makes the response available to later requests; `send_response` returns the structured `Response`, and `send` is its formatted text. Also: `HTTP/x` suffix ignored, `?`/`&` query continuation lines, `< file` bodies confined to the `.http` file's folder (a crafted file must not be able to send an arbitrary local file), Basic-auth shorthand, and `to_curl` export (`tools.http_copy_curl`). Not yet: environments, `$localDatetime`, `$aadToken`.

See `spec/index/index.md` for the project overview and `crates/vix-editor-core/spec/index.md` for the full action catalog.
