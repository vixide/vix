# Jupyter kernel REPL

**Status:** v1 built, 2026-10-30 (T734, all of T734a–T734h). The protocol
core and the transport are
[`vix-jupyter-core`](../../crates/vix-jupyter-core/spec/index.md) and
[`vix-jupyter`](../../crates/vix-jupyter/spec/index.md), tested against an
in-process fake kernel and, opt-in, a real ipykernel. The App wiring, run
targets and dock output (T734c, T734d), Org babel evaluation (T734e), the
trust rules (T734f) and images (T734g) are described in
[App wiring](#app-wiring-t734c-t734d) and the sections after it, which say
where the build differs from the design above them. The size measurement
and the recommendation on `full` are in [Size](#size-t734h). The user guide
is [`docs/jupyter`](../../docs/jupyter/index.md). This page records the
decision and the v1 design.

## Goal

Send code to a running [Jupyter kernel](https://jupyter-client.readthedocs.io/en/stable/messaging.html)
and show what comes back, without leaving Vix. The code can be a selection,
the current line, a `# %%` cell, or an Org `#+begin_src` block. Zed has a
kernel REPL that does this; Emacs has `emacs-jupyter` and `ob-jupyter`. Any
kernel works: IPython, IRkernel, `evcxr`, Deno, and others. Vix talks the
kernel protocol and never assumes Python.

## What Vix has today

- **Org source blocks.** `vix_org::src_block_at(text, line)` finds the
  `#+begin_src` block around a line and returns its language
  (`crates/vix-org/src/text_refs.rs`). `org.edit_src` (`C-c '`) already uses it.
  Nothing evaluates a block, and nothing writes a `#+RESULTS:` block. The
  [org-babel](../../crates/vix-org/spec/babel/index.md) page states that
  Vix does not execute code; this feature changes that, but only when the
  user asks for it.
- **The Run menu** is the debugger (DAP). Its `run.repl` sends one expression
  to the paused debuggee. It is not a kernel REPL.
- **The command runner** (`App::run_command_in`) streams a process's output
  into the bottom dock, one command at a time.
- **The bottom dock** ([`vix-bottom-dock`](../../crates/vix-bottom-dock/spec/index.md))
  is a capped, follow-mode line buffer whose `path:line` lines are clickable.
- **Images** open in a view-only image tab through `ratatui-image` (the `image`
  capability, `crates/vix-editor/spec/index.md`).
- **Tokio** is in the workspace already (`vix-db`, through `sqlx`). `vix-db`
  runs a `current_thread` runtime on its own worker thread and talks to the
  App over channels. A kernel client can use the same pattern.

## Options compared

All numbers come from the spike on 2026-10-30 (macOS, Apple Silicon, release
builds). The kernel was `ipykernel` 7.4.0 with `jupyter_client` 8.10.0 and
`jupyter_console` 6.6.3, installed into a throwaway `uv` virtual environment
outside the repository. The machine had no Jupyter installed.

### A. `jupyter console --simple-prompt` as a subprocess — rejected

Vix would pipe code into `jupyter console` and scrape stdout. The spike ran it
with piped stdin:

- **Each input line is a separate cell.** A two-line `for` loop failed with
  `SyntaxError: incomplete input`, and its body then ran on its own and failed
  with a `NameError`. A Python-only `exec("…")` wrapper works around this,
  but no wrapper works for every kernel language.
- **Prompt scraping.** Output arrives between `In [n]:` / `Out[n]:` prompts.
  Tracebacks are full of ANSI color codes. Stderr was printed one cell later
  than the code that wrote it.
- **No rich output.** `display(Image(png))` printed
  `<IPython.core.display.Image object>`. The PNG bytes never reach the pipe.
- **More to install.** `jupyter_console` is a separate package. `ipykernel`
  alone does not pull it in.
- Startup to first result was about 1.2 s.

### B. A Python `jupyter_client` bridge over JSON lines — viable fallback

Vix would embed a small Python program, run it as `python -I -c …`, and
exchange one JSON object per line on stdio. The program would use
`jupyter_client.manager.start_new_kernel` and `execute_interactive`. The spike
built one in about 30 lines.

- **Full fidelity.** Multi-line code, streams, errors and MIME bundles all
  arrived as JSON.
- **Startup.** The kernel was ready after 0.42 s. Each execute took 4–6 ms,
  and 30 ms for a raised exception.
- **No new Rust dependencies.** But Vix would ship and maintain a Python
  program. It needs a Python that has `jupyter_client` (it comes with
  `ipykernel`, but an R or Rust kernel user may have it in a different
  environment). It adds a second process, and a failure has two layers to
  debug.

### C. Pure-Rust ZeroMQ (`zeromq`, zmq.rs) with a small wire codec — **chosen**

Vix would launch the kernel from its kernelspec `argv` with a connection file
it writes itself, then connect DEALER (shell) and SUB (IOPub) sockets. The
spike's client is a standalone crate of about 150 lines, outside the
workspace. It uses `zeromq` 0.7, `tokio`, `hmac`, `sha2`, `serde_json`,
`uuid` and `bytes`. It signs every message with HMAC-SHA256 and checks the
signature on every message it receives.

- **Round trip works.** It covered `kernel_info`, assignment, `print`, an
  expression result (`execute_result` `text/plain`), a multi-line `for` loop,
  stderr, `ZeroDivisionError` (an `error` message with `ename`, `evalue` and
  `traceback`), and `display(Image(png))` plus `display(HTML(…))`. Each
  `display_data` arrived with every MIME type: `image/png` as base64,
  `text/html`, and `text/plain`.
- **Fast.** In six runs, the kernel answered `kernel_info` after 161–191 ms.
  Each execute took 2–4 ms, and 25 ms for a raised exception. The whole
  session took 167–233 ms.
- **IPC transport works.** The kernel listened on Unix-domain sockets
  (`"transport": "ipc"`), so access depends on file permissions and nothing
  listens on TCP. Timings were the same as over TCP. ipykernel 7 warns on
  every TCP start: "Kernel is running over TCP without encryption".
- **Builds cleanly.** A cold release build of the prototype took 9 s. The
  `zeromq` 0.7.0 source contains **no `unsafe`**. It is MIT-licensed, under
  the official `zeromq` GitHub organization, and was released 2026-10-27
  (3.3 M downloads).
- **Few new crates.** Compared with Vix's `Cargo.lock`, it adds 8 crates:
  `zeromq`, `async-trait`, `asynchronous-codec`, `futures`, `futures-macro`,
  `scc`, `sdd` and `saa`. Their licenses are MIT, Apache-2.0 or both, all in
  the `deny.toml` allow list. `cargo deny check licenses advisories sources`
  with Vix's `deny.toml` passed for every dependency (the only failure was
  the unlicensed prototype crate itself). `tokio`, `hmac`, `sha2`, `uuid`,
  `bytes`, `regex` and `socket2` are already in the lock file.
- **Binary size.** A stripped release binary was 2.00 MB, against 0.64 MB for
  the same dependencies without `zeromq`, so **about +1.36 MB**. With IPC
  support it was 2.05 MB. That is an upper bound for Vix, which already
  links `regex` and most of `tokio`.

### D. `runtimelib` / `jupyter-protocol` (runtimed, used by Zed) — not now

These are the most complete Rust Jupyter client crates. They are
BSD-3-Clause and built on the same `zeromq` crate. The API changes often:
`runtimelib` has 53 releases and shipped majors 4.0 and 5.0 on 2026-10-27
and 2026-10-28; its ZeroMQ client crate, `jupyter-zmq-client`, has had three
majors since 2026-05. They also pull in `chrono`, `dirs`, `ring` or
`aws-lc-rs`, and an `async-dispatcher`/`smol` option. Vix needs about eight
message types, so its own codec is smaller than tracking these releases.
Revisit if `runtimelib` settles and Vix later wants comms, widgets or a
Jupyter-server (websocket) client.

### E. `zmq` (libzmq bindings) — rejected

`zmq` 0.10 was last released 2022-11. It builds the C++ libzmq through
`zmq-sys`/`zeromq-src`, or links a system copy. That means a C++ toolchain for
every release target (musl, Windows on ARM, and others) and FFI `unsafe` on
the hot path. Option C needs neither.

## Decision

Use **option C**: the pure-Rust `zeromq` crate with Vix's own small codec for
the Jupyter wire protocol (v5.3), behind a capability feature. ZeroMQ is
acceptable: no C, no `unsafe`, licenses in the allow list, and Vix already
has tokio. A `jupyter console` subprocess is not enough (option A loses
multi-line code and every rich output). Option B is the documented fallback
if the dependency ever has to go.

## Crate layout

| Crate | Kind | Contents |
| ----- | ---- | -------- |
| `vix-jupyter-core` | pure, no I/O | Wire codec: frames, the `<IDS\|MSG>` delimiter, HMAC-SHA256 sign/verify, and header/parent/metadata/content. Typed messages for the subset Vix uses: `kernel_info`, `execute_request`/`reply`, `stream`, `execute_result`, `display_data`, `update_display_data`, `error`, `status`, `interrupt_request`, `shutdown_request`. Connection-file model. Kernelspec parsing (`kernel.json`) and the search-path list as a pure function of the environment. The `Output` model with MIME selection. ANSI stripping for tracebacks. The Org `#+RESULTS:` formatter (insert or replace). Fixture-tested with messages captured from the spike. |
| `vix-jupyter` | I/O, `process` feature | Launches a kernel from its spec `argv` (substituting `{connection_file}`). A worker thread runs a `current_thread` tokio runtime with the shell, IOPub and control sockets, like `vix-db`. A synchronous `Kernel` handle sends requests and is polled for `KernelEvent`s on the App tick, the same way LSP and DAP events are. Interrupt, restart, shutdown (`shutdown_request` on control, then kill after a grace period), `kill_on_drop`. |
| App shell | new App submodules `jupyter.rs` and a `jupyter_off.rs` stub | Actions, menu, kernel picker, routing outputs to the dock, the Org buffer or an image tab, the status-bar segment, and trust prompts. |

Capability feature: `jupyter = ["dep:vix-jupyter", "vix-jupyter/process"]` in
`tui/Cargo.toml`. v1 lands opt-in, outside `full`, like `cowork`.
`scripts/check` and CI lint and test it with `--features jupyter`. A later
task measures the real size cost inside Vix and moves the feature into `full`
if it is small. `vix-jupyter-core` builds in every configuration, because it
is pure.

## Menu and actions

A **Run → Jupyter** submenu, below the debugger items. Each item has one action
id and one `run_action` arm, and is also in the palette and the action catalog.

| Item | Action | Effect |
| ---- | ------ | ------ |
| Start Kernel… | `jupyter.start` | Pick a kernelspec (display name, language and argv shown) and start it for this workspace. |
| Run | `jupyter.run` | Run the code at the cursor. In order: the selection; the Org `src` block at the cursor; the `# %%` cell (or the language's comment marker followed by `%%`); the current line. Starts the language's default kernel first if none is running. |
| Run and Advance | `jupyter.run_advance` | `jupyter.run`, then move the cursor past the code it ran. |
| Interrupt Kernel | `jupyter.interrupt` | Send SIGINT (`interrupt_mode: "signal"`) or an `interrupt_request` on control (`"message"`). |
| Restart Kernel | `jupyter.restart` | Restart the kernel and clear the execution count. |
| Shut Down Kernel | `jupyter.shutdown` | Shut the kernel down. Vix also shuts down every kernel when it exits. |
| Clear Results | `jupyter.clear_results` | Remove the `#+RESULTS:` block under the Org block, or clear the inline result. |

Keys: `Ctrl+Shift+Enter` runs `jupyter.run` (Zed binds the same key to its
REPL "run"). In an Org `src` block, `C-c C-c` (`org.ctrl_c_ctrl_c`) evaluates
the block when the `jupyter` capability is built in, as Emacs does. Without
it, it does what it does today.

**Choosing a kernel.** The Org block's language comes first. Both `python`
and the `ob-jupyter` style `jupyter-python` match a spec whose `language` is
`python`. Next is the buffer's language, then the `jupyter_kernels` setting
(language → kernelspec name). With no match, Vix shows the picker. One
kernel per language per workspace, so state is shared across blocks and
files, as in a notebook.

**Finding kernelspecs.** Vix looks in `$JUPYTER_PATH`, the user data directory
(`~/Library/Jupyter`, `~/.local/share/jupyter` or `%APPDATA%\jupyter`), then
`{sys.prefix}/share/jupyter` for the Python on `PATH`
(`/usr/local/share/jupyter`, `/usr/share/jupyter`). Each spec is a
`kernels/*/kernel.json`. A missing Jupyter shows a translated install hint
(the same pattern as `GitError::NotInstalled`). Vix never runs `pip`.

## Output rendering

Each IOPub message whose parent is the request is an `Output`:

- **`stream`** (stdout or stderr) and **`text/plain`** results become text.
- **`error`** shows `ename: evalue` and the traceback with ANSI codes stripped.
- **MIME bundles** use the first type Vix can show, in this order:
  `image/png`, `image/jpeg`, `text/markdown`, `text/plain`. `text/html`,
  `image/svg+xml`, `application/json` and widget views fall back to
  `text/plain`, plus a note that a richer form was dropped.

Where output goes:

- **Org `src` block.** Vix writes a `#+RESULTS:` block right after `#+end_src`,
  replacing an existing one, in Org's own format: `: ` lines for short text,
  `#+begin_example` for long text, and a `[[file:…]]` link for an image. An
  image goes to the block's `:file` header path if it has one; otherwise to
  `<org-file-stem>-images/<hash>.png` next to the Org file. The block's
  `:results` header is respected (`output` or `value`, `silent`, `replace`
  or `append`), and so is `:eval no`/`never`.
- **Any other buffer.** The text goes to the bottom dock under a
  `[n] <kernel>` header. The last result's first line also shows as an
  end-of-line hint after the code that ran, reusing the inlay-hint
  rendering. Images open in an image tab when the `image` capability is
  present; without it, the dock gets the `text/plain` form.
- **Status bar.** A kernel segment shows its state (`starting`, `idle`, `busy`,
  `dead`) and the execution count.

## Security

A kernel runs arbitrary code as the user. The rules:

1. **Only an explicit action runs code.** Vix never evaluates anything on file
   open, session restore, export, or Org `#+CALL`/`:eval` in another block. Org
   `:eval no`/`never` refuses to run.
2. **Trust gating (T132, T627, T714).** In an untrusted workspace, and for a
   file outside the workspace, the first run of each Org block asks for
   confirmation. The prompt shows the language, the kernel and the first lines
   of the code, and offers yes, no and "always in this workspace". This is
   Org's `org-confirm-babel-evaluate`. In a trusted workspace a run starts at
   once. The `jupyter_confirm_evaluate` setting (`untrusted` by default, or
   `always`, or `never`) changes this. A selection or line you send yourself
   counts as explicit and is never prompted.
3. **Kernelspecs come only from Jupyter's standard directories.** A
   project-local spec (such as `.venv/share/jupyter/kernels`) is used only in
   a trusted workspace. The picker shows each spec's full argv before it
   starts.
4. **Locked-down connection.** Each kernel gets a fresh 0700 runtime directory
   and a 0600 connection file. The key is random, comes from the OS RNG, and
   is never put on argv or written to a log. Every received message's HMAC is
   checked, and messages that fail are dropped. On Unix the transport is
   IPC (Unix sockets under that 0700 directory), so no TCP port is open. On
   Windows it is TCP bound to `127.0.0.1`. The directory is deleted at
   shutdown.
5. **Remote workspaces.** v1 refuses to start a kernel while the workspace is
   remote (T710+), because the code would run locally. Remote kernels are
   later work.
6. **No stdin.** v1 sends `allow_stdin: false`, so `input()` raises in the
   kernel instead of hanging Vix.

## v1 scope

**In:** one kernel per language per workspace; start, interrupt, restart and
shutdown; run a selection, line, `# %%` cell or Org `src` block; output to
the dock, an inline hint, and Org `#+RESULTS:`; text, streams, errors, PNG and
JPEG; kernelspec discovery and a picker; trust gating; a status-bar
segment; i18n for every string; the opt-in `jupyter` feature.

**Out (later):** the stdin channel (`input()`), `update_display_data` in
place, comms and widgets, kernel completion and inspection (`complete_request`
could feed the completion popup), `.ipynb` editing, Jupyter-server or
websocket and remote kernels, the kernel debugger, SVG and HTML rendering,
and one kernel per file.

## App wiring (T734c, T734d)

Built in `tui/src/app/jupyter.rs`, with `jupyter_off.rs` stand-ins for a
build without the feature. What it does today, where it differs from or
narrows the design above:

- **Capability.** `Capability::Jupyter` owns the `jupyter.` prefix, so menus
  and the palette hide the actions and `run_action` refuses them in a build
  without the feature. `Ctrl+Shift+Enter` is a focus-gated host-side key in
  `global_shared_key` (every keymap), bound only when the capability is built
  in; otherwise the key keeps its old meaning.
- **Menu.** Run → Jupyter: Start Kernel…, Run, Run and Advance, Interrupt
  Kernel, Restart Kernel, Shut Down Kernel, Clear Results. Each is also a
  `cmd.jupyter_*` palette entry.
- **Run targets.** Selection, then the Org `src` block
  (`vix_org::src_block_at`; its body, its language, and an empty language
  means the buffer's), then the `%%` cell (`vix_jupyter_core::cell`; the body
  with trailing blank lines dropped), then the current line. Blank code runs
  nothing. Run and Advance moves the cursor to the line after the code (for a
  cell, the next marker; for a block, the line after `#+end_src`).
- **Choosing a kernel** is as designed: a kernel already serving the
  language; else `jupyter_kernels` (keys normalized like languages); else the
  first spec that `runs` the language; else the picker
  (`vix_choice_list`, one row per spec: display name, language, full argv),
  and the run waits for the choice. A kernel started for a language also
  serves the language that asked for it (so a `.txt` line sent to the picked
  Python kernel finds it next time). Starting a spec that is already running
  reuses it. The kernel's working directory is the workspace root.
- **Dock output.** `[*] <display name>` and `  » <first code line>` when
  sent; each output's lines as they arrive (streams and values as plain text
  with ANSI removed; an error as `ename: evalue` plus the traceback; an image
  as a translated line saying what became of it (opened in a tab, saved for
  an Org link, refused, or cannot be shown) plus its `text/plain` form; a
  translated note when a richer form was dropped); then
  `[n] <display name>: done`, `failed` or `not run` on the reply. A kernel's own output with no request of ours (such as the note an
  interrupt prints) goes to the dock too.
- **End-of-line hint.** The last output's first non-blank line, cut to 60
  characters, after the code's last line, through the editor's inlay-hint
  rendering. It replaces the buffer's inlay hints until the language server
  next sends its own. An Org block that writes `#+RESULTS:` gets no hint
  (`:results silent` does). **Clear Results** clears it, and in an Org block
  also removes the block's `#+RESULTS:`.
- **Controls** act on the kernel for the language at the cursor, else the
  one that ran code last, else the first.
- **Status bar.** `<spec name> <state> [n]` for that same kernel, after the
  Cowork segment.
- **Polling.** `App::poll_jupyter` drains every kernel's events on the tick;
  the loop polls fast while a kernel is starting, busy or has a request in
  flight. `Ready` marks a starting kernel idle. A kernel's `Dead` event
  removes it: `Shutdown` is a status line, anything else an error message
  (with the stderr tail in the dock).
- **Remote.** Start, Run, Run and Advance and Restart are refused with a
  translated warning while `vix_authority::current()` is not local.
- **Trust.** See [Trust and safety](#trust-and-safety-t734f).
- **Test hooks.** `App::with_jupyter_kernel_dirs` (discover only in given
  directories) and `App::with_jupyter_connection` (attach a spec name to an
  already-running local kernel instead of launching its argv) start nothing
  by themselves; `jupyter_kernels`, `jupyter_picker_rows` and
  `jupyter_confirm_open` report state.

## Org babel (T734e)

Built in `tui/src/app/jupyter.rs` and `tui/src/app/jupyter/results.rs`, with
the pure parts in `vix-jupyter-core`'s `babel` and `org` modules.

- **Running a block.** **Run** with the cursor in a `src` block (fences
  included), or `C-c C-c` (`org.ctrl_c_ctrl_c`) there. `C-c C-c` always runs
  the whole block, even with a selection, and in a block it comes before the
  table and checkbox meanings; outside a block, and in a build without the
  capability, it does what it did before. Its remote refusal and trust
  questions are the same as **Run**'s.
- **Header arguments** come from the `#+begin_src` line only (not
  `#+header:` lines or `header-args` properties): `:results` `value` (the
  default) or `output`, `replace` (the default), `append` or `silent`;
  `:file`; `:eval`. `:eval no`/`never` refuses with a status line and starts
  nothing; `:eval query` asks before every run, whatever the setting says.
- **What the result is.** `value` is everything the run shows, in order:
  printed text, values, displays and errors, as a notebook cell shows them.
  This differs from Org's own `value` (the last expression only): a kernel
  has no separate return-value channel, and it is what `ob-jupyter` users
  expect. `output` is printed text and errors only. When the run produced an
  image, the result is a `[[file:…]]` link to it instead (the first image;
  see [Images](#images-t734g)); if it cannot be saved, the text forms are
  written instead.
- **Writing.** On the reply (not on `aborted`), Vix finds the block again:
  at the line it started on if its code is unchanged, else the first block
  with the same code; if neither, it writes nothing and says so. It then
  replaces, appends to (`results_append_edit`) or inserts the `#+RESULTS:`
  block in Org's format, setting the buffer's text in **one undo step** and
  keeping the cursor on the same text. Outputs are collected only for a run
  that writes, and only up to 4 MiB of text and data (later ones still reach
  the dock); a `silent` run gets the dock and the end-of-line hint, like
  any other code. The dock gets every output either way.
- **Clear Results** in a block removes its `#+RESULTS:` (and the blank lines
  before it), also one undo step.

## Trust and safety (T734f)

- **Who asks.** Only an Org block run asks; a selection, cell or line is
  something the user sends on purpose and never asks. The
  `jupyter_confirm_evaluate` setting decides: `untrusted` (the default) asks
  before a block's first run in a session unless the workspace is trusted,
  and always for a file outside the workspace root; `always` asks before
  every run; `never` never asks. `:eval query` asks regardless, and
  `:eval never` refuses regardless.
- **Trusted** means the persisted T132 script-trust decision, or an earlier
  "always in this workspace" answer (and, remote, T714 — though a remote
  workspace never starts a kernel).
- **The question** (`ConfirmKind::JupyterRunBlock`) quotes the language and
  the block's first three lines. `y` runs (and remembers the block for the
  session), `n`/Esc cancels. In an untrusted workspace, for a file inside it,
  it also offers `a`, "always in this workspace", persisted as
  `WorkspaceSession::jupyter_trusted` in the session file the same way the
  T132 decision is (`Session::set_jupyter_trusted`). `a` is not offered for
  a file outside the workspace, for `:eval query`, or under `always`.
- **Project kernelspecs.** A kernelspec directory inside the workspace (an
  activated `.venv`, a `$JUPYTER_PATH` entry in the repository, or a
  directory passed to the test hook) is used only when the workspace is
  trusted for Jupyter; then `<root>/.venv` and `<root>/venv`'s
  `share/jupyter/kernels` are searched too (`gate_project_dirs`). Paths are
  canonicalized before the comparison.
- **Remote workspaces** start no kernel (Start, Run, Run and Advance, Restart
  and `C-c C-c` in a block are refused), and **no stdin** is sent
  (`allow_stdin: false`), both from T734c and kept.

## Images (T734g)

- **Decoding.** `image/png` and `image/jpeg` are decoded from base64 by
  `vix_jupyter_core::image::decode`, which refuses data over 16 MiB, data
  that is not the claimed type, and images over 16,384 pixels on a side or
  16 Mi pixels in all, reading the size from the header before any pixel is
  decoded. A refused image gets a translated dock line (`too large` or `not
  valid image data`) and its `text/plain` form; nothing is opened or
  written.
- **Any buffer but a writing Org block.** The image is written to a private
  `0700` scratch directory (`vix_jupyter::PrivateDir`, removed when Vix
  exits) as `<sha256-prefix>.png|jpg` and opened through the same path as
  any image file (`App::open_image`: `ratatui-image`, a view-only image tab
  that takes focus). Without the `image` capability, or in a terminal
  without graphics, the dock says the image cannot be shown here, and has
  its `text/plain` form. An image from a kernel output with no request of
  ours is never opened.
- **An Org block that writes results.** The first image is saved to the
  block's `:file` path, or `<org-file-stem>-images/<sha256-prefix>.<ext>`,
  relative to the Org file's folder (the workspace root for an unsaved
  buffer), creating folders as needed, and the result is a `[[file:…]]`
  link to it. A `:file` that is absolute or has a `..` component is refused
  with a message, and the text forms are written instead. Saving needs no
  `image` capability.

## Size (T734h)

Measured 2026-10-30 on macOS (Apple Silicon, `aarch64-apple-darwin`), rustc
1.99.1, with the workspace's `[profile.release]` (`lto = "thin"`,
`opt-level = "z"`, `strip = true`; `[profile.dist]` inherits it), building
`cargo build --release -p vix-tui --bin vix` with and without
`--features jupyter`, everything else default (`full`):

| Build | Stripped `vix` |
| ----- | -------------- |
| default (`full`) | 26,836,752 bytes (25.59 MiB) |
| default + `jupyter` | 27,763,584 bytes (26.48 MiB) |
| **Delta** | **+926,832 bytes (+0.88 MiB, +3.5%)** |

This is the whole feature as built (transport, ZeroMQ, Org babel, images,
trust), below the spike's +1.36 MB upper bound because Vix already links
tokio, `regex`, `sha2`, `hmac` and `base64`. It adds 8 crates to the
lock file (see option C).

**Recommendation (for the user to decide; `full` is unchanged):** add
`jupyter` to `full`. The cost is under 1 MiB (3.5%). Nothing it adds runs
without an explicit action: no kernel starts on open, restore or save, Org
blocks are trust-gated, project kernelspecs need a trusted workspace,
remote workspaces are refused, and the client opens no listening TCP port
(IPC on Unix; loopback TCP it connects to on Windows). That is the
difference from `cowork`, which listens and stays out. The argument against
is supply chain: `zeromq` 0.7.0 is days old and brings 8 crates into the
default build; `cargo deny` passes for all of them. Keep it out of the
`minimal` profile either way. If it joins `full`, `scripts/check` and CI
can drop their separate `--features jupyter` runs.

## Testing

- `vix-jupyter-core`: unit and property tests for the codec (round trip,
  rejecting a bad HMAC, missing delimiter, identity frames), plus fixture
  tests built from IOPub messages captured from the spike.
- `vix-jupyter`: an in-process **fake kernel** made from the same `zeromq`
  crate (ROUTER plus PUB sockets) that answers `kernel_info` and
  `execute_request` with scripted IOPub messages. CI never needs Python. This
  is the same approach as the fake LSP and DAP servers (T608).
- An optional smoke test that runs only when `VIX_JUPYTER_PYTHON` points at a
  Python with `ipykernel`, like `ai_provider_smoke`.
- App (`tui/tests/jupyter.rs`, `--features jupyter`): an in-process fake
  kernel on loopback TCP, attached through an explicit kernelspec in a
  temporary directory (no `PATH` or environment edits). It checks that
  nothing starts without an action, the run targets in order and Run and
  Advance, the dock lines (untranslated kernel output, never translated
  text) and the hint, errors without ANSI codes, the image placeholder, the
  Org-block confirmation, interrupt, the picker (Start Kernel… and a
  language with no spec), a missing kernel program, no kernelspec at all,
  the remote refusal, and `Ctrl+Shift+Enter`. `real_ipykernel_runs_code`
  runs a multi-line selection against a real ipykernel when
  `VIX_JUPYTER_PYTHON` is set (passed against ipykernel 7.4.0 on
  2026-10-30). T734e–T734g add (with a `"never"` setting where trust is not
  the point): Org results inserted, replaced (no extra undo step), undone in
  one step, appended (`:results output append`, through `C-c C-c`) and
  `silent`; `:eval never` refuses and `:eval query` asks every time; Clear
  Results removes `#+RESULTS:`; "always in this workspace" is persisted and
  honored by a new App, a file outside the workspace still asks without
  offering it, and `always` asks every run; project kernelspecs appear only
  once the workspace is trusted; an image opens in an image tab (a
  halfblocks picker) and a header-claimed 100000×10 PNG is refused; an Org
  image is saved to `:file` or `<stem>-images/` for its link, and an
  escaping `:file` or an oversized image is not written.
  `real_ipykernel_writes_org_results` runs an Org `:results output` block
  and a `:file` image block against a real ipykernel (passed against
  ipykernel 7.4.0 on 2026-10-30).
