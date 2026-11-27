<!--
Vix's own checklist (AGENTS.md, agents/workflow.md, agents/conventions.md).
Tick what applies; strike through or say "n/a" with a reason for what does not.
-->

## What and why

<!-- One or two sentences. Name the task (T…) if there is one. -->

## Checklist

- [ ] **Spec first.** The owning spec (`crates/<crate>/spec/index.md`, or a
      repo-root `spec/` topic) says what this change does; code, spec and
      `docs/` agree.
- [ ] **Text is translated.** Every new user-facing string is a `t!` key in
      the right `crates/vix-i18n/locales/*.yml` file, in every locale its
      neighbors have, with the same `%{name}` placeholders.
- [ ] **Actions.** A new action id has one `run_action` arm and a menu leaf,
      a palette entry or a `vix-action-catalog` row, and `docs/reference/`
      is regenerated (`cargo run --example list_commands -p vix-tui -- --write`).
- [ ] **Rules hold.** No new `#[allow]` without a `reason`; no
      `unwrap`/`expect` on a runtime path; checked arithmetic for new
      arithmetic (`spec/rust-checked-arithmetic`); a new child process is
      listed in `tui/tests/spawn_sites.rs` and a new print in
      `tui/tests/code_lints.rs`, each with its reason.
- [ ] **Tested.** Unit or integration tests cover the change; a changed
      snapshot was read line by line.
- [ ] **CHANGELOG.md** has a line under `[Unreleased]` (user-visible changes
      only).
- [ ] **tasks.md** ticks the task (`- [x]`) with a short `(Done: …)` note.
- [ ] **`scripts/check` exited 0** on the final commit (the default full tier,
      ending in an `OK` line), not a hand-picked subset of its steps.
- [ ] **Commits** are SSH-signed, on one branch for this change, and an
      AI-assisted commit ends with a `Co-Authored-By:` trailer naming the model.
