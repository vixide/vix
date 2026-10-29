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

## Several requests in one file

Separate requests with a line starting with `###`. **Send HTTP Request**
sends the request the cursor is in (a file with no `###` is one request).

```http
@host = https://api.example.com
@token = {{login.response.body.$.token}}

# @name login
POST {{host}}/login
Content-Type: application/json

{"user": "joel", "id": "{{$guid}}"}

###

GET {{host}}/things
  ?limit=10
  &after={{$datetime iso8601 -1 d}}
Authorization: Bearer {{token}}
```

- **File variables:** `@name = value` anywhere in the file; reference with
  `{{name}}`. Values may use other variables.
- **System variables:** `{{$guid}}`, `{{$timestamp}}`,
  `{{$datetime iso8601}}` or `rfc1123`, `{{$randomInt 1 100}}`,
  `{{$processEnv NAME}}` and `{{$dotenv NAME}}` (from `.env` beside the
  file). `$timestamp` and `$datetime` take an offset such as `-1 d`
  (units `ms s m h d w`).
- **Chaining:** put `# @name login` above a request. Once it has been sent,
  later requests can use `{{login.response.body.$.token}}` (a JSON path),
  `{{login.response.body.*}}` or `{{login.response.headers.X-Id}}`.
  Sending a request that refers to one you haven't sent says so.
- **Query continuation:** following lines that start with `?` or `&` are
  joined to the URL. A trailing `HTTP/1.1` on the request line is ignored.
- **Body from a file:** a body that is the single line `< ./payload.json`
  is read from that file, which must be inside the `.http` file's folder.
- **Basic auth:** `Authorization: Basic user:password` is base64-encoded
  for you.
- **Copy as curl:** **Tools → Copy HTTP Request as curl** puts the request
  under the cursor on the clipboard as a `curl` command, variables
  expanded.

- **Local time:** `{{$localDatetime iso8601}}` (or `rfc1123`, with the same
  optional offset) is `$datetime` in your computer's time zone.
- **XML responses:** for an XML body use an XPath instead of a JSON path:
  `{{soap.response.body.//Token}}`, `{{r.response.body.//item[2]/@id}}`.
  Supported: `/` and `//` steps, names (namespace prefixes ignored), `*`,
  `text()`, `@attr`, and the predicates `[2]`, `[@a]` and `[@a='v']`. The
  first match's text is used.
- **Azure AD token:** `{{$aadToken}}` (optionally `{{$aadToken
  https://vault.azure.net tenantid:ID}}`) asks the Azure CLI for a token by
  running `az account get-access-token`. Install the CLI and run `az login`
  first; without it Vix shows an install hint. The token is kept in memory
  only, never logged or written to disk. The call takes about a second, during
  which the editor waits.

## Environments

Put named sets of variables in `.vix/http-env.json` in your workspace:

```json
{
  "$shared": { "version": "v2" },
  "dev":  { "host": "https://dev.example.com",  "token": "{{$processEnv DEV_TOKEN}}" },
  "prod": { "host": "https://example.com",      "token": "{{$processEnv PROD_TOKEN}}" }
}
```

Choose the active one with **Tools → HTTP Environment…** (the first row means
"no environment"). Vix remembers the choice for the workspace in
`.vix/http-env.active`. Each section maps names to strings, numbers or
booleans, referenced as `{{host}}`.

When the same name is defined in more than one place, the **active
environment wins over the `.http` file's `@name = value`, which wins over
`$shared`**.

**Secrets.** Do not write a token or password into `http-env.json`. Give the
value as `{{$processEnv NAME}}` so it is read from your shell environment when
the request is sent, and add both files to `.gitignore`:

```text
.vix/http-env.json
.vix/http-env.active
```

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
