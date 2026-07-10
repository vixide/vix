# Bottom Dock

The bottom dock is a scrollable line panel for log messages, terminal/command
output, data views, and similar text. It is a full-width strip that sits at the
bottom of the body, directly above the status bar.

## Show / Hide

Toggle the dock with **View → Show/Hide Bottom Dock**, the command palette
(action `view.bottom_dock`), or the `show_bottom_dock` setting. The setting is
off by default, and your choice persists between sessions.

When shown, the dock pins itself directly above the status bar; the explorer,
editor, and message drawer share the space above it. When empty it displays a
`(no output yet)` hint, and otherwise it shows the newest lines, pinned to the
bottom.

## Resizing

The dock's height is draggable. Press its top edge and drag up or down to grow
or shrink it. The height is kept between a 3-row minimum and a limit that always
leaves 3 rows for the body. The chosen height persists in the
`bottom_dock_height` setting.

## Focus and Scrolling

- Click the dock to focus it; its border brightens to show focus.
- Press `Esc` to return focus to the editor.
- While focused: `↑` / `↓` scroll one line, `PgUp` / `PgDn` scroll one page, and
  `Home` / `End` jump to the top / bottom.
- The mouse wheel scrolls the dock whether or not it is focused.

## Click-to-Jump

Clicking a line that names a `path:line` or `path:line:col` location — a build
error, a grep hit, and similar — opens that file at that position. This makes
command output and search results actionable directly from the dock.

## Producers

Two features write into the bottom dock.

### Search in Workspace → Dock

From **Edit → Find**, this prompts for a term, scans every workspace file, and
lists hits in the dock as `relpath:line:col: text`. Each result is
click-to-jumpable. In the prompt, `Alt+C` toggles case-sensitivity and `Alt+R`
toggles regex; the current state is shown under the input. The default is a
case-insensitive literal search.

### Run Command

From **Tools → Run Command…**, this runs a shell command in the workspace root on
a background thread and streams its output into the dock (showing it):

- a `$ command` header,
- the merged stdout/stderr lines as they arrive,
- an `[exit N]` footer when the command finishes.

The UI stays responsive while the command runs. **Cancel Command** kills a
running command and appends `[cancelled]`. Only one command runs at a time.

## Scrollback

The dock keeps a capped line buffer. The cap is the `scrollback` setting
(default 1,000 lines; minimum 1). When the buffer exceeds the cap, the oldest
lines are trimmed. Lowering the cap trims the existing buffer to fit. See
`../configuration/index.md`.

## Example

Run a build and jump straight to the first error:

1. Open **Tools → Run Command…** and enter `cargo build`.
2. The dock appears and streams the build output.
3. When a line like `src/app.rs:42:9: error[...]` appears, click it to open
   `src/app.rs` at line 42, column 9.

---

Vix™ and Vix IDE™ are trademarks.
