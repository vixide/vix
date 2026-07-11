# HTTP / REST Client

Editor action `tools.http_send`.

Write a request in a `.http`-style buffer -- `METHOD url`, optional `Header: value` lines, a blank line, then the body (method defaults to GET; `#` and `//` are comments; an absolute URL is required) -- and send it. The response (status, headers, body) opens in a new tab.

From **Tools -> Send HTTP Request** or the command palette. Pure parser `crate::http_client::parse_request`; blocking `send` via `ureq` on a background thread; `App::http_send` / `poll_http`.

See `spec/index/index.md` for the project overview and `crates/vix-editor-core/spec/index.md` for the full action catalog.
