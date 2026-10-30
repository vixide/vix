# Jupyter protocol core

Pure Jupyter kernel protocol core: signed wire codec, typed messages,
connection files, kernelspecs, outputs and Org results.

This is the pure half of Vix's Jupyter kernel client (T734a): everything about the
[Jupyter messaging protocol](https://jupyter-client.readthedocs.io/en/stable/messaging.html)
(v5.3) that does not need a socket, a process or a file. The transport crate
[`vix-jupyter`](../../vix-jupyter/spec/index.md) moves the bytes; this crate
decides what they mean. The feature design and the choice of ZeroMQ are in
[`spec/jupyter`](../../../spec/jupyter/index.md).

## Modules

| Module | Contents |
| ------ | -------- |
| `wire` | `Signer` (HMAC-SHA256 sign, constant-time verify), `encode`, `decode`, `Limits`, `DecodeError`, the `<IDS\|MSG>` delimiter. |
| `message` | `Header`, `Message`, `PROTOCOL_VERSION`, the `request` builders (`execute`, `empty`, `shutdown`), and `Content`: typed `kernel_info_reply`, `execute_reply`, `execute_input`, `stream`, `execute_result`, `display_data`, `update_display_data`, `error`, `status`, `interrupt_reply`, `shutdown_reply`. Any other `msg_type` is `Content::Other`. |
| `connection` | `ConnectionInfo` (the `kernel-*.json` connection file), `Transport` (`tcp`, `ipc`), `Channel`, `endpoint`, `is_local`. |
| `kernelspec` | `KernelSpec::parse` (`kernel.json`), `argv_for` (`{connection_file}`, `{resource_dir}`), `runs` (language match, `jupyter-python` = `python`), the search path: `PathEnv`, `data_dirs`, `kernel_dirs`, `user_data_dir`, and the project-kernelspec rule (T734f): `gate_project_dirs`, `PROJECT_ENVS`. |
| `output` | `Output` (`Stream`, `Result`, `Display`, `Error`), `MimeBundle::choose`, `Choice`, `Payload`. |
| `ansi` | `strip_ansi`. |
| `cell` | `# %%` code cells (T734d): `is_marker` (the language's line-comment prefix, then `%%`) and `cell_at`, the `Cell` around a line (its marker and body range; lines above the first marker are a cell of their own; `None` when the text has no marker). |
| `org` | `results_block`, `find_results`, `results_edit`, `results_append_edit`, `apply`. |
| `babel` | Org header arguments (T734e): `Header::parse` (`:results` as `Collect` and `Handling`, `:file`, `:eval` as `Eval`), `result_text`, `first_image`. |
| `image` | Kernel images (T734g): `decode` (base64 `image/png`/`image/jpeg` with size caps), `Image` (`extension`, `file_name`), `ImageError`, `MAX_IMAGE_BYTES`, `MAX_SIDE`, `MAX_PIXELS`. |

## Wire codec

A message is `[identity…] <IDS|MSG> signature header parent_header metadata
content [buffer…]`. The signature is the lowercase hex HMAC-SHA256 of the four
JSON frames, keyed with the connection's `key`.

`decode` checks, in this order, and stops at the first failure:

1. at most `Limits::max_frames` frames (default 64) and
   `Limits::max_total_bytes` bytes in all (default 64 MiB);
2. a delimiter frame, followed by at least five frames;
3. the signature, compared in constant time (`hmac`'s `verify_slice`). Only
   lowercase hex is accepted, so a signature has one valid spelling;
4. only then the JSON parts. `serde_json`'s recursion limit bounds nesting.

So an unsigned or oversized message costs at most one HMAC and is never
parsed. `DecodeError` carries no message bytes, so it can be logged.

`Signer::new` refuses an empty key. Jupyter allows one (no authentication),
but then any local process could inject messages, so Vix does not.
`ConnectionInfo::parse` also refuses any scheme other than `hmac-sha256`.
Neither `Signer` nor `ConnectionInfo` prints the key in its `Debug` form.

An empty `parent_header` (`{}`) is `None`. Header fields other than `msg_id`
and `msg_type` default to empty, since some kernels leave `date` out.

## Requests

`request::execute` always sends `allow_stdin: false`, so `input()` raises in
the kernel instead of waiting for a stdin channel Vix does not have in v1.
`store_history` is true and `stop_on_error` is true.

## Connection file

`ConnectionInfo` is the JSON `jupyter_client` writes: `transport`, `ip`, five
ports, `key`, `signature_scheme`, `kernel_name`. `endpoint` spells
`tcp://ip:port` (`tcp://[::1]:port` for IPv6) or `ipc://ip-port`, where `ip`
is a path prefix, as `jupyter_client` does. `is_local` is true for IPC and for
TCP on a loopback address. Writing the file as `0600` in a `0700` directory is
the transport crate's job; this crate only models it. Files over 16 KiB are
refused.

## Kernelspecs

`KernelSpec::parse(name, dir, json)` reads one `kernel.json`: `argv`,
`display_name` (the name when missing), `language`, `interrupt_mode`
(`signal`, the default, or `message`), `env` and `metadata`. It refuses a
file over 64 KiB, a name other than ASCII letters, digits, `.`, `_` and `-`,
and an empty `argv`, one longer than 256 entries, or one with a NUL byte.

`argv_for(connection_file)` substitutes `{connection_file}` and
`{resource_dir}` (the kernelspec's directory). Other placeholders are left
alone. The key is in the file only, never in `argv`.

`data_dirs(env)` is the search path as a pure function of a `PathEnv`
snapshot, following `jupyter_core`: each `$JUPYTER_PATH` entry (`:`, or `;`
on Windows), then the user data directory (`$JUPYTER_DATA_DIR`; otherwise
`~/Library/Jupyter` on macOS, `$XDG_DATA_HOME/jupyter` or
`~/.local/share/jupyter` on other Unixes, `%APPDATA%\jupyter` on Windows),
then `<prefix>/share/jupyter` for each Python environment prefix, then the
system directories (`/usr/local/share/jupyter` and `/usr/share/jupyter`, or
`%PROGRAMDATA%\jupyter`). Duplicates are dropped. `kernel_dirs` appends
`kernels` to each.

`gate_project_dirs(dirs, root, trusted)` applies the project-kernelspec rule
(T734f) to a workspace at `root`: untrusted, it drops every directory inside
`root` (an activated `.venv`, or a `$JUPYTER_PATH` entry in the repository),
so a cloned repository cannot choose the program a kernel runs; trusted, it
appends `<root>/.venv/share/jupyter/kernels` and
`<root>/venv/share/jupyter/kernels` (`PROJECT_ENVS`). Paths compare by
component as given; the caller canonicalizes them.

## Outputs

`Output::from_content` turns `stream`, `execute_result`, `display_data`,
`update_display_data` and `error` into an `Output`, and anything else into
`None`. `MimeBundle::choose` picks the first of `image/png`, `image/jpeg`,
`text/markdown` and `text/plain` that is present. Image data is returned as
base64 with whitespace removed, not decoded (T734g decodes it). When the
choice falls back to `text/plain` while the bundle also has a type Vix cannot
show (such as `text/html`), `Choice::dropped_richer` is set. A value may be a
string or a list of strings (joined), as in notebooks.

`Output::plain_text` is the text form: stream text, `text/markdown` or
`text/plain`, or `ename: evalue` followed by the traceback. Error text has
ANSI codes and control characters removed.

## ANSI stripping

`strip_ansi` removes CSI sequences (`ESC [` … final byte), OSC, DCS, PM and
APC strings (up to BEL or `ESC \`), and two-byte `ESC x` sequences, plus
every other control character except newline and tab: the 8-bit CSI
`U+009B`, a bare CR, backspace. A truncated sequence at the end is dropped.
The output is a fixed point of `strip_ansi`. A kernel's text cannot move the
cursor, retitle the terminal or plant a hyperlink through Vix.

## Org results

`results_block(value)` formats Org's `#+RESULTS:` block:

- text under 10 lines (`org-babel-min-lines-for-block-output`): one `: line`
  per line, `:` for an empty line;
- 10 lines or more: a `#+begin_example` block, with Org's comma escape on
  lines that start with `*` or `#+` (after spaces and commas);
- empty text: the bare `#+RESULTS:` line;
- a file: `[[file:path]]` (`]` escaped as `%5D`).

`find_results(buffer, end_src)` finds the results of the `src` block whose
`#+end_src` is line `end_src`: only blank lines may come between. The block is
the `#+RESULTS:` (or `#+RESULTS[hash]:`) line plus what follows: a whole
`#+begin_…`/`#+end_…` block, a `[[…]]` link line, or consecutive `:` and `|`
lines. An unclosed `#+begin_` takes only its own line, never the rest of the
file.

`results_edit(buffer, end_src, block)` is a line-range replacement: it
replaces existing results, or inserts a blank line and the block after
`#+end_src`, or (with `None`) removes the results and the blank lines before
them, so inserting and then clearing restores the buffer.
`results_append_edit(buffer, end_src, block)` (`:results append`) inserts the
block's lines without its `#+RESULTS:` line after the existing results, or
behaves as `results_edit` when there are none. `apply` performs an edit on a
string; the App sets the editor's text to the result in one undo step.

## Org header arguments

`Header::parse(begin_line)` reads a `#+begin_src <language> …` line (any
case; a `"…"` value may hold spaces). It honors only:

- `:results` — `value` (the default) or `output` (`Collect`), and `replace`
  (the default), `append`, or `silent`/`none` (`Handling`). The last word of
  each kind wins; format words (`raw`, `drawer`, `table`, …) are ignored.
- `:file` — the first word: where an image result is saved.
- `:eval` — `no`/`never` (`Eval::Never`, holding the word), `query`
  (`Eval::Query`), anything else `Eval::Yes` (`no-export`, `never-export`
  and `query-export` only concern export).

Header arguments from `#+header:` lines, `#+PROPERTY: header-args` or
property drawers are not read.

`result_text(outputs, collect)` is the result text in arrival order, ANSI
removed, one output per line group: with `Value`, every stream, value,
display and error, as a notebook cell shows them (a deliberate difference
from Org's own `value`, which is the last expression only: a kernel has no
separate "return value" channel); with `Output`, only streams and errors.
An image contributes its `text/plain` form. `first_image(outputs)` is the
first output whose chosen MIME type is an image: its type and base64 data.

## Images

`decode(mime, base64)` turns kernel image data into checked bytes. A kernel
is not trusted to send sane images, so it refuses, before any pixel is
decoded:

- a type other than `image/png` or `image/jpeg` (`Unsupported`);
- base64 text whose decoded size would pass `MAX_IMAGE_BYTES` (16 MiB), and
  decoded data over it (`TooLarge`);
- data that is not base64, or not the claimed type: no PNG signature and
  `IHDR`, or no JPEG start marker and start-of-frame segment (`Invalid`);
- a width or height of 0 or over `MAX_SIDE` (16,384), or more than
  `MAX_PIXELS` (16 Mi pixels, 64 MiB as RGBA) in all (`TooManyPixels`).

The size comes from the PNG `IHDR` chunk or the JPEG `SOFn` segment, so a
small file claiming a huge image is refused without decoding it.
`Image::file_name` is the first 16 hex digits of the bytes' SHA-256 plus the
extension (`png` or `jpg`), so the same image always gets the same name.

## Testing

Unit tests in each module; `crates/vix-jupyter-core/tests/properties.rs` has property tests (codec
round trip, any bit flipped in a signed part is rejected, hostile frames
never panic, `strip_ansi` leaves no control characters and is idempotent,
Org insert-then-clear restores the buffer) and fixture tests in the shape
ipykernel 7 sends. The `jupyter_wire` fuzz target
([`fuzz/README.md`](../../../fuzz/README.md)) fuzzes the decoder, typed
contents, outputs and `strip_ansi`.
