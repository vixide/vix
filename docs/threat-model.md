# Threat Model

This page is the threat model behind [`SECURITY.md`](../SECURITY.md), worked
out per trust boundary and tied to the code that enforces it. `SECURITY.md`
says what a vulnerability report is judged against; this page says, for each
place where content Vix did not write meets something that can act as you,
what is at stake, how it gets in, what already stops it (with the file that
does it), and what is still open. It is a contributor's document: read it
before adding a feature that crosses one of these lines, and update it in the
same change that adds or moves a boundary
([`spec/security`](../spec/security/index.md)).

## The starting point

Vix is a local editor running as you. It can already read and write every
file you can, start any program you can, and reach any host you can. Its job
is narrower than a sandbox's: **content you did not write must not act as you
without your say-so.** "Content you did not write" is a cloned repository, a
file you open, a network peer's messages and a child process's output. "Your
say-so" is an explicit action: a menu item, a key, a `y` at a prompt.

The adversaries this model considers:

- **A hostile repository** you clone and open: its files, its `.vix/`
  directory, its `node_modules/`, its build scripts.
- **A hostile file** you open from anywhere (a download, an attachment).
- **A hostile network peer**: an AI provider or a man in the middle, an IRC
  or Matrix server or user, a Cowork guest, an HTTP server, a database
  server, a remote host.
- **A hostile or confused child process**: a language server, debug adapter,
  ACP agent, Jupyter kernel or MCP client.
- **A leak by accident**: a secret written to a log, a crash report, a
  persisted file with loose permissions, or a commit.

Out of scope, as in `SECURITY.md`: an attacker who already controls your
account, your `~/.config/vix/` or the machine; what an external program does
once you have chosen to run it; and a workspace you chose to trust running
code.

## Trust boundaries

### Workspace trust (untrusted workspaces)

- **Assets.** Your account: anything a project can make Vix run runs as you.
- **Entry points.** `<project>/.vix/scripts/*.rhai`, `.vix/settings.toml`
  (can set commands), `.vix/env.toml` and direnv/mise detection,
  `node_modules/.bin/prettier`, project Jupyter kernelspecs, ACP agents, the
  well-known language servers found on `PATH` (they run the project's build
  scripts and macros).
- **Mitigations.** One persisted decision per workspace root
  (`tui/src/app/trust.rs`, stored as `scripts_trusted` in
  `crates/vix-session/src/lib.rs`; `Session::set_workspace` carries it across
  a session save). The prompt needs an explicit `y` and ignores keys for
  `SCRIPT_TRUST_KEY_GUARD` (250 ms, `tui/src/app.rs`), so a typed-ahead Enter
  cannot answer it. Project settings are read only when trusted
  (`tui/src/app/project_settings.rs`), as are project scripts
  (`tui/src/app/scripts.rs`) and the project's Prettier
  (`tui/src/app/prettier.rs`). Well-known language servers ask first, naming
  the server (`maybe_prompt_lsp_trust` in `tui/src/app/trust.rs`,
  `crates/vix-lsp/src/defaults.rs`). A trusted plugin whose SHA-256 changes is
  not loaded (`crates/vix-script/src/plugins.rs`). Tasks run only when chosen,
  through a prompt that shows the resolved command.
- **Open risks.** Trust is all-or-nothing per root: trusting a workspace for
  its scripts also trusts its settings and language servers. A trusted
  workspace that later pulls a hostile commit stays trusted (only plugins
  are re-checked by hash; `.vix/settings.toml` and scripts are not).

### The authority slot and the remote agent over SSH

- **Assets.** Files and commands on the remote host; the local machine, if
  the remote side could make the local Vix act.
- **Entry points.** `vix-authority`'s SSH, container, Kubernetes and WSL
  authorities; the remote agent (`vix --agent`) and its frames; the agent
  upload.
- **Mitigations.** Authentication and host keys are `ssh`'s own (keys,
  agent, `known_hosts`); Vix never puts a password on a command line and
  runs background commands with `BatchMode=yes`
  (`crates/vix-authority/src/ssh/authority.rs`). The agent must report
  exactly this build's protocol and Vix version
  (`crates/vix-authority/src/agent/mod.rs`,
  `crates/vix-authority/src/agent/server.rs`); every frame
  is capped at `MAX_FRAME` (64 MiB, `crates/vix-authority/src/agent/wire.rs`).
  An uploaded agent binary is checked by SHA-256 on the remote side, and the
  upload is refused when the host has no checksum tool
  (`crates/vix-authority/src/ssh/agent_boot.rs`). A remote workspace starts
  untrusted, remembered per host and path; its `.vix/settings.toml` is never
  read, trusted or not (`tui/src/app/project_settings.rs`). Child processes
  for the remote pickers and search are argv-only, no shell
  (`tui/src/app/remote_pick.rs`, `tui/src/app/remote_search.rs`).
