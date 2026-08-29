# Find

The find dialog: one box for every kind of search, opened with **Edit → Find →
Find…** (`Ctrl+F`) or the action id `find` / `edit.find`.

The box holds the query, the replacement, the match toggles, and — this is the
part that used to be spread across three menu items — **what kind of search this
is** and **where it looks**.

## Fields and options

| Row | Contents |
| --- | -------- |
| Find | The search pattern. `Tab` moves between fields. |
| Options | `Case`, `Smart case`, `Word`, `Regex`, `Replace`, `In: …` — each clickable, each with an `Alt` key |
| Replace | Shown when Replace is on; `Once` / `Ask` / `All` buttons act on it |
| Status | The match count, or the hint naming the options |

| Option | Key | Meaning |
| ------ | --- | ------- |
| Case | `Alt+C` | Match case exactly |
| Smart case | `Alt+S` | Case-insensitive until the query contains an uppercase letter |
| Word | `Alt+W` | Whole words only |
| Regex | `Alt+R` | Treat the query as a regular expression |
| **Replace** | `Alt+H` | Turn the find into a find-and-replace, focusing the replacement field. Turning it off returns focus to the query, keeps the replacement text, and ends interactive mode. |
| **In:** | `Alt+I` | Cycle the scope: **Buffer** → **Files** → **Workspace** |

## Scope

`Scope` (in `vix-find-panel`) is what makes one dialog enough:

| Scope | Where it looks | What shows the results |
| ----- | -------------- | ---------------------- |
| **Buffer** | The active buffer | The find box itself: matches highlighted, `Enter` steps through them |
| **Files** | Every file in the project | The workspace search panel — a hit list you can open, with replace-all |
| **Workspace** | Every file in the project | The bottom dock, as `path:line:col` lines that are click-to-jump |

Widening hands the whole search over: query, replacement, replace mode, and the
case/regex toggles all travel, so nothing is retyped. The panel is the *Files*
stage, so `Alt+I` there moves on to the dock listing. The workspace panel also
takes `Alt+H` to switch between find and replace, for the same reason the box
does.

This replaced three menu items — **Find in Files…**, **Replace in Files…**, and
**Find In Workspace…** — that were the same search with different destinations.
Their action ids remain for key bindings and the palette; what is gone is having
to close one dialog and retype the query into another.

## Engine

The matching itself is pure and testable: `matches` (regex → char ranges),
`replace_all` (capture groups, `$1` / `${name}`), `unescape` (`\n`, `\t`, `\\`),
and `PathFilter` (include/exclude path regexes for workspace search). The host
owns the state (`SearchBar`), the keys, and the rendering.

See `crates/vix-query/spec/index.md` for interactive query-replace and the
workspace panel, and `crates/vix-menu/spec/index.md` for the menu around it.
