# Tools menu

The **Tools** menu is the cross-cutting home for everything that acts on the
buffer or the workspace without belonging to one editing domain: run something,
convert something, generate something, inspect something, or pick a character.

It is specified here, at the repo root, because no single crate owns it — each
item dispatches a `tools.*` action from `App::run_action` into the crate that
implements it. The menu's structure itself is specified in
[`crates/vix-menu/spec/index.md`](../../crates/vix-menu/spec/index.md); this page
maps each group to the crate that owns the behavior, so a change lands in the
right spec.

## Groups

| Group | Items | Owning crate(s) |
| ----- | ----- | --------------- |
| Run | Command Palette, Run Command, Cancel Command, Tasks, Run Tests, Test Panel, Terminal | `vix-palette`, `vix-tasks`, `vix-test-runner`, `vix-terminal` |
| Compare | Compare with File… | `vix-diff-view` |
| Language | Language Server… | `vix-lsp` |
| Insert ▸ | UUID/ZID, Lorem ipsum, Date/Time ([datetime](insert/datetime.md)), Markdown/HTML/SQL/[LaTeX](insert/latex.md)/Org fragments | `vix-uuid-tool`, `vix-zid-tool`, `vix-lorem`, `vix-clock-panel`, `vix-org` |
| Draw ▸ | ditaa ASCII shapes ([draw](draw/index.md)) | App shell (`App::draw_insert`) |
| Convert ▸ | CSV/TSV/JSON/TOML/YAML/Markdown/HTML converters, base and base64, URL, JWT, case | `vix-convert-from-*-into-*-tool`, `vix-convert-tabular`, `vix-base-tool`, `vix-base64-tool`, `vix-url-tool`, `vix-jwt-tool`, `vix-case` |
| Checksum ▸ | MD5, SHA-1/256/512, CRC32 | `vix-checksum-tool` |
| Format | Format Document | `vix-format-tool` |
| Generate | QR Code, Markdown Preview | `vix-qr-tool`, `vix-markdown-preview` |
| Inspect | Calculator, Regex Tester, Color Converter, Unit Converter, Pomodoro | `vix-calculator-tool`, `vix-regex-tool`, `vix-color-converter-tool`, `vix-unit-converter-tool`, `vix-pomodoro-tool` |
| Find | TODO Finder | `vix-textops` (`tag_column`) + App shell |
| Network | Send HTTP Request | `vix-http-client` |
| Pickers | Characters (Nerd Font / ASCII / HTML entities), X11 Colors, Media Types, Snippets, Contacts | `vix-nerd-font-picker`, `vix-ascii-character-picker`, `vix-html-character-picker`, `vix-x11-color-picker`, `vix-media-type`, `vix-snippets`, `vix-contact-panel` |
| Boxes | Calendar, Clock | `vix-calendar-panel`, `vix-clock-panel` |
| About | About this file / this text / this system | `vix-file-information-panel`, `vix-text-information-panel`, `vix-system-information-panel` |

## Rules

- One action id, one `run_action` arm — a Tools item never re-implements
  behavior that a crate already exposes.
- Every item's label and hover help are i18n keys (`menu.item.tools.*` and
  `menu.item.tools.*.help`) in `locales/app.yml`.
- A tool that transforms text is a pure function in its crate, driven through
  `App::transform_selection_or_buffer` or `App::rewrite_at_cursor`, so it is
  unit-testable without a terminal.

## Sub-specs

- [Insert → Date/Time](insert/datetime.md)
- [Insert → LaTeX](insert/latex.md)
- [Draw (ditaa)](draw/index.md)
