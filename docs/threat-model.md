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
- **Open risks.** None from framing since T848: frames over
  `MAX_FRAME_BYTES` (64 MiB, about 45 times the largest measured
  rust-analyzer reply) or headers over 8 KiB are refused before the body is
  buffered, and the connection is dropped with a translated error
  (`crates/vix-lsp-core/src/frame.rs`).

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

### Native Codex and Claude Code conversations

The opt-in `ai-agents` feature starts the installed provider in a trusted local
workspace. An isolated conversation runs in a verified linked Git checkout
and keeps the original workspace as its trust boundary. Linked worktrees
share repository history and refs. Provider permissions still govern access
outside the working folder. Vix uses stdio and the provider’s login. Native tools run under the
provider’s own permission rules and can edit files on disk. Vix never loosens
those rules: it sends no sandbox, approval policy or permission mode, so the
user's Codex config and Claude Code settings decide what runs unasked. Each
approval Vix receives belongs to the current turn. The user sees the request
(sanitized, with Codex file changes and their diffs) and can allow it once or
decline it; requests arriving together are queued, never answered without a
key press. Questions Vix cannot answer are declined. Stopping a turn sends the
provider's interrupt, closes its input and, after 500 ms, kills it; on Unix the
kill takes its whole process group, so the commands it started stop too. The
provider's stderr is kept only as an 8 KiB tail for error messages. The
private conversation file contains prompts and visible replies, in an
owner-only directory. A separate lock keeps another Vix from editing it
concurrently.
[Conversation contract](../crates/vix-ai-agents/spec/index.md)

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
  own tools (outside ACP) are the agent's business, not Vix's. A path is
  checked when the agent asks and saved when you save, so an agent that can
  also write the workspace itself could swap the file for a link in between
  (see the file-system audit below).

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
  `SECURITY.md`). Closed by T848: requests default to a 10 s connect and 60 s
  inactivity timeout, overridable with `# @timeout` and
  `# @connection-timeout`; and `{{$processEnv}}`, `{{$dotenv}}` and
  `{{$aadToken}}` are never read before you approve a masked preview (method,
  URL, variable names) — an untrusted workspace asks before every such send, a
  trusted one once per file and origin per session
  (`crates/vix-http-client/src/rest.rs`, `tui/src/app/http.rs`).

### The repository itself

- **Assets.** Credentials that could be committed by mistake.
- **Mitigations.** `tui/tests/secret_scan.rs` fails `cargo test` when a
  tracked file holds a private key header, an AWS access key id, a GitHub,
  Slack, Anthropic or OpenAI-style token of real length; deliberate fakes need
  an allowlist entry with a reason. `cargo deny` checks advisories and
  licenses (`deny.toml`); fuzzing and mutation testing are described in
  `SECURITY.md`.
  `scripts/secret-scan-history` runs the same rules over every line a commit
  ever added (all refs by default, or a revision range), so a secret that was
  committed and later deleted is still found; the opt-in pre-push hook runs it
  over the commits a push adds, which also catches a commit made with
  `--no-verify`.
- **Open risks.** Only the listed token shapes are recognized, and the hooks
  are opt-in (`make hooks`): a push from a clone without them, or with
  `--no-verify`, is checked only by `tui/tests/secret_scan.rs` on the tree.
  A full-history run on 2026-11-27 found nothing (2,672 commits).

## File-system audit (T832)

Every place where Vix decides something about a path and then acts on it,
checked for `..`, symlinks, case-insensitive file systems (macOS, Windows,
case-folding Linux directories) and the gap between the check and the use.
"Same account" means the only party that could win the race already runs as
you, which is out of scope (above).