- **Open risks.** A compromised remote host controls everything Vix shows
  from it (file contents, listings, search results) and can feed hostile
  content into the local editor; that content is then "a file you open" and
  must stay inert. The SHA-256 check guards against a corrupted upload, not
  against a remote root who replaces the agent after it starts.

### Language servers and DAP adapters

- **Assets.** Your account (the server runs as you); the buffer text sent to
  it; the files a server's edits touch.
- **Entry points.** `lsp_servers` in your settings or a trusted workspace's,
  the well-known servers, debug adapters, and every message they send back
  (`workspace/applyEdit`, document links, file operations, diagnostics).
- **Mitigations.** Language servers start only from your settings, a trusted
  workspace's, or after the per-server trust prompt (above). A crashing server
  is restarted at most `MAX_RESTART_ATTEMPTS` (3) times
  (`crates/vix-lsp/src/lib.rs`). Directory scans for file operations stop at
  `SCAN_MAX_ENTRIES` and `SCAN_MAX_DEPTH`
  (`crates/vix-lsp-core/src/file_ops.rs`). A document link to a URL opens
  with `open`/`xdg-open` by argv, no shell (`tui/src/app/doc_links.rs`).
  Debug adapters start only on **Run → Start Debugging** or **Run → Attach**
  (`crates/vix-dap/src/lib.rs`, `tui/src/app/lsp_dap.rs`). Every spawn site is
  listed in `tui/tests/spawn_sites.rs`.
- **Open risks.** The `Content-Length` framing has no upper bound
  (`crates/vix-lsp-core/src/frame.rs`): a server that announces a huge body
  makes Vix buffer it. The server already runs as you, so this is a
  robustness issue rather than an escalation.

### Rhai scripts

- **Assets.** The buffer and the editor state; your account, if a script
  could escape.
- **Entry points.** Global scripts in `~/.config/vix/` (trusted), project
  scripts and plugins (behind workspace trust).
- **Mitigations.** The engine has no file, network or process access: the
  module resolver is a `DummyModuleResolver`, so `import` fails (the T134
  fix), and `set_max_operations` and `set_max_expr_depths` bound runaway
  scripts (`crates/vix-script/src/engine.rs`;
  `crates/vix-script/tests/sandbox.rs`).
- **Open risks.** A trusted script can still edit the active buffer however
  it likes and bind keys, so it can, for example, rewrite a file you then
  save. That is the feature; the prompt shows which script is trusted.

### MCP server mode (`vix --mcp`)

- **Assets.** Workspace files, especially secrets in them; databases shared
  through MCP.
- **Entry points.** JSON-RPC from the MCP client over stdio (no listening
  socket, `tui/src/mcp.rs`).
- **Mitigations.** Read-only tools; every path is resolved lexically, then
  canonicalized and checked inside the root, so a symlink cannot leave it;
  secret-looking files (`.env`, keys, `.netrc`, `.git/`, `.ssh/`) are
  refused by `is_secret`; reads stop at `MAX_FILE_BYTES` (2 MiB)
  (`crates/vix-mcp-server/src/workspace.rs`). A database is reachable only
  when you mark its saved connection shared, and then only through one
  read-only statement in a read-only transaction with a 30-second timeout and
  a row cap, with credentials found without prompting
  (`crates/vix-db/src/mcp_share.rs`).
- **Open risks.** `is_secret` is a name list: a secret kept in a file with an
  ordinary name (a production settings file, say) is readable by any MCP client you connect.

### ACP agents (the Agent panel)

- **Assets.** Workspace files, secrets, your account (agents can ask to run
  commands).
- **Entry points.** `fs/read_text_file`, `fs/write_text_file`, permission
  and terminal requests from the agent process.
- **Mitigations.** Paths must be absolute, inside the root, with no `..`, and
  are checked again with symlinks resolved (`crates/vix-acp/src/policy.rs`).
  Reads refuse the same secret files as MCP (`acp_path_is_secret` in
  `tui/src/app/acp.rs`). Every permission defaults to deny, the helpers only
  ever pick a one-shot allow, and every write becomes a diff you accept or
  reject. ACP agents start only in a trusted workspace.
- **Open risks.** As with MCP, the secret list is by file name. An agent's
  own tools (outside ACP) are the agent's business, not Vix's.

