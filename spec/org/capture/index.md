# Org Capture

Documentation: <https://orgmode.org/manual/Capture.html>

Introduction: <https://howardism.org/Technical/Emacs/capturing-intro.html>

**Status: implemented.** Pure logic lives in the unit-tested `vix-org-capture`
crate; `crates/vix-org/spec/index.md` § Capture has the operational summary.
This page is the design record — scope decisions below (see "Decisions") are
what shipped.

## Why the old capture wasn't `org-capture`

Before this design, the **Org → Capture** submenu had three fixed items
(`crates/vix-org/spec/index.md` § Menu):

| Item | Action | Behavior today |
| ---- | ------ | -------------- |
| Anything… | `org.capture` | Single-line prompt, pre-filled from `org_anything_capture_template`; inserts `* TODO <input>\n` at the cursor. |
| Contact… | `org.contacts.new` | Prompt for a name; insert a hardcoded contact skeleton (`vix_org_contacts::new_contact`, the `vix-org-contacts` crate) at the cursor. |
| Todo… | `org.capture_todo` | Multiline prompt pre-filled from `org_todo_capture_template`; inserts the text verbatim at the cursor. |

Every one of these is a *fixed, single-purpose* prompt: one hardcoded shape,
one settings field for the default text, always inserted at the cursor in
whatever buffer is currently focused. Real Emacs `org-capture` is a different
thing — a **global, template-driven system**:

- Any number of *named templates*, each with a key, a description, a target
  location, and a body with placeholders that expand at capture time
  (`%^{Prompt}`, `%t`, `%a`, …).
- Capture works from anywhere and files the result into a chosen
  destination — not necessarily the buffer you were just looking at.
- A template can prompt for several fields in sequence before inserting
  anything, e.g. the org-contacts template below, which asks for name, bio,
  email, phone, birthday, address, and eight social-profile URLs, one at a
  time, then assembles them into one property drawer.

This proposal generalizes Vix's three fixed capture actions into that model,
while keeping today's three as the built-in default templates so nothing
regresses.

## Example template (the motivating case)

```org
* %^{Name}
  :properties:
  :bio: %^{bio}
  :email: %(or my-org-contacts-capture-email "")
  :phone: %^{phone}
  :birthday: %^{birthday}
  :postal_address: %^{postal address}
  :country_code: %^{country code}
  :company: %^{company}
  :title: %^{title}
  :manager: %^{manager}
  :bluesky_url: %^{Bluesky URL}
  :codeberg_url: %^{Codeberg URL}
  :github_url: %^{GitHub URL}
  :gitlab_url: %^{GitLab URL}
  :instagram_url: %^{Instagram URL}
  :linkedin_url: %^{LinkedIn URL}
  :mastodon_url: %^{Mastodon URL}
  :end:
  :ways_of_working:
  %^{ways of working}
  :end:
  :notes:
  %^{notes}
  :end:
```

Compare `vix_org_contacts::new_contact` (`crates/vix-org-contacts/src/lib.rs`,
**removed** once capture shipped — see Decisions), which produced the *same
shape* but as Rust code emitting an all-blank drawer — no prompting, no
per-field values, no way for a user to add or drop a field without editing
Rust. A template engine turns that hardcoded function into data any user can
edit in `config.toml` (the shipped built-in `"c"` template uses a narrower
Email/Phone/Address/Birthday set, matching `vix-org-contacts`'s existing
uppercase `EMAIL`/`PHONE`/`ADDRESS`/`BIRTHDAY` fields used by its
directory/birthdays/vCard views — the richer template above, with its
lowercase keys and extra fields, is not parsed by those views and is left as
a **custom `org_capture_templates` entry** for anyone who wants it verbatim;
reconciling org-contacts' fields with it is a separate, not-yet-scoped
change).

## Placeholder syntax (proposed subset)

Emacs's real placeholder set is large and partly Elisp-backed (`%(sexp)`
evaluates arbitrary Elisp). Vix has no Elisp, so this proposes a pragmatic,
pure-Rust subset — everything that doesn't require evaluating arbitrary code:

