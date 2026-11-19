# Crash reports

When Vix panics it saves a report on the user's machine, so a crash can be
reported with the information a maintainer needs. Nothing is ever sent
automatically: Vix's "no telemetry, no phone-home" promise
([`SECURITY.md`](../../SECURITY.md)) holds.

## Behavior

- **Saving.** `vix::crash::install_hook` (called in `main` right after
  `ratatui::init`) runs before the terminal-restoring hook. It writes
  `crash-<UTC timestamp>.txt` to `<data dir>/crashes/` (for example
  `~/.local/share/vix/crashes/`) and keeps the newest ten. After the terminal
  is restored it prints where the report is.
- **What a report holds.** The Vix version; the operating system and
  architecture; the panic message, at most 500 characters, with text between
  backquotes and double-quoted text over 40 characters replaced by
  `<removed>` (the standard library quotes string contents that way); the
  panic's location in Vix's source; and a backtrace. The home directory is
  replaced by `~` and the user name (three characters or more) by `<user>`
  everywhere. No buffer text, no environment, no host name.
- **Next start.** `App::start` reports the newest report once, in the status
  line and the message drawer (`status.crash_report_waiting`); a `last-seen`
  file in the same folder records that it was mentioned.
- **Help → Report a Crash…** (`help.report_crash`) opens the newest report
  in a read-only tab and asks (`confirm.report_crash`,
  `ConfirmKind::ReportCrash`) whether to open a prefilled GitHub issue. **y**
  opens `<repository>/issues/new?template=bug_report.yml` with the title,
  the "what happened" field (the report, at most 4,000 characters), the
  version and the OS filled in, through the system opener the LSP document
  links use (`app::doc_links::open_external`); the user reviews and submits
  it in the browser. When no browser opens, the link goes to the message
  drawer. **n**/Esc closes. With no report, the status says so.
- The privacy statement (**Help → Privacy Statement**) says all of this.

## Not done

- The desktop GUIs do not install the hook yet.
- Reports are not symbolized off-machine: a release build's backtrace has
  function names but may lack line numbers.

## Files

- `tui/src/crash.rs`: the hook, the report text and scrubbing, saving and
  keeping, `take_unseen`, the issue URL.
- `tui/src/app/crash_report.rs`: the start-up notice, the menu action and its
  confirmation.
