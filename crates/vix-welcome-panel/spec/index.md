# Welcome Panel

The first-run welcome screen's scroll state.

Vix shows this overlay the first time it runs (and on demand from **Help →
Welcome…**). The *text* lives in the host's i18n catalog (the `welcome.body`
locale key) so it is translatable; this crate is pure state — it holds the
lines the host hands it and tracks the scroll offset. The host renders the
visible window with a scrollbar and forwards scroll keys.

## What's New (T826)

**Help → What's New…** (action `help.whats_new`) shows the release notes for
the running version in this same overlay. The notes are bundled at build
time: `tui/build.rs` reads the repository's `CHANGELOG.md` and keeps
`whats_new::excerpt(changelog, CARGO_PKG_VERSION)`, every `## ` section
from the first (`[Unreleased]`) through the one headed `## [<version>]`
(only the first section when none names the version), which the host embeds
with `include_str!`. A build without the changelog (the crates.io package of
`vix-tui` does not carry the repository root) embeds an empty text, and the
overlay shows the translated `help.whats_new.empty` note instead.

`whats_new::paragraphs` turns the excerpt into plain paragraphs: a `## [x] -
date` heading reads `x (date)` over a rule of `─` (`[Unreleased]` reads
the translated `help.whats_new.unreleased`), `### Kind` reads `Kind`, a
`- ` bullet reads `• `, an indented line joins the paragraph before it,
blank runs collapse, `**` is dropped and a Markdown link keeps its text.
The overlay starts with the translated title `help.whats_new.title`. The
notes themselves are the English changelog, as written.

**Once after a version change.** At startup, after the first-run welcome
check, the host compares the version recorded in `session.toml`
(`last_seen_version`, see `crates/vix-session/spec/index.md`) with the
running one. When they differ, or none is recorded, it opens What's New and
records the running version, so it appears once per new version. On the
first launch the welcome overlay is already open; then the version is only
recorded, so a new user sees the welcome screen and not both. A failure to
record is a warning in Messages (`msg.session_save_failed`).

The welcome overlay itself is the first-launch Quick Tour (T826 found it
already existed): its dismissal is persisted by turning the
`startup.show_welcome_dialog` setting off when it is first shown, which stays
the user-facing switch to see it again. It is not duplicated in
`session.toml`.

## Checked arithmetic

The crate follows the `rust-checked-arithmetic` spec and, outside tests, denies `arithmetic_side_effects`, `integer_division`, `modulo_arithmetic`, `as_conversions`, `indexing_slicing` and `string_slice` (RCA-ENF-3). Scrolling uses `checked_*` with declared fallbacks; the old `saturating_*` calls are gone.

| ID | Failure | Declared fallback | Why it is correct |
| -- | ------- | ----------------- | ----------------- |
| `WEL-F1` | A scroll subtraction goes below zero | `0` | A short text does not scroll, and moving up stops at the top. |
| `WEL-F2` | A scroll step does not fit | the last valid offset | Moving down stops at the end of the text. |
