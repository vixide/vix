# Modal editing (`vix-modal`)

The Vi keymap's Normal mode is a **binding table**, not a modal engine: a
flat `match` of hardcoded key sequences in the App shell, not composable
motions an operator can apply to. `vix-modal` is the real engine — this
spec is the T111 audit (what exists today, precisely, so "the gap is real"
is a finding and not an assumption) plus the v1 design it justifies.

**Status**: design-only (improvement plan T111). T112 (mode engine) through
T115 (text objects + dot-repeat) implement it in slices; each should update
this file if reality and the design disagree, same as anywhere else. Today
the crate is a documented no-op: no dependencies, no public items, just
this spec and the crate-root doc comment pointing here.

## The audit

Everything today lives in `src/app.rs`, as flat match-based methods on
`App` — there is no separate module, no `Mode` enum, no motion/operator
abstraction.

**Dispatch.** `App::on_key` routes `Keymap::Vi` to `vim_key`, which (when
not in the `:` command line or Insert mode) hands everything to
`vim_normal_key` for `Focus::Editor`. `Keymap::Spacemacs` routes to
`spacemacs_key`, which — after its own `SPC`-leader handling — falls
through to **the same** `vim_normal_key` for its Normal-mode vocabulary.
Spacemacs Normal mode is not a second implementation; it is the Vi one,
inheriting every gap below verbatim, plus a leader-key layer on top.

**Mode state** is three separate `App` fields, not a `Mode` enum:
`modal_insert: bool` (Insert vs. Normal, shared by Vi and Spacemacs),
`vim_cmd: Option<String>` (the `:` command line, its own pseudo-mode), and
`vim_pending: Option<char>` (the first key of a 2-key sequence). **Visual
mode does not exist**: no flag, no selection-mode field, no `v`/`V`/`Ctrl-V`
binding. The underlying editor's `Action` trait already plumbs
`MoveLeft/Right/Up/Down { shift: bool }` end-to-end (shift-arrow already
extends a selection) — Vi's own motion dispatch (`editor_motion`) simply
never passes `shift: true`. The substrate for Visual mode already exists;
nothing wires it.

