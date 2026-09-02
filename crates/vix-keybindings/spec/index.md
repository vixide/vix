# Keybinding registry & override layer

`vix-script`'s `bind_key(key_token, command_id)` (T102) already lets a
script *ask* for a key; `LoadedScript::bindings` already *records* the
request. Nothing checks it against a real key event yet — that was T104's
original, narrower scope ("Script keybindings... via the existing
keymap-model override layer"). This spec exists because that "existing...
layer" turned out not to exist: this document designs it, so scripts (and,
built the same way, a user's own persisted overrides) have something real
to plug into.

**Status**: design-only. This spec is written before any of
`vix-keybindings`' functional code — the audit below is what justifies the
design, not a description of already-built behavior. Today the crate is a
documented no-op, same shape as `vix-script` was after T101 and `vix-modal`
after T111. T104a onward implement it in slices (§ "Staged plan"); each
should update this file if reality and design turn out to disagree.

## The audit

### One choke point, nine keymaps, no shared table

`App::on_key` (`src/app.rs:2519`) has exactly one place every keymap passes
through before its own dispatch runs — right after the last overlay/panel/
org-table guard, before the per-keymap `match`:

```rust
// src/app.rs:2551-2557
if self.org_table_key(key) {
    return;
}
// Keymap-specific dispatch. ...
match self.active_keymap() {
    Keymap::Apple => { if self.global_key(key) { return; } }
    Keymap::Vscode => { if self.vscode_key(key) { return; } }
    Keymap::Emacs => { if self.emacs_key(key) || self.global_shared_key(key) { return; } }
    Keymap::Vi => { if self.vim_key(key) || self.global_shared_key(key) { return; } }
    Keymap::Spacemacs => { if self.spacemacs_key(key) || self.global_shared_key(key) { return; } }
    Keymap::IntelliJMacOS => { if self.intellij_key(key, false) || self.global_shared_key(key) { return; } }
    Keymap::IntelliJWindows => { if self.intellij_key(key, true) || self.global_shared_key(key) { return; } }
    Keymap::Eclipse => { if self.eclipse_key(key) || self.global_shared_key(key) { return; } }
    Keymap::Sublime => { if self.sublime_key(key) || self.global_shared_key(key) { return; } }
}
```

An override check inserted here runs after every modal/overlay/prompt/
org-table guard (so it never steals a key from a context that legitimately
owns it right now) and before all 9 keymaps (so it genuinely intercepts,
for every keymap, in one place — not nine).

`App`'s private `enum Keymap` (`src/app.rs:879`) has 9 variants. The
already-shipped `vix-keymap-model::KEYMAPS` (`crates/vix-keymap-model/
src/lib.rs:34`) has **10** string ids — `vscode-macos` and `vscode-windows`
are separate persisted ids that both dispatch through the single
`Keymap::Vscode` variant, since VS Code's bindings are identical in a
terminal regardless of host OS (its own doc comment says as much). A
registry keyed on keymap has to pick one granularity; § Design picks
`vix-keymap-model`'s 10 ids (§ "Why 10, not 9").

### None of the 9 keymaps are queryable today — only 2 are even partly a table

- **Emacs** (`emacs_key`, `src/app.rs:3154`) is the closest thing to
  data-driven: top-level `Ctrl`-letter bindings are a hardcoded
  `match c.to_ascii_lowercase() { 'x' => ..., 's' => self.run_action("edit.find"), ... }`
  (`src/app.rs:3200-3224`), but every chord *continuation* below that is a
  real `&[(&str, &str)]` table looked up with `.iter().find(...)` — six of
  them: `SPACEMACS_LEADER`, `EMACS_CTRL_X`, `EMACS_CTRL_C`,
  `EMACS_CTRL_C_X`, `EMACS_CTRL_C_P_C`, `EMACS_CTRL_C_P_C_M`
  (`src/app.rs:20852`, `20938-21019`). E.g.:

  ```rust
  // src/app.rs:20938
  const EMACS_CTRL_X: &[(&str, &str)] = &[
      ("C-f", "file.open"),
      ("C-s", "file.save"),
      ("C-c", "file.quit"),
      ("k", "file.close"),
      ("b", "buffers"),
      ("o", "view.focus_other_pane"),
      ("2", "view.split_horizontal"),
      ("3", "view.split_vertical"),
      ("1", "view.unsplit"),
  ];
  ```

  Encouragingly, this table's own string format (`"C-f"`, bare `"k"`) is
  already a *subset* of `vix-macros`' `C-`/`A-`/`S-` token grammar — every
  entry here is also a valid `vix_macros::decode_key` token. What is
  *not* compatible is the **lookup key** the host derives from an incoming
  `KeyEvent` to search these tables with: `Self::chord_key_name`
  (`src/app.rs:3393-3401`) only ever produces `"C-<char>"` or a bare char —
  it drops Alt and Shift entirely, because nothing in these six tables
  needs them. A shared registry needs one lookup path
  (`vix_macros::encode_key`) that both this crate and any future new tables
  use, not two.

- **Every other keymap** — `vim_normal_key` (`src/app.rs:3459`), `vscode_key`
  (`2825`), `intellij_key` (`2880`), `eclipse_key` (`2935`), `sublime_key`
  (`2983`), `global_key`/`apple_ctrl_key` (`2767`/`2772`),
  `global_shared_key` (`3025`) — is a hardcoded `match`/`if` chain with
  **no** backing data structure. `apple_ctrl_key` alone:

  ```rust
  // src/app.rs:2776-2811
  match c.to_ascii_lowercase() {
      'q' => self.run_action("file.quit"),
      'n' => self.run_action("file.new"),
      'o' if Self::shift(&key) => self.run_action("file.open_recent"),
      'o' => self.run_action("file.open"),
      // ...
      'd' if !Self::shift(&key) && self.focus == Focus::Editor => {
          self.editor_motion(KeyCode::Delete);
      }
      // ...
      _ => return false,
  }
  ```

  Most arms are already `self.run_action("some.id")` — trivially
  table-able as `(token, action_id)`. A few are not: `'d' if ... =>
  self.editor_motion(KeyCode::Delete)` calls a method directly, with no
  action-id string to key a table entry on. **This is the real blocker for
  an exhaustive registry**, not just tedious transcription: a `(token,
  action_id)` pair can't represent a binding that has no corresponding
  action id yet. § Design's answer is to give every such bespoke call a
  real action id (a small, one-time addition per keymap, done as part of
  each keymap's own conversion task) so *every* binding, across all 9
  keymaps, reduces to the same shape.

### The existing "shortcut" displays are cosmetic, not data

`App::shortcut_rows` (`src/app.rs:14935-14994`, feeding the F1 help
overlay via `crates/vix-keyboard-shortcut-panel`) already aggregates three
sources — `keyboard_shortcut_panel::ROWS` (a curated static list),
`collect_menu_shortcuts` (walks `vix_menu::menus()`'s `Item.shortcut`
display strings), and the six Emacs/Spacemacs chord tables above — but
every one of them produces **display text** (`"Ctrl P"`, `"C-x C-f"`),
never something compared back against a live `KeyEvent`. `vix-keyboard-
shortcut-panel`'s own spec (`spec/index.md:44-48`) already lists a
"reverse lookup" (press a key, see what it does) as roadmap, not done —
confirming nothing today can answer "what does key K do" *programmatically*,
only "show the user this string next to that label."

### Persisted-override precedent: `macros.toml`

`crates/vix-macros/src/lib.rs:32-37` is the shape a new `keybindings.toml`
should copy exactly — plain `toml`+`std::fs`, not `confy::load`/`.save()`
(that's for `Settings` itself); `confy` is used only to *locate* the config
directory:

```rust
// crates/vix-settings/src/lib.rs:406-413 — the path-resolution half
pub fn macros_path() -> Option<std::path::PathBuf> {
    confy::get_configuration_file_path(APP_NAME, Some(CONFIG_NAME))
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("macros.toml")))
}
```

```rust
// crates/vix-macros/src/lib.rs:157-184 — the read/write half
pub fn load(path: &Path) -> Vec<Macro> {
    std::fs::read_to_string(path).ok()
        .and_then(|text| toml::from_str::<MacrosFile>(&text).ok())
        .map(|f| f.macros).unwrap_or_default()
}
pub fn upsert(path: &Path, mac: Macro) -> std::io::Result<()> {
    // find-or-push, then:
    let body = toml::to_string(&MacrosFile { macros })...;
    std::fs::write(path, body)
}
```

## Design

### Schema: every binding, built-in or override, is `(key_token, action_id)`

One shape, everywhere:

```rust
/// A single key → action binding, in `vix-macros`' token grammar
/// (`C-`/`A-`/`S-` prefixes, e.g. `C-c`, `S-Tab`, `Enter`, `a`) — the same
/// grammar `vix-script`'s `bind_key` already validates against.
pub struct Binding {
    pub key_token: &'static str,
    pub action_id: &'static str,
}
```

Getting there requires two things, both scoped into the staged plan below,
not attempted in one sweep:

1. **A real action id for every bespoke method call** a keymap handler
   makes directly (e.g. `apple_ctrl_key`'s `self.editor_motion(KeyCode::
   Delete)`) — each becomes a one-line `"some.new.action_id" =>
   self.editor_motion(KeyCode::Delete)` arm in `App::run_action`'s
   dispatch (no behavior change, just a name), so the binding itself can
   live in a table instead of only in the match.
2. **Each keymap's hardcoded `match` replaced by a table + lookup.** A
   single-key binding (the overwhelming majority) becomes one `Binding`
   row. A genuine multi-key *chord sequence* (`C-x C-s`, `SPC f f`) keeps
   its existing prefix/accumulator state machine — that part is real
   control flow, not data — but each chord's *leaf* bindings (what
   `EMACS_CTRL_X` etc. already are) move into this crate's tables, sharing
   one schema with every non-chorded keymap instead of a bespoke
   `&[(&str, &str)]` per chord level.

### Why 10 keymap ids, not `App`'s private 9-variant enum

The registry is keyed on `vix_keymap_model::KEYMAPS[i].id` (`"apple"`,
`"vscode-macos"`, `"vscode-windows"`, …, 10 total), not `App`'s private
`enum Keymap`. Two reasons: it's already `pub`, already stable
(persisted), and already the identifier both the View → Keymap menu and
`Settings` use — reusing it avoids a second, private, crate-local keymap
enum this crate would otherwise need. `vscode-macos` and `vscode-windows`
get identical binding tables (small, deliberate duplication) rather than
inventing a second, coarser enum just to avoid it.

### Registry API

```rust
/// Every built-in binding for one keymap id.
pub struct KeymapTable {
    pub keymap_id: &'static str,
    pub bindings: &'static [Binding],
}

/// All 10 keymaps' tables (built as `const` data, no runtime cost —
/// the same "pure data crate" shape as `vix-menu`'s `Item`/`TOOLS`).
pub const TABLES: &[KeymapTable] = &[ /* one entry per keymap id */ ];

/// The action bound to `token` in `keymap_id`'s built-in bindings, if any.
pub fn lookup(keymap_id: &str, token: &str) -> Option<&'static str>;

/// Every `(keymap_id, token)` pair bound to `action_id` — feeds the F1
/// help overlay in place of `collect_menu_shortcuts` + the six ad hoc
/// chord-table walks, and is what makes "does this shadow a built-in"
/// checkable at all (§ Override layer).
pub fn shortcuts_for(action_id: &str) -> Vec<(&'static str, &'static str)>;
```

Each keymap's `App` dispatch function (`vim_normal_key`, `emacs_key`, …)
is rewritten, one keymap per task, to: decode the incoming `KeyEvent` to a
token via `vix_macros::encode_key`, call `vix_keybindings::lookup`, and
`run_action` on a hit — replacing the hardcoded match body with a table
lookup. Genuinely stateful behavior (Vim's operator-pending/count
accumulation once `vix-modal` lands, Emacs/Spacemacs's chord-prefix flags)
stays host-side control flow around the lookup, not inside it.

### Override layer

```rust
/// Where an override came from — for reporting, not for precedence: a
/// conflict between the two is reported and rejected, never silently
/// resolved by one source outranking the other (§ "Conflict handling").
pub enum Source {
    Script(String),  // the script's file stem
    User,            // the persisted keybindings.toml
}

pub struct Override {
    pub key_token: String,
    pub action_id: String,
    pub source: Source,
}
```

- **Persisted user overrides**: a new `Settings::keybindings_path() ->
  Option<PathBuf>` mirroring `macros_path()` exactly
  (`<config dir>/keybindings.toml`); a `KeyBindingsFile { #[serde(default,
  rename = "binding")] bindings: Vec<UserBinding> }` with a plain
  `toml`+`fs` load/save, the `macros.toml` pattern verbatim — no `confy`
  serialization involved beyond locating the directory.
- **Script overrides**: exactly `LoadedScript::bindings` (`vix-script`,
  already implemented in T102/T103) — nothing new to build here, just
  something to finally check.
- **The `on_key` choke point**: `App::override_key(&mut self, key:
  KeyEvent) -> bool`, inserted between `org_table_key` and `match
  self.active_keymap()` (§ "One choke point" above) — the *only* new call
  site, covering all 10 keymaps at once.
- **Conflict handling** (fixes `crates/vix-script/spec/index.md`'s already-shipped
  contract — "a conflicting `bind_key` is reported, never silently
  clobbered"): at load time (script load, `script.reload`, and
  `keybindings.toml` load/save), every override's token is checked against
  every *other* override's token, regardless of source. Two overrides
  claiming the same token is a real conflict: **both are rejected**, and a
  message names the token and both sources — simpler and more predictable
  than picking a winner (a script or a user edit can't silently go quiet
  because something else happened to load first). An override claiming a
  token a *built-in* binding already owns (checked via `lookup`, now
  possible once a keymap's table exists) is **not** a conflict — that's
  what "override" means, the override wins outright — but it is
  **reported once, informationally** (`message()`-level, not `error()`),
  naming the shadowed built-in action, so a user or script author isn't
  surprised later when the built-in silently didn't fire.
- Nothing about *executing* the resolved action is new: an override that
  wins is just `self.run_action(&action_id)` — the same call every keymap
  handler and every `script:`-prefixed palette entry already funnel
  through.

## What's deliberately not in scope here

- **Rewriting Vim/Spacemacs's operator/motion grammar.** `vix-modal`
  (T111–T115) owns that; this crate's Vim/Spacemacs table only covers
  whatever `vim_normal_key` dispatches *today* (single hardcoded keys, no
  counts or operators) until that lands, at which point `vix-modal`'s
  motions/operators become this registry's Vim/Spacemacs entries instead.
- **A UI for editing `keybindings.toml`.** Hand-edit the file for v1, the
  same as `macros.toml`/theme JSON files today; a settings-panel editor is
  a separate, later feature if wanted.
- **Per-mode Vim bindings** (Normal vs. Insert vs. future Visual) as
  distinct registry entries — out of scope until `vix-modal` defines what
  "mode" means for real; today's registry only ever represents Vim's
  single Normal-mode table.

## Staged plan

Each is its own branch, its own gate run, its own merge — no single
giant, hard-to-review rewrite of `src/app.rs`'s key dispatch:

- **T104** (this spec).
- **T104a** — `vix-keybindings` crate: the `Binding`/`KeymapTable`/
  `lookup`/`shortcuts_for` API: convert the **Emacs** keymap first (it is
  already the most table-driven, so converting it proves the schema
  cheaply before touching the eight `match`-only keymaps). `App::
  shortcut_rows` switches to `shortcuts_for` for Emacs's contribution.
- **T104b–T104g** — one keymap each: Vim (+Spacemacs, which shares its
  Normal-mode table plus its own leader), VS Code, IntelliJ, Eclipse,
  Sublime, Apple/`global_shared_key`. Each adds any missing action ids its
  bespoke calls need, replaces its `match` with a table + lookup, and is
  verified against the existing integration-test suite (421 tests already
  exercise a great deal of key dispatch) plus a handful of new per-keymap
  smoke tests — zero intended behavior change, so a passing gate is the
  bar, not new features.
- **T104h** — `Settings::keybindings_path()` + `keybindings.toml`
  load/save (the `macros.toml` pattern).
- **T104i** — the `on_key` choke point (`App::override_key`) + conflict
  handling (§ "Conflict handling") for `keybindings.toml` entries.
- **T104j** — wire `LoadedScript::bindings` into the same choke point —
  the *original* T104 ask, now actually possible. Closes the loop opened
  by `crates/vix-script/spec/index.md`'s "Key bindings" section.
- **T105** (sample scripts + docs) stays blocked on T104j for any sample
  that demonstrates `bind_key` specifically; samples that don't need it
  are unblocked already (T103 shipped `register_command`/`prompt`/
  buffer editing).
