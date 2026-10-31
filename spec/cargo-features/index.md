# Capability Cargo features

Vix makes some capabilities optional at build time: the **DB** workbench, inline
**image** preview, **script**ing, the integrated **terminal**, the **HTTP**
client, the **AI** assistant, **Git** and **Jujutsu** integration, **LSP** and
**DAP** servers, the **tasks** shell runner, the **hunspell** probe, and **IM**'s IRC connections. Every
capability is **default-on**, so a released binary is unchanged; the features
exist for source builds that want a smaller or locked-down Vix.

This reverses T103/T111's "wired in = always compiled" decision, deliberately and
only for areas with a measured payoff (see `plan.md` § "Capability Cargo
features"): an area earns a flag when it removes many third-party crates (`sql`
48, `image` 81 before T583, `script` 9, `terminal` 10) or a whole outward surface
(`http`, `ai`, `git`, `jj`).

A second criterion (T595) is **security surface**: every place Vix starts another
program. `lsp`, `dap`, `tasks`, `hunspell` and `git` exist for that reason, not for
size. In a build without them Vix starts **no child process** of its own. `org` stays
unflagged: it has no payoff and the heaviest coupling.

## The table

| Feature | Owns action ids | Crates and code it gates |
| --- | --- | --- |
| `sql` | `db.*`, `tools.edit_sql` | `vix-db`, `vix-edit-sql` |
| `image` | (none; image tabs) | `image`, `ratatui-image` (43 exclusive crates; forwards to `vix-editor/image`) |
| `script` | `script.*`, `script:*` | `vix-script` (rhai; `tui/src/app/scripts.rs`, with `scripts_off.rs` no-op stand-ins) |
| `terminal` | `tools.terminal`, `terminal.*` | `vix-terminal`, `vt100` (portable-pty; `terminal_off.rs` no-op stand-ins) |
| `http` | `tools.http_*` (e.g. `tools.http_send`, `tools.http_copy_curl`), `tools.hl7_send` | `vix-http-client` (`http_off.rs` stand-ins) and `vix-hl7/net` (the MLLP socket sender; the HL7 parser and View stay in every build) |
| `ai` | `ai.*` | `vix-ai-core`, `-diff`, `-panel` (`ai_off.rs` stand-ins; Git's generated commit message and DB assist are `cfg(ai)`) |
| `git` | `git.*` | `vix-git/process` (the runners that spawn `git`; the crate keeps its data types and parsers) and `vix-doctor/probe-git`; implies `tasks`; `tui/src/app/git.rs`, with `git_off.rs` stand-ins |
| `jj` | `jj.*` | (surface only: no crates; `tui/src/app/jj.rs`); implies `tasks`, since it runs the `jj` CLI |
| `oc` | `oc.*` | `vix-oci/process` (the runner that starts `docker`/`podman`/`nerdctl`; the crate keeps detection, argv builders and parsers, and `vix-oci-panel` stays compiled); implies `tasks`, since logs, builds and Compose stream through the no-shell runner; `tui/src/app/oc.rs`, with `oc_off.rs` stand-ins |
| `im` | `im.connect`, `im.disconnect`, `im.join`, `im.leave`, `im.direct` (only the network actions; the IM panel, Notes, sharing, status, notifications and history are in every build) | `vix-im-irc/net` (the IRC connection thread: TCP plus TLS via `rustls`/`webpki-roots`; the codec and client state machine stay compiled), with `tui/src/app/im_net.rs` and `im_off.rs` stand-ins beside the always-compiled `tui/src/app/im.rs`; part of the `net` and `full` umbrellas (T752) |
| `lsp` | `lsp.*` | `vix-lsp/process` (starting a server) and `vix-doctor/probe-lsp` (the `--doctor` check that starts each configured server) |
| `dap` | `run.*` | `vix-dap/process` (starting a debug adapter) |
| `tasks` | `project.*`, `tools.run_command`, `tools.cancel_command`, `tools.tasks`, `tools.test*`, `edit.pipe_selection`, `edit.insert_command_output` | the runners `spawn_shell`, `run_shell_filter` (Pipe Through Command, T659) and `spawn_argv` in `tui/src/app.rs` (Run Command, project lifecycle, tests, the AI CLI provider; and `spawn_argv`, which runs `git`/`jj` with real argv and no shell, for clone, grep, branch delete and the jj commands that take user text, T569); plus `vix-format-tool/process` (spawning the user's Prettier), `vix-authority/process` (building any `Command` through the authority) and `vix-authority/ssh` (the shell-based SSH authority behind remote editing, T712–T717: without `tasks`, a remote folder cannot be opened) |
| `hunspell` | (none) | `vix-spellcheck/hunspell` (running `hunspell -D` to find dictionaries) |
| `cowork` | `cowork.*` | `vix-cowork`, `vix-cowork-net` (real-time collaboration, T690-T697; `cowork_off.rs` stand-ins). **Not** in `full` or `default`: the default build has no collaboration or listening code. See `spec/cowork` |
| `jupyter` | `jupyter.*` | `vix-jupyter` with its `process` feature (starting Jupyter kernels from their kernelspec argv and the ZeroMQ transport, T734b; `vix-jupyter-core` is pure and needs no feature); the Run → Jupyter wiring in `tui/src/app/jupyter.rs` (T734c/d; `jupyter_off.rs` stand-ins). **Not** in `full` or `default` until T734h measures its size inside Vix. See `spec/jupyter` |
| `full` | umbrella for every capability except `cowork` and `jupyter` (including `lsp`, `dap`, `tasks`, `hunspell`); `default` enables it, and whole-product tests gate on it | all of the above |
| `net` | umbrella for `http`, `ai`, `sql`, `im` | everything that can reach the network (`cowork` is separate: it is opt-in) |

`syntax-*` / `lang-*` (Tree-sitter grammars) are a separate, older axis and are
unchanged.

## Rules

- **Default-on.** `default` enables `full`, which lists every capability feature. `--no-default-features`
  is a supported build.
- **Gate at the root, filter at runtime.** `cfg` lives only in the `vix-tui` package
  (`tui/src/capability.rs` and, from T588, the gated modules). `vix-menu` and
  `vix-palette` stay `cfg`-free: `vix_menu::set_action_filter` hides any leaf the
  root's `capability::is_available` rejects, plus submenus and menus left empty
  and any stray separator. The palette applies the same predicate. With no
  filter set the menus are exactly as built.
- **Settings never lose data.** `Settings::save` writes back only the fields it
  knows, so a section can never be compiled out of `Settings` itself. The DB,
  DAP and org-capture types live in `vix-settings` (T584) and round-trip in
  every build.
- **A disabled id fails loudly.** `App::run_action` checks
  `Capability::for_action`; an id owned by a compiled-out capability — reached by
  keymap, a script's `bind_key`, or typed input — shows the translated
  *"X is not included in this build"* (`msg.capability_missing`, all locales).
- **AI also has a runtime switch.** Separately from the `ai` feature, the
  `ai = "off"` setting (T730) hides the AI menu and palette entries and refuses
  every AI request in a build that has the capability. See
  `crates/vix-ai-panel/spec/ai/index.md`.
- **Builds say what they are.** `vix --version --json` carries a `capabilities`
  array and `vix --doctor` / Help → Run Diagnostics a "build capabilities" row.
- **Prefixes are the contract.** `Capability::for_action` maps an action-id
  prefix to its capability; a new action under an owned prefix is gated for
  free. A new capability adds a feature, an enum variant, and prefix rows.

## Status

Run L is implemented except two items. Done: T586 (framework), T587 (CI for every
feature combination, `spec/ci/index.md`), and the capabilities `sql` (T588), `image`
(T589), `script` (T590), `terminal` (T591), `http` and `ai` (T592), and `jj` (T593).
Each gated capability keeps shared call sites `cfg`-free with a small `*_off.rs`
stand-in module or an uninhabited stand-in type.

`git` (second half of T593) is done as a surface-only lock-down: without it Vix
never spawns `git` (no status, gutter, inline blame, branch/log/conflict tools, Git
menu, dashboard commit count, or `git` probe in `--doctor`). The `vix-git` crate stays
compiled because `App`'s fields and the panel drawing use its plain data types; plain
file comparison (Tools → Compare with File) moved to `diff_tool.rs` so it works in
every build. T594 is decided below.

Tests: `tui/src/capability.rs` (prefix map), `vix-menu` (`filter_menus`), and, in a
`--no-default-features` build, `an_action_from_a_disabled_capability_warns_…`.

## Process spawning (T595)

Every first-party site that starts a program is behind a feature: `vix-git` runners
(`git`), `vix-lsp` and `vix-dap` (`lsp`, `dap`), `vix-doctor` probes, `vix-spellcheck`
(`hunspell`), the shell runner (`tasks`, which `git`, `jj` need), plus the already
gated `terminal`, `sql` (SSH tunnels, keychain helpers) and `ai` (keychain helper). The
`explorer_delete = "trash"` path uses `NSFileManager` on macOS rather than the crate's
default `osascript` child. `format` is not a capability: `vix-format-tool` pretty-prints
JSON/YAML/TOML in-process; its one spawn, the user's Prettier, sits behind its own
`process` feature, which root `tasks` turns on.

Since T710 the workspace spawns (the shell and argv runners, `vix-git`,
`vix-git-graph`, `vix-lsp`, `vix-dap`, Prettier) build their `Command` through
`vix_authority::current().command(…)`. `vix-authority`'s own `process` feature is
what lets any authority build one; each spawning crate's `process` feature (and root
`tasks`) turns it on, so a minimal build still has no way to start a process.

With `ai` but not `tasks`, only the HTTP providers work; the `cli` provider hands its
command to a shell and reports *"tasks is not included in this build"*.

What a `--no-default-features` binary still links: `crossterm`'s terminal-size fallback
(`tput`) is third-party code that runs only when the size `ioctl` fails; Vix cannot
remove it. Everything else was checked by reading the linked binary's symbols
(`nm`: the only caller of `Command::output` is `crossterm::terminal::sys::unix::tput_value`).

`tui/tests/spawn_sites.rs` keeps this list honest: it scans `tui/src/` and every crate's
`tui/src/` for process-spawning code and fails when a file outside its `ALLOWED` table
(file, reason, gating feature) contains one, or when an allowed file stops
spawning. A new spawn site must be gated behind a capability feature first.

## Measured

`cargo tree --no-default-features -e normal` has **zero** `sqlx`, `ureq`, `rustls`, `ring`, `keyring`, `rhai`, `portable-pty`, `vt100`, `image` or `ratatui-image` crates: with `net` (`http`, `ai`, `sql`, `im`) off, the build contains no TLS stack and makes no outbound network connections.

## Distribution (T594)

Decided with the maintainer: **ship a minimal variant** (`--no-default-features`,
"no features") as an extra release artifact, alongside the normal build. It ships as
plain per-target archives with checksums (`.github/workflows/release-minimal.yml`,
`spec/ci/index.md`), not as installers: dist builds one app per package and
Homebrew, npm and MSI each assume one binary per target. The normal installers and
SBOM are unchanged and still carry the full default build. Documented for users in
`docs/getting-started`.

Measured (macOS arm64, `cargo build --release`, 2026-10-18): the default build is
22,893,440 bytes (21.8 MiB); `--no-default-features` is 12,857,520 bytes (12.3 MiB),
about 44% smaller, with `vix --version --json` reporting `"capabilities":[]`. The
`binary-size` CI job reports the default build only; per-feature-off sizes are not yet
in its comment.

The release workflow builds the minimal variant with the dedicated `[profile.minimal]`
(inherits `release`; `lto = "fat"`, `codegen-units = 1`, `panic = "abort"`), whose purpose is
a reduced security surface, not size. `panic = "abort"` ends the process on a panic
rather than unwinding and carrying on with half-updated state, and fat LTO with one
codegen unit lets the linker drop dependency code no enabled feature reaches. The
binary is also smaller (7,800,336 bytes, 7.4 MiB, about 66% below the default build),
but that is a side effect. Panics abort instead of unwinding in that variant only.
