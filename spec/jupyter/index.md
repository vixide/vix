# Jupyter kernel REPL

**Status:** spike done 2026-10-30 (T734). Nothing is implemented yet; this
page records the decision and the v1 design. Implementation is split into
T734a–T734h in [`tasks.md`](../../tasks.md).

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
- App: build an `App`, run `jupyter.run` against the fake kernel, then check
  the Org buffer's `#+RESULTS:` and the dock lines (state, never translated
  text).
