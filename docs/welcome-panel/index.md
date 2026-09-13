# Welcome Panel

The first-run welcome screen: a scrollable overlay of getting-started text
shown automatically the first time Vix™ runs.

## When it appears

Vix shows this overlay automatically on first launch. At startup, if the
`show_welcome_dialog` setting (see `docs/reference/settings.md`) is `true`
(its default), Vix opens the welcome overlay and immediately sets that
setting to `false` and saves it — so the overlay appears once, on the very
first run, and not on later launches.

You can also open it on demand at any time from **Help → Welcome…**, or set
`show_welcome_dialog` back to `true` in your settings to see it again on the
next launch.

## Content

The overlay's text lives in the host's i18n catalog (the `welcome.body`
locale key), so it is translated along with the rest of the UI. The crate
itself holds no text — it is pure scroll state: the lines the host hands it,
plus a scroll offset. The host renders the visible window with a scrollbar
and forwards scroll keys to it.

## Keybindings

| Key                       | Action                                          |
| ------------------------- | ----------------------------------------------- |
| `↑` `↓`                   | Scroll one line                                 |
| `Page Up` `Page Down`     | Scroll one page                                 |
| `Home` `End`              | Jump to the first / last line                    |
| Mouse wheel                | Scroll (the overlay is modal — the wheel only scrolls it) |
| `Esc`                     | Close the overlay                               |

## Notes

The same overlay type is also reused for a few other read-only text screens
reachable from **Help** — License, Report Issue, and Privacy — each just
hands it different lines. See the specification at
`crates/vix-welcome-panel/spec/index.md`.

---

Vix™ and Vix IDE™ are trademarks.
