# Keybinding registry & override layer

`vix-script`'s `bind_key(key_token, command_id)` (T102) already lets a
script *ask* for a key; `LoadedScript::bindings` already *records* the
request. Nothing checks it against a real key event yet — that was T104's
original, narrower scope ("Script keybindings... via the existing
keymap-model override layer"). This spec exists because that "existing...
layer" turned out not to exist: this document designs it, so scripts (and,
built the same way, a user's own persisted overrides) have something real
to plug into.

**Status**: all 10 keymap ids are fully converted — T104a (Emacs), T104b
(Vi + Spacemacs), T104c (VS Code), T104d (`IntelliJ`), T104e (Eclipse),
T104f (Sublime Text), and T104g (Apple + `global_shared_key`, the last
slice). This spec now describes built behavior throughout, not intent —
each conversion updated this file if reality and design turned out to
disagree, same as T104a and T104b each already did once (§ "Schema
refinement, made during T104a" and § "A second schema addition, made
during T104b" below), T104c/T104d/T104e each reconfirming without needing
a third (§ "VS Code's own subtlety, found during T104c", § "`IntelliJ`'s
own subtlety, found during T104d", § "Eclipse's own subtlety, found
during T104e"), T104f finding nothing new at all, and T104g finding the
biggest departure yet (§ "Apple and `global_shared_key`'s own subtlety,
found during T104g" below): a genuinely mixed Shift-guard pattern within
one keymap, and a keymap-agnostic binding set (`global_shared_key`) the
original schema had no notion of at all.

**T104h done too**: `keybindings.toml` now round-trips for real —
`Settings::keybindings_path()` and `user_bindings::{UserBinding, load,
upsert}`, the `macros.toml` pattern copied verbatim (§ "Persisted-override
precedent" below, now built rather than planned).

**T104i done**: the `on_key` choke point is real. `overrides::{Source,
Override, Conflict, Shadow, Resolved, resolve}` (§ "Override layer"
below, now built) resolves a batch of override requests against each
other and against a keymap's built-ins; `App::override_key` — inserted
between `org_table_key` and every keymap's own `match`, exactly the one
insertion point the audit identified — consults the resolved
`self.key_overrides` map ahead of all 10 keymaps. `App::
load_key_overrides` reads `keybindings.toml` and calls the new,
separately-testable `App::apply_key_overrides(requests)`, which does the
actual resolve/report/store — split out specifically so T104j can feed
script `bind_key` requests into the very same call. Only user-sourced
(`keybindings.toml`) requests feed it today; `vix-script`'s
`LoadedScript::bindings` still isn't checked against anything — that's
T104j, the last remaining slice (§ "Staged plan") and the reason this
whole epic started.

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

### Registry API (as implemented, T104a/T104b — see the two schema-addition notes below)

```rust
/// One dispatch depth within a keymap: the top level (`""`), or a specific
/// chord prefix already typed, named by the key tokens typed to reach it,
/// space-joined (e.g. `"C-x"`, `"C-c C-x"`). A non-chorded keymap has
/// exactly one context, `""`.
pub struct ChordContext {
    pub name: &'static str,
    pub bindings: &'static [Binding],
}

/// Every built-in binding for one keymap id.
pub struct KeymapTable {
    pub keymap_id: &'static str,
    pub contexts: &'static [ChordContext],
}

/// All 10 keymaps' tables (built as `const` data, no runtime cost —
/// the same "pure data crate" shape as `vix-menu`'s `Item`/`TOOLS`).
pub const TABLES: &[KeymapTable] = &[ /* one entry per keymap id */ ];

/// The action bound to `token` in `keymap_id`'s `context` (top level =
/// `""`), if any.
pub fn lookup(keymap_id: &str, context: &str, token: &str) -> Option<&'static str>;

/// Every `(keymap_id, context, token)` triple bound to `action_id` — feeds
/// the F1 help overlay in place of `collect_menu_shortcuts` + the six ad
/// hoc chord-table walks, and is what makes "does this shadow a built-in"
/// checkable at all (§ Override layer, § "Overrides never see a
/// non-empty context").
pub fn shortcuts_for(action_id: &str) -> Vec<(&'static str, &'static str, &'static str)>;

/// (T104b) The result of matching a growing multi-character sequence — a
/// `key_token` that's a whole typed sequence, not one keypress, the shape
/// a leader-style accumulator needs (§ "A second schema addition, made
/// during T104b").
pub enum SequenceMatch {
    Action(&'static str),
    Prefix,
    None,
}

/// (T104b) Match `seq` against `keymap_id`'s `context` the leader way:
/// exact, valid-prefix, or neither.
pub fn lookup_sequence(keymap_id: &str, context: &str, seq: &str) -> SequenceMatch;
```

Each keymap's `App` dispatch function (`vim_normal_key`, `emacs_key`, …)
is rewritten, one keymap per task, to: decode the incoming `KeyEvent` to a
token via `vix_macros::encode_key`, call `vix_keybindings::lookup`, and
`run_action` on a hit — replacing the hardcoded match body with a table
lookup. Genuinely stateful behavior (Vim's operator-pending/count
accumulation once `vix-modal` lands, Emacs/Spacemacs's chord-prefix flags)
stays host-side control flow around the lookup, not inside it — a chord's
own *prefix-entry* keys (`C-x` starting a chord, not one of its leaves)
are never table entries either, for the same reason: they're a mode
transition, not a dispatchable action.

#### Schema refinement, made during T104a

The original design above this note used a flat `KeymapTable{keymap_id,
bindings}` — one list per keymap, no notion of chord depth. Reading
Emacs's actual dispatch (`emacs_key` plus five `*_chord_key` functions) to
convert it for real showed that doesn't work: the same token means
different things at different chord depths (`b` alone does nothing at the
top level; inside the `C-x` chord it switches buffers), so a flat table
can't represent a chorded keymap without collisions. `ChordContext` is the
fix — one named sub-table per dispatch depth, `""` for the top level.
Non-chorded keymaps (everything except Emacs and Spacemacs) will still
only ever need the one `""` context each.

**A real, unrelated bug found and fixed along the way**: the old
`EMACS_CTRL_X` const (used only for the which-key popup and the F1 help
overlay — never for actual dispatch, since the real `C-x` chord handler
was a *second*, separate hardcoded `match`) had drifted from that handler:
it claimed `b` ran a `"buffers"` action that didn't exist anywhere in
`App::run_action`, and it was missing the `C-b`/`0` bindings the real
handler accepted. Converting both into one shared `"C-x"` `ChordContext`
fixed this — `nav.switch_buffer` (new) is the one real action id both
`C-x b` and `C-x C-b` now run, in both the dispatch and the display. This
is the exact "drift risk" the original audit (§ "None of the 9 keymaps
are queryable today") predicted a hand-maintained shadow registry would
eventually hit — except it had already happened, in code that shipped
before this spec existed.

Two more, smaller findings from the same pass: Emacs's Meta (Alt)
bindings (`emacs_meta_key`, a sixth hardcoded match, entirely separate
from the Ctrl-chord one) turned out to be single keystrokes, not a chord
prefix — Meta+key is one physical key combination, unlike `C-x` (a full
keystroke *used as* a prefix for a second one) — so they belong as
ordinary entries in the `""` top-level context, not a context of their
own; folding them in retired `emacs_meta_key` as a separate function
entirely. And since the F1 help overlay only ever read the five
`EMACS_CTRL_*` consts (never the top-level Ctrl match or the Meta match),
neither set of bindings had ever been shown there — the unified table
fixes that too, as a side effect of having one true source instead of two.

#### A second schema addition, made during T104b

Converting Vim was routine — `vim_normal_key`'s single top-level `match`
plus its `g`/`d`/`y` pending-operator continuations mapped onto
`ChordContext` exactly the way Emacs's chords did (`""`, `"g"`, `"d"`,
`"y"`, one context each). Spacemacs's own `SPC`-leader did not.
`spacemacs_leader_lookup`'s actual algorithm — accumulate typed characters
into a growing string, then check: exact match on some binding → run it;
strict prefix of some binding → keep waiting; neither → abort — is a
prefix search over **whole multi-character sequences**
(`"ff"`, `"gs"`, …), not a series of fixed chord depths the way Emacs's
`C-x`/`C-c` families are. There's no natural "context per depth" here: the
character after `f` could continue into many different second characters,
all under one `f`-prefixed umbrella, not a small fixed set of named
depths.

Rather than force this into `lookup`'s single-keypress-per-context shape,
`vix-keybindings` gained one addition: [`SequenceMatch`] (`Action`/
`Prefix`/`None`, exactly mirroring `App`'s own now-deleted `LeaderHit`
enum) and `lookup_sequence(keymap_id, context, seq)`, reusing the *same*
`Binding`/`ChordContext`/`KeymapTable` data — a `Binding`'s `key_token` is
simply the whole sequence for this one context, `""` under keymap id
`"spacemacs"` — just queried differently. No other keymap needs this yet;
it's here for whichever future one does (a leader-style accumulator is a
recognizable shape, not unique to Spacemacs).

Spacemacs's shared Normal-mode vocabulary (motions, `i`/`a`/…) is **not**
duplicated under `"spacemacs"` at all — `spacemacs_key` delegates to the
very same `vim_normal_key` Vi uses, so `lookup("vi", ..., ...)` already
covers it; `"spacemacs"`'s own table holds only the leader context.

#### VS Code's own subtlety, found during T104c

VS Code's `vscode_ctrl_key` needed no schema change — every binding is a
plain `Ctrl`-held `Char`, no chords, so one `""` context sufficed exactly
like Vim's. What it *did* need was its own token-building function
instead of reusing `crate::macros::encode_key` (the way Emacs's and Vim's
conversions safely could): `encode_key` treats `Shift` as implicit in an
uppercase `Char` and never prefixes `S-` for one, on the assumption that
a physically-shifted letter always arrives with its case already
reflecting that. VS Code's original dispatch never made that assumption
— it checked the `Shift` *modifier bit* explicitly (`Self::shift(&key)`)
precisely because a terminal can report `Ctrl+Shift+p` as a **lowercase**
`p` with the bit set, not an uppercase `P`. Reusing `encode_key` as-is
would have silently collided `Ctrl+P` (Quick Open) and `Ctrl+Shift+P`
(Command Palette) on exactly that class of terminal. The table's tokens
therefore encode Shift explicitly (`"C-S-p"`, not the bare `"C-P"` a
case-based scheme would produce) via a small dedicated
`App::vscode_ctrl_token`, not `vix_macros::encode_key` — still valid
`vix-macros` grammar (`decode_key` already strips `S-` in any prefix
order), just not what `encode_key` itself would ever emit for a `Char`
key. Whichever keymap converts next should check the same question
before assuming `encode_key` is safe to reuse unmodified: does this
keymap's own dispatch rely on the Shift bit, the char case, or (as here)
both interchangeably in a way a naive token scheme could collide?

#### `IntelliJ`'s own subtlety, found during T104d

Two findings, neither a schema change. First: `intellij_key`'s dispatch
needed the same Shift-bit-explicit token function VS Code's did (renamed
generically to `App::intellij_ctrl_token` isn't shared code with VS
Code's — each keymap gets its own small token function, since what
counts as "this keymap's dispatch" differs — but the *reasoning* is
identical, restated here rather than assumed obvious). It also needed one
more modifier than VS Code: `Ctrl+Alt+L`/`Ctrl+Alt+O` are a single
keystroke's modifier combination (not a chord — nothing about them waits
for a second key), so they live as ordinary `"C-A-…"` tokens in the same
`""` context as everything else, the same reasoning T104a used to fold
Emacs's Meta bindings into its top level rather than a separate context.

Second, and more consequential: **`intellij-macos` and `intellij-windows`
are not one shared table**, unlike VS Code's. Converting the actual
dispatch (not just skimming its doc comment) showed the platforms
genuinely diverge — the "go to" family alone uses `Ctrl+O`/`Ctrl+Shift+O`/
`Ctrl+L` on macOS but `Ctrl+N`/`Ctrl+Shift+N`/`Ctrl+G` on Windows, plus
macOS-only (`Ctrl+,` → Settings) and Windows-only (`Ctrl+Y` → delete
line) bindings with no equivalent on the other platform at all. Two full,
independently-written tables (`intellij.rs`'s `MACOS`/`WINDOWS` consts) —
the ~13 genuinely shared bindings are duplicated across them rather than
factored into a shared slice, since at this size plain, readable
duplication beat a shared-slice indirection for two tables that might
keep diverging further as more of `IntelliJ`'s real keymap gets added
later.

One more thing worth recording precisely, since it would otherwise look
like a bug introduced by this conversion: **the original dispatch left
two bindings genuinely unguarded by Shift** — macOS's plain `Ctrl+N`
arm and Windows's `Ctrl+G` arm neither checked the Shift modifier at all,
so `Ctrl+Shift+N` does the same thing as `Ctrl+N` on macOS (`file.new`,
not a distinct "Go to File" the way `Ctrl+Shift+O` is on the same
platform), and `Ctrl+Shift+G` does the same as `Ctrl+G` on Windows (both
`nav.goto_line`). Each table lists the Shift variant as an explicit
second row with the identical action id, rather than "helpfully"
inferring it should be distinct — this is a faithful transcription of
what the original code actually did, not a design choice made here.

#### Eclipse's own subtlety, found during T104e

No schema change again — Eclipse is all-`Ctrl` (plus one exception, see
below), no chords, one `""` context. The now-expected Shift-bit-explicit
token function was needed again (`App::eclipse_token`, same reasoning as
VS Code's and `IntelliJ`'s). The one genuinely new wrinkle: Eclipse's
original dispatch has a binding that **isn't** a `Ctrl` chord at all —
`Alt+/` (word completion) — matched as its own leading case, entered only
when `Ctrl` is *not* held (`Self::alt(&key) && !Self::ctrl(&key)`), before
the function ever looks at `Ctrl`. Consequently `Ctrl+Alt+/` falls through
to the `Ctrl` branch and resolves to `edit.toggle_comment` (plain `Ctrl+/`'s
action) — `Alt` is simply never examined once `Ctrl` is present. Rather
than adding a second context for this one binding, `eclipse_token` builds
whichever single-prefix token applies (`"C-…"`, optionally `+"S-"`, when
`Ctrl` is held — `Alt` ignored in that case, matching the original; `"A-…"`
when only `Alt` is held), and the table carries `"A-/"` as an ordinary row
alongside the `"C-…"` ones in the same `""` context. Confirms the general
lesson again: a keymap not needing a schema change doesn't mean it has no
real shape to get right, just that the *existing* shape (one flat context)
happens to still fit.

Sublime Text (T104f) reconfirmed the same shape and the same
Shift-bit-explicit token need with no wrinkle of its own — the first
keymap in the chain to find nothing new at all, itself worth recording as
evidence the pattern had stabilized (not a gap in the audit).

#### Apple and `global_shared_key`'s own subtlety, found during T104g

The last slice, and the one that actually stretched the schema — two
distinct findings, neither a `ChordContext`/`SequenceMatch`-style schema
change, but both bigger than anything T104c–f needed.

**First: Apple's `apple_ctrl_key` genuinely mixes two Shift conventions in
one keymap**, unlike every prior conversion (which was uniformly one or
the other). Several letters (`o`/`s`/`w`/`t`/`b`/`f`/`g`) have an explicit
`if Self::shift(&key)` guard branching to a **different** action — the
same shape VS Code/`IntelliJ`/Eclipse/Sublime already needed. The rest
(`q`/`n`/`p`/`e`/`r`/`/`/`7`/`_`/`]`/`;`) never examine the Shift bit at
all — the same action fires either way. Rather than special-casing the
token function per letter, the table keeps one uniform Shift-bit-explicit
`apple_ctrl_token` and gives every Shift-agnostic letter an explicit
duplicate `"C-S-…"` row with the identical action id — the "faithfully
preserve an unguarded quirk" technique T104d introduced for `IntelliJ`'s
`Ctrl+Shift+N`/`Ctrl+Shift+G`, just needed far more broadly here (ten
letters, not two).

Two bindings still don't fit any table row, kept host-side in
`apple_ctrl_key` exactly like Eclipse's `Alt+/`: `Ctrl+Alt+R` (query
replace) is the only binding in this keymap that keys off `Alt` at all —
encoding `Alt` into the uniform token would have meant every *other*
letter also needing an Alt-agnostic duplicate row, so it stays a small
pre-check instead. `Ctrl+D` (forward delete) is genuinely focus-gated —
only claims the key while the editor pane is focused, left unclaimed
elsewhere so other panes keep their own `Ctrl+D` — which a static,
keymap-keyed table can't express at all (see the next finding, which hit
this same wall six times over).

**Second, and the actual scope surprise: `global_shared_key` isn't
keyed on a keymap id — it's the same function every one of the 9 `App`
dispatch functions falls back to, applying identically regardless of
active keymap.** The original schema (`KeymapTable { keymap_id, contexts
}`) has no way to represent "binds regardless of keymap" — forcing a
choice: invent an 11th pseudo keymap id, or a genuinely separate,
keymap-agnostic list. Picked the latter: a flat `SHARED: &[Binding]` plus
`lookup_shared(token) -> Option<action_id>`, outside `TABLES` entirely,
so the `every_keymap_id_has_exactly_one_table`-style invariant over the
real 10 ids stays meaningful.

Not everything in `global_shared_key`'s original dispatch fits even that
looser shape. Splits three ways:
- The `Alt+<letter>` menu-mnemonic branch is a dynamic lookup into the
  live menu structure (`menu_index_for_alt`) — never static data, stays
  host-side unconditionally, first in the function as before.
- Six arms are focus-gated (`Ctrl+Shift+Right`/`Left`, `Alt+Up`/`Down`,
  `Alt+n`/`p`) — same wall `Ctrl+D` hit above, `App::focus` is per-request
  runtime state a fixed table can't express without changing behavior for
  every other pane. Stay host-side, split into two residual `match`
  blocks (one before the `SHARED` lookup, one after — see below).
- Everything else — 13 bindings with no extra runtime condition — moved
  into `SHARED` for real.

**A genuine ordering hazard, caught by tracing the original top-to-bottom
`match` by hand rather than assuming order doesn't matter**: `Ctrl+Shift+
Right`/`Left` (focus-gated, in the original's first two arms) and
`Alt+Right`/`Left` (unconditional, now in `SHARED`) share the same two
physical keys. The original's arm order meant `Ctrl+Alt+Shift+Left` (an
admittedly obscure combination, but a real one) resolved to
`edit.select_less` while the editor was focused — the first matching
arm — not `nav.back`. Moving the unconditional `SHARED` lookup ahead of
the focus-gated check would have flipped that precedence for good, so the
focus-gated `Ctrl+Shift+Right`/`Left` check runs **first** in the new
`global_shared_key`, then falls to `SHARED`, then to the remaining
focus-gated arms — the last group never shares a key with `SHARED`'s
rows, so their relative order doesn't matter, only their position
relative to the Right/Left check does.

`SHARED`'s tokens also go slightly beyond the plain `Ctrl`-chord shape
every table so far used: named keys (`Tab`, `BackTab`, `Left`, `Right`,
`F1`–`F12`) and `Ctrl+Space`, built by a dedicated `App::shared_token`
(not `apple_ctrl_token` — the shapes don't overlap enough to share one
function). Only the `F`-key rows ever encode `Shift` (`F3` vs
`Shift+F3`); everything else in `SHARED` ignores the Shift bit entirely,
matching the original dispatch's own guards or lack of one — the same
"don't assume Shift needs handling uniformly" lesson as Apple's own
table, just for named keys instead of letters this time.

Four bespoke method calls needed real action ids for the first time this
task (mirroring T104a's `nav.switch_buffer`): `nav.back`/`nav.forward`
(`App::nav_back`/`nav_forward`, Alt+Left/Right) and `view.toggle_menu`
(`self.menu.toggle()`, F10). `view.toggle_explorer_focus` (Apple's
`Ctrl+E`) and `view.focus_other_pane`/`edit.find_next`/`edit.find_prev`/
`help.shortcuts`/`motion.delete_forward` (several `SHARED`/Apple rows)
already existed from earlier work and were reused as-is, not
re-invented.

### Override layer

As implemented (T104i — `crates/vix-keybindings/src/overrides.rs`):

```rust
/// Where an override request came from — for reporting, not for
/// precedence: a conflict between two requests is reported and rejected,
/// never silently resolved by one source outranking the other.
pub enum Source {
    Script(String),  // the script's file stem
    User,             // the persisted keybindings.toml
}

pub struct Override {
    pub key_token: String,
    pub action_id: String,
    pub source: Source,
}

pub struct Conflict { pub key_token: String, pub sources: Vec<Source> }
pub struct Shadow {
    pub key_token: String,
    pub action_id: String,
    pub source: Source,
    pub shadowed_action_id: &'static str,
}
pub struct Resolved {
    pub accepted: Vec<Override>,
    pub conflicts: Vec<Conflict>,
    pub shadows: Vec<Shadow>,
}

pub fn resolve(requests: Vec<Override>, keymap_id: &str) -> Resolved { .. }
```

Matches the original design almost exactly; the one addition is
`Conflict`/`Shadow`/`Resolved` as real return types instead of leaving
"a message names the token and both sources" as prose — `App` needed
*something* structured to build its two message strings from, and giving
the crate's own unit tests something concrete to assert on (7 tests,
§ below) was worth the small extra surface.

- **Overrides never see a non-empty context.** Neither `vix-script`'s
  `bind_key(key_token, command_id)` nor `keybindings.toml`'s schema has
  any notion of a chord — both take one token, always implicitly the
  `""` (top-level) context. `resolve`'s shadow check is always
  `lookup(keymap_id, "", token)`, never a `ChordContext` other than the
  top level; a script or user override simply can't shadow (or be
  shadowed by) an Emacs `C-x`-chord leaf, only a keymap's top-level
  bindings.
- **Persisted user overrides — done, T104h**: `Settings::keybindings_path()
  -> Option<PathBuf>` mirrors `macros_path()` exactly
  (`<config dir>/keybindings.toml`); `user_bindings::{UserBinding,
  KeyBindingsFile, load, upsert}` is a plain `toml`+`fs` load/save, the
  `macros.toml` pattern verbatim.
- **Script overrides**: still exactly `LoadedScript::bindings`
  (`vix-script`, T102/T103) — nothing new built here yet; T104j feeds
  these into the same `resolve()` call `App::apply_key_overrides` (below)
  already accepts.
- **The `on_key` choke point — done, T104i**: `App::override_key(&mut
  self, key: KeyEvent) -> bool`, inserted between `org_table_key` and
  `match self.active_keymap()` — the *only* new call site, covering all
  10 keymaps at once, exactly as designed. Builds the incoming key's
  token with `crate::macros::encode_key` (the shared `vix-macros`
  grammar every override source is authored in — deliberately *not* any
  single keymap's own Shift-bit-explicit convention, since an override
  isn't scoped to one keymap) and looks it up in `self.key_overrides: HashMap<String, String>`
  (token → action id), populated by resolution.
- **Resolution is split across two methods, deliberately**:
  `App::load_key_overrides` reads `keybindings.toml` and builds
  `Vec<Override>` (all `Source::User`); `App::apply_key_overrides(requests:
  Vec<Override>)` does the actual `resolve()` call plus reporting plus
  storing into `self.key_overrides` — pulled out specifically so T104j
  can pass a combined `Vec<Override>` (persisted + script-sourced) into
  the *same* method, and so integration tests can drive the choke point
  directly without touching the real, global `keybindings.toml` path
  (there's no project-scoped equivalent the way scripts have
  `.vix/scripts/`, so a test can't seed the file the way
  `script_reload_picks_up_a_script_added_after_startup` does — calling
  `apply_key_overrides` directly is the test seam instead).
- **Conflict handling — done, T104i, scoped to `keybindings.toml` alone
  for now** (fixes `crates/vix-script/spec/index.md`'s already-shipped
  contract — "a conflicting `bind_key` is reported, never silently
  clobbered" — will apply for real once T104j feeds script requests into
  the same `resolve()` call): `resolve()` groups every request by
  `key_token` in a `BTreeMap` (not a `HashMap` — deterministic grouping
  order, so the same input always produces the same message order, not
  just the same content). A token claimed by two or more requests is a
  real conflict: **all of them are rejected**, reported via
  `msg.keybinding_conflict` (`self.messages.error`) naming the token and
  every source (`Source::describe()`). A token an *already-accepted*
  request claims that a **built-in** binding in the resolved
  `keymap_id`'s table already owns is **not** a conflict — the override
  wins outright — but is reported once via `msg.keybinding_shadows_builtin`
  (`self.messages.info`), naming the shadowed built-in action.
- **The shadow check runs against whichever keymap is active when
  resolution happens** (`self.settings.keymap` at the moment `apply_key_
  overrides` is called — app startup, or `keybindings.reload`), not
  continuously. Switching the active keymap afterward doesn't retroactively
  surface (or retract) a shadow warning for a binding that only became
  a shadow under the new keymap — documented limitation, not a bug: the
  design's "reported once" already implies a point in time, and building
  a live-updating check for every keymap switch wasn't worth it for a
  feature whose v1 has no editing UI regardless.
- Nothing about *executing* the resolved action is new: an override that
  wins is just `self.run_action(&action_id)` — the same call every keymap
  handler and every `script:`-prefixed palette entry already funnel
  through.
- **New this task**: `keybindings.reload` (mirrors `script.reload`,
  including a Tools-menu leaf) re-runs `load_key_overrides` and reports
  how many overrides ended up active — the natural way to pick up a
  hand-edited `keybindings.toml` without restarting, matching the
  Conflict-handling bullet's own "at load time... `keybindings.toml`
  load/save" trigger list. It's the only new action id this task added —
  `override_key`/`apply_key_overrides` themselves need none, since a
  resolved override's action already has a real id by construction (it
  came from a `keybindings.toml`/script request naming one).

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