| Placeholder | Meaning | Vix source |
| ----------- | ------- | ---------- |
| `%^{Prompt}` | Ask the user for text; insert the answer. Multiple `%^{}` in one template prompt **in order**, wizard-style. | `vix_org_capture::extract_prompts` + `App`'s per-step `Prompt` |
| `%^{Prompt\|c1\|c2\|...}` | The full `\|`-delimited list after the label; the prompt box pre-fills with `c1` (the org-priority idiom `%^{Priority\|0\|0\|1\|...\|9}` relies on this). Only the pre-fill is used today — the remaining candidates are parsed (`FieldPrompt::choices`) but not yet offered as a select popup. | `vix_org_capture::extract_prompts` |
| `%t` / `%T` | Today's date as an active timestamp `<2026-07-22>` / with time `<2026-07-22 Wed 14:30>`. | `jiff::Zoned::now()`, read at `App::capture_context` |
| `%u` / `%U` | Same, but inactive `[2026-07-22]` (doesn't count toward the agenda). | Same |
| `%<...>` | A tiny built-in `strftime` subset — `%Y %m %d %H %M %a %%` only — e.g. `%<%Y/%m/%d>` for a bare date string. No `chrono`/`jiff` dependency in the (pure) capture crate; `App` supplies the numeric year/month/day/etc. via `Context`. | `vix_org_capture::strftime` (private) |
| `%a` | Annotation: a link back to where capture was invoked, `[[file:path::line][desc]]`. | `App::capture_context` (active buffer path + cursor line) |
| `%i` | The current selection, if any was active when capture opened. | `Editor::get_selection_text` |
| `%f` / `%F` | Active file's name / absolute path. | `App::active_path` |
| `%c` | System clipboard contents. | `vix_clipboard::get` |
| `%^g` / `%^G` | Prompt for tags, inserted verbatim wherever the placeholder sits (usually a trailing `:tag:tag:`). No existing-tag completion yet — a plain text prompt formatted by `App::format_capture_tags` (space-separated → `:tag:tag:`). | `vix_org_capture::wants_tags` + `App`'s tag-prompt step |
| `%?` | Not inserted — marks where the cursor lands after the capture is filed. Parsed (`Expansion::cursor_offset`) but not yet acted on: `App::file_capture` doesn't move the cursor there. | `vix_org_capture::expand` |
| `%%` | Literal `%`. | Escape |

Not implemented: `%(sexp)` (arbitrary Elisp eval — no Elisp in Vix), `%^C` /
`%^L` (interactive clipboard/link chooser — `%c` and manual `[[...]]` entry
cover the common cases), `%x` (X selection — redundant with `%c` in a
cross-platform TUI), `%k`/`%K` (link to the clocked task — Vix's clock is
per-buffer, not a global clocked-task register), `%n` (see Decisions).

The example template's `%(or my-org-contacts-capture-email "")` — an Elisp
call that guesses an email from context — has no equivalent. Dropped; see
Decisions.

## Targets

Emacs targets: `file`, `file+headline`, `file+olp`, `file+olp+datetree`,
`file+function`, `id`, `clock`, `function`. Vix subset, ranked by how well
they map onto existing infrastructure:

| Target | Meaning | Maps onto |
| ------ | ------- | --------- |
| `cursor` (default) | Insert at point in the active buffer. | `App::insert_content` |
| `id:<ID>` | File as a child of the headline/node carrying `:ID: <ID>` (searched across every project `.org` file). | `App::file_capture_by_id` + `vix_org_capture::insert_under_id` |
| `file:<path>` | Append to the end of `<path>` (creating it if missing). | `App::file_capture_write` + `vix_org_capture::insert_top_level` |
| `file+headline:<path>#<Headline>` | File under a specific headline in `<path>`, creating the headline if absent. | `vix_org_capture::insert_under_headline` |
| `file+datetree:<path>` | File under today's date in a `Year > Month > Day` outline tree inside one file. Distinct from Roam Dailies (`daily/YYYY-MM-DD.org`, one file per day) — a datetree keeps one growing file; both exist side by side (see Decisions). | `vix_org_capture::insert_datetree` |

`file+olp` (arbitrary outline path) and `file+function`/`function` (fully
custom placement) are **out of scope** — `file+headline` covers the common
case, and custom placement functions require embeddable code Vix doesn't have.

## Entry types

| Type (`entry_type`) | Shape | Existing precedent |
| ---- | ----- | ------------------- |
| `entry` (default) | A headline: `* <expanded template>` | `org.capture` |
| `plain` | Raw text, no headline wrapper | `org.capture_todo` |
| `item` | A plain list item: `- <expanded template>` | none yet |
| `check-item` | A checkbox item: `- [ ] <expanded template>` | `org.toggle_checkbox` handles these once created |
| `table-line` | Wraps as a table row: `\| <expanded template> \|` | none yet |

## Properties

| Property | Meaning | Default |
| -------- | ------- | ------- |
| `prepend` | Insert at the top of the target (headline/file) instead of the bottom. | `false` |
| `empty_lines` | Blank lines to pad before/after the inserted entry. | `0` |
| `immediate_finish` | Skip the "review the expanded template before filing" step — file immediately once all `%^{}` prompts are answered. | `false` |
| `clock_in` | Splice a `CLOCK:` line (`org::clock_in`) right after the entry's first line. | `false` |

`jump_to_captured` was considered and dropped: `App::file_capture_write`
already opens every file target unconditionally (via `roam_write_and_open`,
matching Roam's own file-write precedent), so there was no distinct "stay put"
behavior left to gate behind a flag — see Decisions.

## Settings shape

Today's two fields (`org_anything_capture_template`, `org_todo_capture_template`)
were single strings. The template-driven system needs a **list** of named
templates — TOML's array-of-tables:

```toml
[[org_capture_templates]]
key = "t"
description = "Todo"
entry_type = "entry"
target = "cursor"
template = "* TODO %^{Task}\n  %U"

[[org_capture_templates]]
key = "j"
description = "Journal"
target = "file+datetree:journal.org"
template = "* %U %^{Entry}"

[[org_capture_templates]]
key = "c"
description = "Contact"
target = "cursor"
template = "* %^{Name}\n  :PROPERTIES:\n  :EMAIL: %^{Email}\n  :END:"
```

(`entry_type` and `target` both default — to `"entry"` and `"cursor"` — so
either can be omitted, as in the `j`/`c` examples above.)

`org_anything_capture_template` / `org_todo_capture_template` were removed
outright in favor of built-in entries seeded into `org_capture_templates`
(see Decisions); see `vix-settings/spec/index.md` § Org-capture templates for
the full field reference.

## Menu

**Capture → Anything… / Contact… / Todo…** run the built-in `"a"`/`"c"`/`"t"`
templates by key (`App::start_capture_by_key`); **Capture → Choose
Template…** (`org.capture.select`) opens every configured template in a
chooser (`CaptureChooser`), arrow-key/click selectable — this is where custom
templates beyond the three built-ins are reached, since `vix-menu`'s `Item`
tree is `&'static` (compile-time) and can't be populated from
`org_capture_templates` at runtime. Selecting a template (`App::start_capture`):

1. Runs its `%^{}` prompts in order (one prompt box at a time, via
   `PendingCapture`/`App::advance_capture`), then its tag prompt if it uses
   `%^g`/`%^G`.
2. Expands the remaining placeholders (`vix_org_capture::expand`) and wraps
   the result per `entry_type` (`vix_org_capture::wrap_entry`).
3. Files it immediately if `immediate_finish`; otherwise opens the expanded
   text in a multiline review buffer (`PromptKind::OrgCaptureReview`) the user
   can edit before filing (`App::file_capture`).

## Decisions

1. **Scope.** Full system in one pass: placeholder engine, all proposed
   targets (`cursor`, `id:`, `file:`, `file+headline:`, `file+datetree:`),
   entry types, and properties — not staged incrementally.
2. **Settings migration.** `org_anything_capture_template` /
   `org_todo_capture_template` are replaced outright by
   `org_capture_templates` (seeded with equivalent built-in entries on
   first run of the new schema). No dual system, no compatibility shim —
   matches this repo's "change the code, don't shim" convention.
3. **`%(or my-org-contacts-capture-email "")`.** Dropped. The email field
   becomes a plain `%^{Email}` prompt like every other field — no
   git-config-backed smart default.
4. **Datetree vs. Roam Dailies.** Both exist. `file+datetree:<path>` is a
   distinct target shape (one growing file, `Year > Month > Day` outline
   tree) alongside Roam Dailies' one-file-per-day. Journal-style templates
   can target either.
5. **`%n` (user name).** Deferred — no `user_name` setting yet; drop `%n`
   from the first implementation and add it if/when something needs it.
6. **Existing Contact capture.** `org.contacts.new` /
   `crate::org_contacts::new_contact` is replaced by a seeded
   `org_capture_templates` entry (`key = "c"`) producing the same drawer
   shape as editable data instead of hardcoded Rust.
