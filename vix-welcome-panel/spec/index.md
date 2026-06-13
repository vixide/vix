# Welcome Panel

A friendly, novice-oriented welcome screen shown the **first time Vix runs**, and
reopenable any time from **Help → Welcome…**. It explains what Vix is, how to get
started, what it can do, and how to send feedback.

## As implemented in Vix

**Status:** Shipped. The text and scroll state live in the internal
`vix-welcome-panel` crate (`LINES` is the content; `Panel` tracks the scroll
offset); the host (`src/app.rs`, `src/ui.rs`) renders the overlay with a
scrollbar and forwards scroll keys.

**First run.** The `welcomed` setting (default `false`) gates the automatic
appearance: the panel opens once on first launch, then `welcomed` is set so it
does not return on later launches. **Help → Welcome…** reopens it on demand.

| Key / action   | Effect                          |
| -------------- | ------------------------------- |
| `↑` / `↓`      | Scroll one line                 |
| `PgUp`/`PgDn` / `Space` | Scroll one page         |
| `Home`/`End`   | Jump to the top / bottom        |
| mouse wheel    | Scroll                          |
| `Esc` / `Enter` / `q` | Close the panel          |

## Content

The welcome text covers, in friendly language for newcomers:

- What Vix is (a fast, keyboard-friendly terminal editor/IDE).
- Getting started: menus (`F10`), open/save/close, the explorer (`Ctrl+B`), the
  Command Palette (`Ctrl+P`), and the shortcut reference (`F1`).
- What Vix can do: editing, find/replace, the file explorer, Git, the character
  and color pickers, LSP features, and themes/locales/keymaps.
- That settings persist between sessions.
- How to give feedback — the project website and email.