**Motions**: `h j k l`, `0`/`^` (not distinguished — both do "smart Home"),
`$`, `w`/`b` (a third, local word-boundary scanner, distinct from both
`vix-textops` and `vix-editor-core`'s own), `gg`/`G`, `%`. Each is a direct
`self.editor.cursor_*()` call with no reusable return value — nothing here
is a function an operator could call to get a range. **Not bound at all**
today, though the underlying cursor methods already exist and are wired to
the Go menu: `e` (word end), `f`/`t`/`F`/`T` + char, `{`/`}` (paragraph),
`(`/`)` (sentence).

**Operators don't compose.** `vim_pending` supports exactly three literal
2-key sequences, hardcoded: `gg`, `dd` (→ cut the whole line), `yy` (→ copy
the whole line). `d` + any other motion (`dw`, `d$`, `dG`, `df x`, …) is
silently swallowed — only the doubled key works. `c` (change) has no key
arm at all. `x` is its own hardcoded forward-delete-one-char action, not
`d` composed with a one-character motion.

**No counts.** No count field anywhere; a bare digit before a motion is
silently dropped (`3dw`, `5j`, `2gg` all do nothing useful — corroborated
by the project's own `docs/for-vim-users/index.md`: "no counts (`3w`)").

**No named registers.** `dd`/`yy`/`p` all go through the single OS
clipboard (`vix_clipboard`, with an in-memory fallback) — the same one
every other keymap's Cut/Copy/Paste uses. There is a `clipboard_ring`
(paste-from-history, cap 30) but it is not register addressing.

**No dot-repeat.** No tracking anywhere of "the last change." The
raw-keystroke macro recorder (Edit menu, manual start/stop) is unrelated —
a real, working substrate `.` could be built on, but nothing wires it that
way today.

**No text objects.** No inner/around grammar at all. The Tree-sitter/LSP
"expand selection to enclosing node" feature (`expand_to_node`,
`request_selection_range`) is a different mechanism — syntax-tree-driven,
expand-only, LSP-first — not a stand-in for `iw`/`i(`/`a"`.

**Already-solid substrate to build on, not reinvent**:
- `vix-textops` — pure, cursor-relative `word_units`/`sentence_units`/
  `paragraph_units`/`section_units`, already used by Emacs-style bindings.
  The natural motion-layer foundation, though it currently knows only one
  "word" kind (no Vim `word` vs `WORD` distinction) and is whole-buffer
  string based, not rope-based.
- `vix-editor-core`'s `Action` trait + `MoveLeft/Right/Up/Down { shift }` —
  selection-extension plumbing already end-to-end; Visual mode is a wiring
  problem, not a missing-substrate one.
- The single clipboard abstraction — the natural unnamed register (`"`).
- Generic find/find-next/find-prev — `/`, `n`, `N` need no new engine work.
- The macro recorder — reusable scaffolding for dot-repeat.

Three independent word-boundary implementations exist today (`vix-textops`,
`vix-editor-core::named`, and a third local one in `vix-editor`). `vix-modal`
should pick one canonical source — `vix-textops`, since it is already the
shared pure crate other keymaps depend on — rather than add a fourth.

## Design: modes

Four modes for v1, a genuine `enum` this time:

```rust
enum Mode {
    Normal,
    Insert,
    Visual,      // character-wise
    VisualLine,  // line-wise
}
```

**Cut from v1**: Visual Block (`Ctrl-V`). Character and line-wise cover the
common case; block edits are a materially bigger feature (column-wise
insert/change across multiple lines) that deserves its own slice later, not
a rushed corner of this one.

The engine intercepts before the old ad hoc handling when the active
keymap is Vi or Spacemacs *and* a new settings flag is on (see § Rollout) —
it does not coexist key-by-key with `vim_normal_key`; once active, it owns
Normal, Insert-entry-detection, and Visual mode dispatch completely for
those keymaps. The `:` command line, Insert-mode's own text-editing pass-
through, and Spacemacs's `SPC`-leader stay exactly where they are today, in
the App shell — `vix-modal` calls back into host-supplied callbacks for
"enter Insert mode," "open the `:` line," etc., the same shape as
`vix-script`'s host-callback API (`crates/vix-script/spec/index.md`).

## Design: motions

Pure functions, operating on **character offsets** into `&str` (matching
`vix-find-panel`'s and `vix-script`'s convention), not `(line, col)` and not
bytes: `fn(text: &str, pos: usize, count: usize) -> usize` is the shape for
a simple motion; a few (paragraph/sentence, which have a natural start/end)
return a range directly.

v1 motion set — exactly `tasks.md` T113's list, no more:

```
h j k l          — char/line stepping
w b e            — word motions (lowercase only — see cut line)
0 ^ $            — line start / first non-blank / line end
gg G             — document start / end
{ }              — paragraph start / end
( )              — sentence start / end
f t F T + char   — find/till a character, forward/backward
%                — matching bracket
```

Each takes a `count` (see § Counts) that multiplies its effect (`3w` = the
3rd next word start, `5j` = 5 lines down). Reuse `vix-textops`'s
`word_units`/`sentence_units`/`paragraph_units` where the semantics already
match, rather than a fourth reimplementation — that crate may need a small
extension (e.g. a `nth` parameter) rather than being called in a loop, which
is T113's call once it's in the code.

**Cut from v1**: `W`/`B`/`E` (WORD — whitespace-delimited, vs. word's
alphanumeric-run definition), `;`/`,` (repeat last `f`/`t`), `*`/`#`
(search word under cursor). Each is a small, self-contained addition once
v1's motion shape exists — deliberately not bundled in now.

## Design: operators

`d c y` compose with **any** motion above, a text object (§ below), or the
active Visual selection — the actual fix for "operators don't compose":

```
d{motion}   delete the range, into the active register
c{motion}   delete the range, into the active register, enter Insert mode
y{motion}   copy the range into the active register (buffer unchanged)
```

`x` becomes real sugar for `d` + a one-character-forward motion, not its
own code path — matching Vim's actual semantics (`x` **is** `dl`) instead
of vix's current bespoke "forward-delete" action.

`p`/`P` are not operators — they're Normal-mode commands that read the
active register and insert its content after/before the cursor (or replace
the Visual selection, in Visual mode). Line-wise vs. character-wise paste
follows how the register was written (`yy`/`dd` produce a line-wise
register; `yw`/`dw` produce a character-wise one) — the same distinction
real Vim makes, since it's what makes `p` "paste this as its own line vs.
paste this inline" behave correctly without the user thinking about it.

**Cut from v1**: no other operators (`>`/`<` indent, `~` case-toggle, `gu`/
`gU`) — `d c y (x) p P` only, matching `tasks.md` T114's exact list.

## Design: counts

A single accumulating numeric-prefix state, reset on every mode-affecting
key. Vim's actual composition rule — a count before the operator **and**
a count before the motion multiply (`2d3w` deletes 6 words) — is worth
stating explicitly here since it's easy to under-implement as "only one
count slot": `{count1}{operator}{count2}{motion}` → effective count is
`count1 * count2` (each defaulting to 1 when absent).

## Design: registers

The unnamed register `"` is special: reads and writes mirror the real OS
clipboard (via the same `vix_clipboard` abstraction everything else already
uses), so Vi register `"` and every other keymap's Copy/Cut/Paste keep
round-tripping through the same clipboard exactly as today — nothing about
existing non-Vi behavior changes. Named registers `a`–`z` (`"ayy`, `"ap`)
are a small in-memory map, **not** persisted across restarts — unlike the
undo tree, session-only is an explicit, deliberate v1 simplification, not
an oversight.

**Cut from v1**: uppercase registers (`"A` = append to `a` rather than
replace it), numbered registers (`"1`–`"9`, the delete-history ring),
special registers (`"%` current filename, `"/` last search, `".` last
insert). Named a–z plus the unnamed clipboard register covers the common
case; the rest is a natural follow-on once the register map exists.

## Design: dot-repeat

Not semantic replay ("do the last operator+motion+count again, recomputing
everything") — **keystroke replay**, matching what the audit found is
actually the closest existing substrate: record the exact key sequence of
the last change (an operator+motion, a full Insert-mode session from entry
to `Esc`, or a single-key command like `x`/`p`), and replaying `.` re-runs
those keys through the same dispatch. This reuses the macro
recorder/player's existing mechanics conceptually — "auto-record a
one-shot macro of the last change, replay on `.`" — rather than building a
second, hand-rolled replay engine.

`{count}.` overrides the recorded count with `{count}` (real Vim's `.`
behavior: `3.` repeats the last change but 3 times / with 3 instead of
whatever it originally used), not just an unconditional exact replay.

A "change" for this purpose is: an operator+motion/text-object pair, `p`/
`P`, or an Insert-mode session (everything typed from entering Insert to
leaving it counts as one change, matching Vim). Pure motions (`w`, `gg`,
`/pattern`) do not update what `.` repeats — moving the cursor isn't a
change.

## Design: text objects

`i`/`a` (inner/around) + one of: `w` (word), `(`/`)`/`b` , `{`/`}`/`B`,
`[`/`]`, `<`/`>`, `"`, `'`, `` ` `` — exactly `tasks.md` T115's
`iw aw i( a( i" a"` plus the natural bracket/quote siblings it implies.
Pure functions too: `fn(text: &str, pos: usize, count: usize) -> Option<(usize, usize)>`
(a text object can fail to find its delimiter — e.g. `di"` with no quote on
the line — unlike a motion, which always lands *somewhere*).

Delimiter-pair objects (`i(`, `a"`, …) are a **character/bracket-matching
scan**, not the Tree-sitter structural expand feature — different
mechanism, deliberately: it needs no grammar, no LSP, works on any file
type, and matches Vim's actual (non-syntax-aware) definition of "the
quote/bracket pair around the cursor." The structural expand-to-node
feature stays exactly what it is today (a separate, syntax-aware "select
enclosing node" tool) — a plausible future `i{node}`-style object once v1's
delimiter-based ones exist, not a replacement for any of them now.

**Cut from v1**: sentence/paragraph text objects (`is`/`as`/`ip`/`ap` —
motions `(`/`)`/`{`/`}` already exist; the objects are a small follow-on),
tag objects (`it`/`at` for HTML/XML — `vix-tags`' matching-tag jump is a
related but distinct feature already shipped elsewhere), custom
user-defined objects.

## v1 cut line, summarized

Explicit, per `tasks.md` T111 ("no ex commands, no macros — Vix already has
macros") plus the per-section cuts above, gathered in one place:

- **No ex-command scripting** (`:%s/…/…/g`, `:g/pattern/…`) — the existing
  simple `:` commands (`:w`, `:q`, `:42`, `:e path`, …) are unrelated to
  `vix-modal` and unchanged.
- **No macro-via-`q`/`@`** — Vix's existing Edit-menu macro recorder is a
  separate, already-shipped feature; not being duplicated or replaced.
- **No Visual Block.** Character and line-wise Visual only.
- **No WORD motions** (`W`/`B`/`E`), no `;`/`,`, no `*`/`#`.
- **No operators beyond `d c y (x) p P`** (no `>`/`<`/`~`/`gu`/`gU`).
- **No register persistence, no uppercase/numbered/special registers.**
- **No sentence/paragraph/tag text objects** — delimiter and word objects
  only.

Each is a natural, small follow-on once v1's shapes (a real `Mode`, a
motion function, an operator, a register map) exist — the point of the cut
line is that v1 is a complete, coherent, *shippable* subset, not a stub.

## Rollout

A new `Settings::modal_engine: bool`, matching the boolean-toggle naming
convention already used (`relative_line_numbers`, `auto_pair`, …), default
**off** when T112 lands (a mode engine with no motions yet isn't useful on
its own) and flipped to on-by-default once T115 ships the whole v1 slice —
that flip is whoever ships T115's call, not fixed here. `vix-modal`
replaces `vim_normal_key`'s Normal-mode dispatch entirely once active for
Vi and Spacemacs alike (Spacemacs keeps delegating for its shared
vocabulary, just to the new engine instead of the old function); the `:`
line and `SPC`-leader are untouched either way. T115 also updates
`docs/for-vim-users/index.md`'s honest "where Vim still wins" section to
state exactly what v1 now covers — that page's current list ("no counts,
no text objects, …") is this spec's whole reason to exist, and it should
stop being true incrementally as T112–T115 land, not all at once at the
end.

## Planned crate shape (T112+)

Not binding — T112's call — but a starting shape:

- `mode.rs` — the `Mode` enum, the dispatch entry point host code calls per
  key, delegating to the pieces below.
- `motion.rs` — the pure motion functions (§ Design: motions).
- `operator.rs` — `d`/`c`/`y` composition over a motion/text-object/Visual
  range, plus `x`/`p`/`P`.
- `count.rs` — the numeric-prefix accumulator and the `count1 * count2`
  composition rule.
- `register.rs` — the unnamed (clipboard-mirrored) + named a–z register
  map.
- `text_object.rs` — the `i`/`a` + delimiter pure functions (§ Design: text
  objects).
- `repeat.rs` — the keystroke-replay dot-repeat mechanism (§ Design:
  dot-repeat).
- `lib.rs` — the public surface `src/app.rs` calls.

Unit tests drive motions/operators/text objects directly (pure functions,
no host needed) — "heavy unit tests" per T113, including a representative
operator×motion grid per T114 ("tests per operator×motion pair for a
representative grid"). Mode dispatch and dot-repeat need a small mock-host
harness, the same terminal-independent-testing principle as
`vix-script`/everywhere else (`spec/test/index.md`).
