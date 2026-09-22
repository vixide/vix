# Accessibility for screen readers

This page is about a **screen reader** actually announcing what Vix is
doing — a different, narrower question than color/contrast accessibility,
which T203's WCAG-AA `high-contrast` [theme](../themes/index.md) already
covers. A high-contrast theme helps a low-vision sighted user; it does
nothing for someone who can't see the screen at all.

## What a terminal can actually signal outside the visible screen

Two channels, and only two, reach a screen reader (or anyone else not
looking at the screen) without that person reading the live TUI redraw
directly:

- **The terminal bell** (`BEL`, `\x07`). Close to universal — every
  terminal either beeps audibly or, if configured for a *visual* bell
  instead, flashes. Not a rich channel (it can't say *what* happened,
  only *that* something did), but reliable and simple.
- **The terminal window/tab title** (`OSC 0`/`OSC 2`). Some screen
  readers announce a foreground window's title text when it changes
  (behavior varies by screen reader, terminal emulator, and OS — this is
  not a guaranteed channel the way the bell is close to being one).

That's the entire honest list. Nothing else a TUI does — an overlay
appearing, a status-line update, a color change — reaches a screen reader
at all unless the terminal emulator or its accessibility layer is doing
extra work on top, which most aren't.

## The bigger finding: Vix's own rendering model is the real barrier

Before shipping anything, it's worth being honest about what actually
matters most here — and it isn't the bell or the title. Real-world
accessibility reporting on modern TUI frameworks (Ink, Bubble Tea, tcell,
and — Vix's own foundation — `ratatui`) identifies a structural problem:
these frameworks treat the terminal as "a 2D grid of pixels" and
**redraw the whole screen on every update, repositioning the cursor
constantly**. Screen readers expect a mostly-linear, sequential flow of
text, the way a classic scrolling terminal program (or a well-behaved
`readline`-based CLI) produces one; a cursor jumping around a
full-screen redraw on every keystroke is, in the words of one detailed
write-up on the subject, enough to "make screen readers go nuts."

Vix fits this pattern exactly: `src/ui.rs`'s `draw` function repaints
the entire frame — menu bar, docks, editor, any open overlay — on every
tick, and the cursor moves with every keystroke, mouse click, and
overlay open/close. Nothing checked in this audit found any accommodation
for that today, and there isn't a small patch that changes it: it's the
same rendering architecture every overlay and panel in this codebase is
built on (`src/ui/*.rs`, the whole `panel!`-macro-dispatched family this
session's own [`git.conflict_list` overlay](../../crates/vix-conflict-tool/spec/index.md)
is one more instance of). Saying otherwise — implying a bell or a title
update makes Vix "accessible" — would be the wrong kind of confident.

Two more contributors compound this, confirmed by grep across this
codebase, not assumed:

- **Heavy box-drawing borders** (`Borders::ALL`, `BorderType::Rounded`)
  on nearly every panel and overlay — exactly the kind of decoration
  real screen-reader users report as noise, repeatedly announced
  ("box drawing light horizontal…") with no semantic value.
- **Nerd Font icons** (`vix-theme`'s icon set) used throughout menus,
  panels, and the status bar as the *only* representation of meaning in
  several places (a panel's title icon, a gutter mark) — private-use-area
  glyphs a screen reader either can't pronounce at all or announces as an
  unhelpful "unknown character."

## What shipped in this pass

One narrow, honest, clearly-safe win, exactly the shape the low-risk-only
brief for this audit asked for: **`Settings::accessibility.
bell_on_command_done`** (off by default — opt-in, so it never surprises
someone who didn't ask for it). When on, Vix rings the terminal bell when
a background command started via `run_command_in` (Project →
Compile/Run/Test, "Run shell command", …) finishes — useful for anyone,
sighted or not, who isn't watching the screen when a long-running command
wraps up. It does nothing about the structural problem above; it's a
small, real, additive improvement, not a fix for the bigger finding.

A terminal-title update was considered and **not** shipped this pass: it
needs care around restoring the terminal's original title on exit (a
titled terminal left renamed after Vix quits is a worse outcome than not
trying), and its actual announcement behavior is genuinely uncertain
across screen readers/terminals in a way the bell mostly isn't — a
real follow-on, not a "ran out of time" gap.

## What would actually move the needle (a real, sized follow-on — not undertaken here)

Other real TUI/CLI tools facing this same problem have converged on the
same shape: an explicit, opt-in **screen-reader mode** — a flag (or a
persisted setting) that switches rendering to something closer to the
classic linear-scrolling model a screen reader can actually parse:

- Disable box-drawing borders/dividers entirely in favor of plain blank
  lines or minimal ASCII separators.
- Replace icon-only signals (panel title icons, gutter marks) with a
  short text label alongside or instead of the glyph.
- Prefer appending new content (closer to how a classic terminal program
  behaves) over redrawing regions in place, where the UI shape allows it.

This is a genuine redesign of how panels render, not a settings field —
correctly out of scope for this audit, whose brief was research and any
small, safe wins, not a rendering rewrite. Recorded here, with the real
precedent that motivated it, so it isn't re-discovered from scratch: it's
exactly the shape terminal-based AI coding assistants and other modern
TUIs have shipped as an explicit screen-reader mode once teams looked at
this seriously, rather than something Vix would be inventing new ground
on.

## Bottom line

Vix is **not** meaningfully accessible to a screen reader today, and this
audit's job was to say that plainly and explain why, rather than let a
small bell setting stand in for having actually solved it. The bell
change is real and worth keeping; the structural fix is a genuine,
separately-sized project for whenever it's prioritized.