### AI providers and their API keys

- **Assets.** API keys; the buffer text, selection or schema you send.
- **Entry points.** `ai_api_key_command`, the OS keyring, provider responses,
  `ai_command` (a shell template you configure).
- **Mitigations.** Keys come from your `ai_api_key_command`, then the OS
  keyring, never a plaintext settings value
  (`crates/vix-ai-core/src/secret.rs`, used from `tui/src/app/ai.rs`).
  Default endpoints are HTTPS (`crates/vix-ai-core/src/lib.rs`); the local
  model endpoint defaults to plain HTTP on `localhost`. Vix quotes the prompt
  it substitutes into `ai_command` and drops quotes written right around
  `{prompt}` or `{file}`, so chat text cannot break out into the shell.
  Nothing is sent until you invoke an AI action, and the AI text transforms
  (Annotate, Improve) arrive as a hunk-by-hunk review
  (`crates/vix-ai-diff/src/lib.rs`), not a silent overwrite.
  An imported settings bundle never imports `ai_command` or other
  command-bearing fields (`crates/vix-settings-bundle/src/lib.rs`).
- **Open risks.** Whatever you send goes to the provider you configured;
  Vix does not strip secrets from a selection you send. Responses are
  untrusted text and must stay inert (they are shown or offered as diffs,
  never run).

### Database credentials

- **Assets.** Database passwords; the data; the query history.
- **Entry points.** `password_command`, the keyring, the password prompt,
  connection strings, SSH tunnels, server error messages.
- **Mitigations.** A password comes from `password_command`, then the
  keyring, then a prompt, and is saved only to the keyring and only if you
  ask (`crates/vix-db/src/secret.rs`, `crates/vix-db/src/lifecycle.rs`).
  Errors pass through `redact_url_secret` before they reach the UI
  (`crates/vix-db/src/session.rs`). `db_history.toml` keeps query text,
  never bind values, and is owner-only on Unix. PostgreSQL and MySQL use
  rustls. The tests never touch the real keyring
  (`vix_db::secret::use_system_keyring` is opt-in).
- **Open risks.** Literal secrets typed into query text land in the history
  file. The bundled SQLite is affected by RUSTSEC-2026-0459 until `sqlx`
  accepts the fixed `libsqlite3-sys` (ignored with a dated note in
  `deny.toml`): do not open untrusted SQLite files in the workbench.

### Cowork networking

- **Assets.** The buffers you share; your account, through a guest's edits.
- **Entry points.** A TCP listener the host opens; invites; guest frames.
- **Mitigations.** Every session is encrypted and authenticated with
  `Noise_NKpsk0_25519_ChaChaPoly_BLAKE2s`
  (`crates/vix-cowork-net/src/handshake.rs`); invites are single-use and
  short-lived (`crates/vix-cowork-net/src/invite.rs`); the host approves each
  guest by name and nothing is sent to an unapproved one
  (`crates/vix-cowork-net/src/server.rs`). The suggested bind address is
  `127.0.0.1:0` (`tui/src/app/cowork.rs`).
- **Open risks.** An approved guest can edit shared buffers, which you may
  then save; binding a public address is your choice and exposes the
  handshake to the network.

### IM networking (IRC and Matrix)

- **Assets.** Account passwords; the conversation; the editor, if a message
  could act.
- **Entry points.** IRC servers and users (CTCP, mentions, formatting), the
  Matrix helper process and its server.
- **Mitigations.** IRC uses TLS (rustls, `webpki-roots`) unless the account
  says `tls = false`, and then Vix warns (`crates/vix-im-irc/src/net.rs`,
  `tui/src/app/im_net.rs`). Passwords print as `<redacted>` in `Debug`
  (`crates/vix-im-irc/src/client.rs`, `crates/vix-matrix-protocol/src/protocol.rs`).
  The Matrix password comes from `VIX_IM_PASSWORD_<NAME>` at connect time,
  or a keyring session (`tui/src/app/im_matrix.rs`). Messages are text: a
  shared `path:line` reference opens only when you choose it.
- **Open risks.** The `vix-matrix` helper is found beside the `vix`
  executable or on `PATH` (`crates/vix-matrix-protocol/src/net.rs`), so a
  hostile `PATH` entry could stand in for it; that needs control of your
  environment, which is out of scope.

### Jupyter kernels