| Area | The check | The use | Bypass? | Result |
| --- | --- | --- | --- | --- |
| Workspace trust (`tui/src/app/trust.rs`, `session_key` in `tui/src/app/session.rs`) | The decision is keyed by the root's canonical path (symlinks resolved; on macOS and Windows `canonicalize` returns the on-disk case, so `~/Proj` and `~/proj` are one key) | `.vix/scripts`, `.vix/settings.toml`, language servers, read under the root | A symlink to the root, or a case variant, is the same key, so it is the same decision; re-pointing a link at a new clone gives a new key, which asks again | Not exploitable. Replacing a trusted directory's contents in place keeps the trust (already an open risk above). |
| Saving through a link (`write_atomic`, `crates/vix-fileops/src/lib.rs`) | `canonicalize` the target, then write a temp file beside the *resolved* target and rename it over | The resolved file | Another user who owns `/tmp/notes` swaps it for `/tmp/notes -> ~/.bashrc` after you open it; your save resolves the link (`readlink`, which Linux's `fs.protected_symlinks` does not cover) and replaces `~/.bashrc` | **Fixed.** Both writers refuse to follow a link in a sticky, world-writable directory unless you or the directory's owner own it (the kernel's rule), `PermissionDenied`; test `a_symlink_another_user_owns_in_a_sticky_shared_directory_is_not_followed`. **Closed (T857)** for the SSH save without the agent too: its shell script applies the same rule to each link it resolves and every ancestor, exiting with permission denied; test `a_save_does_not_follow_another_users_link_in_a_sticky_shared_directory` in `crates/vix-authority/tests/ssh.rs` (a fake remote `id` plays the other user). |
| Saving a dangling link (same) | `canonicalize` fails, so the path is used as given | The temp file is renamed over the link | None: the link is replaced and its target is never created | Not exploitable; pinned by `write_atomic_replaces_a_dangling_symlink_rather_than_creating_its_target`. The in-place fallback (directory not writable) follows the link, but then the directory also stops anyone else planting one. |
| Save conflict check (`write_atomic_if_unchanged`, `crates/vix-authority/src/lib.rs`, `crates/vix-authority/src/ssh/authority.rs`) | Length and modification time compared with what the buffer saw | The atomic write | A change inside the same second with the same length is missed; the local check runs only for a remote workspace | Not a security check: it guards against lost edits, not an attacker (who would have to be on the same account or remote host). |
| Explorer copy (`copy_recursive`, `crates/vix-fileops/src/lib.rs`; paste in `tui/src/app.rs`) | Source type from `symlink_metadata`; the conflict prompt asked `exists` | `fs::copy` / `create_dir_all` on the destination | A tree ships `notes.txt -> ~/.bashrc` (or a dangling `id.pub -> ~/.ssh/authorized_keys`); pasting a file of that name "overwrote" the link's target, and a dangling link did not even prompt | **Fixed.** A destination symlink, at any level, is removed and replaced; a dangling link is a conflict. Tests `copy_recursive_replaces_a_destination_symlink_instead_of_writing_through_it`, `a_dangling_symlink_at_the_paste_destination_asks_first`. Swapping a *source* file for a link between the stat and the copy needs a concurrent writer in the source tree (same account). |
| Remote copy without the agent (`crates/vix-authority/src/ssh/authority.rs`) | None on the destination | `cp -P` / `mkdir -p` + `cp -RP` | As above, on the remote host | **Fixed** for a top-level destination link (removed first; test `a_copy_replaces_a_destination_symlink_instead_of_writing_through_it` in `crates/vix-authority/tests/ssh.rs`). **Closed (T857)** for a link *inside* an existing destination tree: before `cp -R`, the script removes every link in the destination (`find -type l`, not following links) that the source tree has an entry for; test `a_copy_into_an_existing_tree_replaces_the_links_inside_it`. The agent path uses `copy_recursive` and replaces those too. |
| Explorer move, delete, trash (`move_path`, `remove_path`, `trash_path`, `freedesktop_trash.rs`; `tui/src/app/dir_edit.rs`) | `symlink_metadata` (a link is a link) | `rename`, `remove_file`, `remove_dir_all`, a rename into the trash | A directory replaced by a link to outside between the stat and the removal: `std::fs::remove_dir_all` removes a top-level link as a link and walks the tree without following links (the CVE-2022-21658 fix), and `rename` never follows its operands | Not exploitable. |
| Remote path mapping (`crates/vix-authority`) | None: a remote workspace's root is where it opens, not a jail | The remote account's own file system | A remote path can name anything the remote account can; that is the feature. Remote results never become local paths (backups and undo are keyed by `ssh://` names, hashed) | Not a boundary. Only `FakeAuthority` (`crates/vix-authority/src/fake.rs`, tests) confines paths to a backing directory, and it refuses `..` and escaping links (`crates/vix-authority/tests/api_misc.rs`). |
| Rhai scripts and plugins (`crates/vix-script/src/engine.rs`, `plugins.rs`) | The engine has no file access; a plugin's tree is hashed (links by target text) and links must resolve inside it | Scripts are read after the hash check | The hash and the load are two reads, but the plugin directory is in your config directory | Not exploitable (same account). |
| MCP file tools (`resolve` in `crates/vix-mcp-server/src/workspace.rs`) | Lexical join, then `canonicalize`, then a prefix check against the canonical root and `is_secret` on the canonical relative path | `fs::read` of the canonical path | A link out of the root, `..`, and `notes.txt -> .env` are all caught. `is_secret` compared `.git`/`.ssh` case-sensitively, so in a case-folding directory where `canonicalize` keeps the requested case, `.GIT/config` passed | **Fixed** (defense in depth: on macOS `canonicalize` returns the on-disk case, so it was not reachable there): directory names compare in ASCII lower case; tests `secret_files_are_recognized`, `a_case_variant_of_a_secret_directory_is_refused`. The resolve-then-read gap needs a concurrent writer in the workspace; the MCP tools are read-only, so the client cannot make one through Vix. |
| ACP agent paths (`contain_on_disk` in `crates/vix-acp/src/policy.rs`; `acp_read`/`acp_write` in `tui/src/app/acp.rs`) | Absolute, no `..`, lexically under the root, then the canonical path (or its nearest existing ancestor) under the canonical root | A read of the path; a write becomes a pending diff, saved later by you | A dangling link `root/planted -> /outside/new` cannot be canonicalized, so it was judged by its parent (inside) and accepted | **Fixed:** a dangling link on the way is refused; test `symlink_out_of_root_is_refused`. **Closed (T857):** the check runs when the agent asks and the save when you save, so an agent with its own write access to the workspace could swap the file for a link in between; now the App remembers each file an agent proposed, and the writer (`write_active_to_disk_with` in `tui/src/app.rs`, through `acp_save_refused` in `tui/src/app/acp.rs`) runs `contain_on_disk` again on the final path right before the write and refuses one that leads out; test `a_link_swapped_in_after_the_agent_asked_is_refused_at_save_time`. Left: the few system calls between that check and the write. |
| Hot-exit backups and undo histories (`crates/vix-undo-store/src/hot_exit.rs`, `crates/vix-undo-store/src/lib.rs`) | File names are a SHA-256 of the identity, or a validated id; a tampered backup is rejected | `write_atomic_private` (new files `0600`) under the data and config directories | Planting a link there needs your account | Not exploitable. A restored backup goes into a buffer, not to disk. |
| Crash reports (`tui/src/crash.rs`) and interrupted-save copies (`recovery_dir`) | None | `write_atomic_private` (was `fs::write`) of `crash-<stamp>.txt` under the data directory; `*.vixrecover` files created `0600` and `create_new` in a `0700` directory, published only after they read back intact (T865) | Same account | Not exploitable. **Closed (T857):** crash reports were written with the default mode (`0644` under the usual umask); they are now written with `write_atomic_private`, so a new report is `0600`; test `a_saved_report_is_private_to_its_owner` in `tui/src/crash.rs`. |

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

## Quarterly audit checklist

Once a quarter (January, April, July, October), the maintainer works through
this list on a clean, up-to-date checkout of `main`. CI already runs most of
these continuously; the audit is the point where someone reads the results
and the open items instead of only seeing a green run. Everything here is
local and read-only: anything outward-facing that comes out of it (an
advisory, a release, a repository setting) goes through
[incident response](incident-response.md) or the release process, and is the
maintainer's to do.

- [ ] **Advisories and licenses.** `cargo deny check` in the root, in
      `matrix/` (its own `deny.toml`) and with `--manifest-path
      gui/Cargo.toml`; `cargo audit` as a second opinion where it is
      installed. Every `ignore` in `deny.toml` still has a dated reason that
      holds; remove the ones whose advisory no longer applies.
- [ ] **Dependency review.** The Dependabot PRs and alerts since the last
      audit are merged or closed with a reason ([`spec/dependabot`](../spec/dependabot/index.md));
      each `ignore` in `.github/dependabot.yml` still needs to be there;
      new direct dependencies since the last audit (`git diff
      <last-audit>..main -- '**/Cargo.toml'`) are maintained, licensed per
      `deny.toml` and worth what they pull in; `cargo tree -d` duplicates
      have not grown without a reason.
- [ ] **Secrets in history.** `scripts/secret-scan-history` exits 0 over all
      refs; a finding is revoked first and handled as an incident, never by
      rewriting history unasked. `cargo test -p vix-tui --test secret_scan`
      passes and the `allow` lines in `scripts/secret-patterns` still have
      reasons that hold.
- [ ] **Threat model.** Each trust boundary above still matches the code:
      features added since the last audit that run something, read project
      configuration, talk to the network, hold a secret or take a path from
      outside are listed under their boundary (the new-feature checklist in
      their pull requests says which); each open risk is still open, or is
      marked closed with its task.
- [ ] **File-system audit.** Each row of the
      [file-system audit](#file-system-audit-t832) table still names the code
      that does the check and the test that pins it; a new place that checks
      a path and then acts on it gets a row.
- [ ] **Hardening claims.** Each statement in `SECURITY.md`'s Hardening
      section is still true (the weekly fuzz and mutation runs are green, or
      their failures have tasks; release checksums and SBOMs were on the last
      release); the supported-versions table names the current major.
- [ ] **Log it.** Add a dated line below with what was found and the tasks
      opened for it.

### Audit log

| Date | Result |
| --- | --- |
| 2026-11-27 | Checklist written (T832). `scripts/secret-scan-history` over all 2,672 commits of every ref: nothing found. The rest of the list starts with the first quarterly audit. |
