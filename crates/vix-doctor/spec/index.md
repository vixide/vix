# Doctor

Environment checks for vix --doctor: git on PATH, configured LSP servers
spawnable, the active locale's spellcheck dictionary loadable, terminal
color support. Module `doctor`.

- menu "Help"
  - menuitem "Run Diagnostics"
- `vix --doctor` (CLI flag, `src/cli.rs`)

Both entry points run the same fixed set of checks and print the same
plain-text report — the CLI prints to stdout and exits before the TUI
starts; the menu entry shows the report in a read-only overlay (the same
[`vix::app::WelcomePanel`] shape `Help → License`/`Help → Report an
Issue` already use for plain multi-line text).

## Checks

Each check is independent and reports one of three outcomes: pass, fail,
or skip (a missing prerequisite for a check — e.g. no `lsp_servers`
configured at all — is a skip, not a fail, since there is nothing wrong
to report).

- **Git on `PATH`.** Spawns `git --version`. Fail if the spawn itself
  fails (the binary isn't found); pass otherwise, regardless of exit
  code — the goal is confirming the shell can find and start it, not
  validating its output.
- **Configured LSP servers spawnable.** One check per `Settings::
  lsp_servers` entry, spawning `command[0]` (its first arg only, not the
  full configured argument list — a server might require a real project
  root or other arguments to behave, and this check cares only whether
  the binary itself can be found and started). Skip entirely when no
  servers are configured.
- **Active locale's spellcheck dictionary loadable.** Calls
  [`vix_spellcheck::load_for`] with the current `Settings::
  dictionary_path` and `Settings::locale` — the exact discovery-and-parse
  path the running editor itself uses, not a separate existence check,
  so a pass here really means spellcheck will work.
- **Terminal color support.** Reads `TERM`: fail if unset or `"dumb"`.
  Notes (does not fail on) `NO_COLOR` being set, since that's a
  deliberate opt-out, not a misconfiguration.

## Design notes

A new crate rather than a method on [`vix::app::App`] because the CLI
entry point (`vix --doctor`) runs before any `App` exists at all — the
checks need to be callable from a bare `Settings` value with no
terminal, no event loop, no live editor state. `vix-doctor` depends on
`vix-settings` (for `Settings`/`LspServer`), `vix-spellcheck` (for
`load_for`), and `vix-i18n` (every check name/detail string is
translated via `t!`, see below); the App shell and `main.rs` are both
plain consumers, not the other way around.

Every check name and detail string is internationalized (T561: an
earlier version called `t!` zero times — a real hard-rule violation,
found by a self-audit, not caught at build time). `src/main.rs` sets
the process-wide locale (from `--locale` if passed, else the persisted
setting) *before* handling `--doctor`, specifically so the CLI path is
genuinely localized too, not just the in-app **Help → Run
Diagnostics** overlay (whose own locale is always already set by the
time it can be opened at all).