- **Assets.** Your account (a kernel runs code); the connection key.
- **Entry points.** Kernelspecs (yours and a project's), Org source blocks,
  the kernel's ZeroMQ sockets.
- **Mitigations.** Each kernel gets a fresh `0700` runtime directory and a
  `0600` connection file with a random key, created new
  (`crates/vix-jupyter/src/runtime_dir.rs`); sockets bind only to loopback
  (`crates/vix-jupyter-core/src/connection.rs`); a `kernel.json` over
  `MAX_KERNEL_JSON_BYTES` (64 KiB) is refused
  (`crates/vix-jupyter-core/src/kernelspec.rs`). Project kernelspecs wait for
  workspace trust, and an Org block asks before it runs in an untrusted
  workspace or for a file outside it (`tui/src/app/jupyter.rs`).
- **Open risks.** Another local process running as you can read the
  connection file; that is the same account, so out of scope.

### Crash reports

- **Assets.** Your home path, user name, and anything quoted in a panic
  message.
- **Entry points.** The panic hook; **Help → Report a Crash…**.
- **Mitigations.** The report holds version, OS, panic message, location and
  backtrace, with the home directory, the user name and quoted text scrubbed,
  and never buffer text or the environment (`tui/src/crash.rs`). Nothing is
  sent: the menu item opens a prefilled issue you review and submit yourself
  (`tui/src/app/crash_report.rs`).
- **Open risks.** A backtrace frame can name a workspace path; the scrub
  removes the home prefix, not the rest of the path.

### The HTTP client

- **Assets.** Secrets referenced by a `.http` file; services on your
  network.
- **Entry points.** `.http` buffers and their environment files, the
  responses.
- **Mitigations.** Only `http` and `https` URLs are sent
  (`crates/vix-http-client/src/lib.rs`). A request is sent only when you send
  it. Azure tokens come from `az` by argv
  (`crates/vix-http-client/src/aad.rs`).
- **Open risks.** No loopback or private-address guard (an accepted risk in
  `SECURITY.md`). No explicit timeouts: `ureq`'s defaults apply. A `.http`
  file from a hostile repository can reference `{{$processEnv NAME}}`,
  `{{$dotenv NAME}}` or `{{$aadToken}}` and send the value to a host it
  chooses, the moment you send the request
  (`crates/vix-http-client/src/rest.rs`); there is no workspace-trust gate or
  preview of the resolved request for those variables.

### The repository itself

- **Assets.** Credentials that could be committed by mistake.
- **Mitigations.** `tui/tests/secret_scan.rs` fails `cargo test` when a
  tracked file holds a private key header, an AWS access key id, a GitHub,
  Slack, Anthropic or OpenAI-style token of real length; deliberate fakes need
  an allowlist entry with a reason. `cargo deny` checks advisories and
  licenses (`deny.toml`); fuzzing and mutation testing are described in
  `SECURITY.md`.
- **Open risks.** The scan sees only the working tree, not history, and only
  the listed token shapes.

## Checklist for a new feature

Answer these in the pull request (the template links here). A "yes" to any of
the first four means the change crosses a boundary above, so it updates this
page and `SECURITY.md` in the same change.

- [ ] **Does it run anything?** A new child process is argv-only (no shell
      unless the user wrote the command), listed in `tui/tests/spawn_sites.rs`,
      behind a Cargo feature, and, if the program or its arguments come from
      the project, behind workspace trust.
- [ ] **Does it read project content that can configure behavior?** Anything
      under `.vix/`, a project config file or a project-local binary waits for
      workspace trust, and is never read for a remote workspace unless the
      spec says why.
- [ ] **Does it talk to the network?** It connects only when the user acts,
      uses TLS by default (warning before plaintext), allows only the schemes
      it needs, sets a timeout, and treats every byte received as untrusted
      (bounded sizes, no execution, no path taken from it unchecked).
- [ ] **Does it hold a secret?** The secret comes from a command or the OS
      keyring, never a plaintext setting; it is redacted from `Debug`, errors
      and messages; anything persisted that may hold it is `0600`; it is never
      in a crash report.
- [ ] **Does it take a path from outside?** It is checked inside its root
      after resolving symlinks (`crates/vix-acp/src/policy.rs` and
      `crates/vix-mcp-server/src/workspace.rs` are the patterns), refuses
      secret-looking names, and does not check-then-use across a gap another
      process could exploit.
- [ ] **Does it parse input Vix did not write?** Sizes and depths are capped,
      it returns an error rather than panicking, and a new parser gets a fuzz
      target (`fuzz/fuzz_targets/`).
- [ ] **No secret in the change.** `tui/tests/secret_scan.rs` passes; a test
      fixture that needs a full-length fake is allowlisted there with a
      reason.

The longer [Security Checklist](security-checklist.md) covers the same ground
in more general terms.
