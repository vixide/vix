# HTTP / REST Client

A minimal HTTP client driven by a `.http`-style buffer — write a request as
plain text, send it, and read the response in a new tab.

## Buffer format

Write the request in the active buffer using the common "REST client" shape:

```http
POST https://api.example.com/things
Content-Type: application/json
Authorization: Bearer TOKEN

{"name": "widget"}
```

- The first non-blank, non-comment line is `METHOD url`; the method is
  optional and defaults to `GET` if the line is just a URL.
- The URL must be absolute (include a scheme) — otherwise the line isn't
  recognized as a request at all.
- Lines up to the next blank line are `Header: value` pairs.
- Everything after the blank line is the request body.
- Lines starting with `#` or `//` are comments.

## Sending it

With such a buffer active, choose **Tools → Send HTTP Request** or the
command palette. Vix™ sends the request on a background thread (via the
pure-Rust `ureq` client) so the editor stays responsive, and the status line
shows the URL while it's in flight.

Only `http://` and `https://` URLs are accepted; anything else is rejected
before any network activity, both as defense-in-depth and as a clear error.

## The response

When the response arrives, it opens in a new tab as plain text: an HTTP
status line, the response headers, a blank line, then the body — for
example:

```text
HTTP/1.1 200 OK
Content-Type: application/json

{"id": 42, "name": "widget"}
```

An HTTP error status (4xx/5xx) is still a real response and opens the same
way, formatted with its own status line and body. Only a transport failure
— DNS, connection, or TLS — is reported as an error message instead of a
new tab.

## Notes

Parsing is pure logic in `crate::http_client::parse_request`; sending is
blocking and runs on a background thread, polled once per event-loop
iteration by `App::poll_http`. See the specification at
`crates/vix-http-client/spec/index.md`.

---

Vix™ and Vix IDE™ are trademarks.
