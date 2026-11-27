# Modal editing (`vix-modal`)

A real modal-editing engine for the Vi/Spacemacs keymaps: modes, operator x
motion composition, counts, registers, text objects, dot-repeat. The Vi
keymap's Normal mode today is a **binding table**, not that engine: a
flat `match` of hardcoded key sequences in the App shell, not composable
motions an operator can apply to. `vix-modal` is the real engine — this
spec is the T111 audit (what exists today, precisely, so "the gap is real"
is a finding and not an assumption) plus the v1 design it justifies.

**Status**: v1 shipped in slices (improvement plan T112–T115, all done) and
`Settings::modal_engine` is now **on by default** for the Vi and Spacemacs
keymaps — each slice updated this file if reality and the design disagreed,
same as anywhere else, and still should as any of the gaps below close.
**T112** (done): the `Mode` enum, `Settings::modal_engine`, and host wiring
for Visual/Visual Line entry, exit, and cursor-extending movement. **T113**
(done): `count.rs`'s numeric-prefix accumulator and `motion.rs`'s pure
`h j k l w b e 0 ^ $ gg G { } ( ) f t F T` functions, wired into Normal
mode — one small, deliberate drift from § Design: motions' framing below:
`tasks.md`'s own T113 bullet lists `h j k l w b e 0 $ ^ gg G { } f/t/F/T`
without `( )` or `%`; T113 implemented `( )` anyway (near-zero extra cost
once `{`/`}` existed) but left `%` for a small follow-on (a different kind
of scan — delimiter matching, not char/word/line position — and the
existing `edit.match_bracket` action already covers it). **T114** (done):
`operator.rs` (`operator_range`/`delete_range`/`insert_at`/`paste_plan`,
all pure — no buffer mutation, the host applies the result through its own
selection + `InsertText` action) and `register.rs` (`Registers`, the named
`a`-`z` map). `d`/`c`/`y` compose with every T113 motion, `x` is real sugar
for `d` + one right motion, `dd`/`cc`/`yy` act on the whole line, `p`/`P`
read a register, `"{a-z}` selects one. One deliberately deferred real-Vim
nuance, not in the spec's own cut list: `cc`'s indentation preservation
(`cw`'s "acts like `ce`" special case landed in T837). Operators composing
with a **Visual** selection landed later, in T840 (§ Design: Visual
operators). **T115** (done):
`text_object.rs` — `iw`/`aw`, a parameterized `inner_pair`/`around_pair`
(`(`/`)`/`b`, `{`/`}`/`B`, `[`/`]`, `<`/`>`) and `inner_quote`/
`around_quote` (`"`/`'`/`` ` ``), composing with `d`/`c`/`y` exactly like a
motion (always a plain character-wise range). Dot-repeat: `.` replays the
last real change (`d{motion}`, a text object, or `p`/`P` — not `y`,
matching real Vim's own rule) as host-side keystroke replay
(`tui/src/app/modal.rs`, not this crate — there's no pure function to write for
it). **T837** (done): `c` and the Insert-mode session it opens, and a plain
Insert-mode session (`i`, `a`, `o`, …), are recorded too and each change
is one undo step (§ Design: dot-repeat); `cw` keeps the white space after
the word (`operator::change_word_end`). `cc`'s indentation preservation
remains deliberately unimplemented. None of the open items above are in this
spec's own cut list; they're documented gaps, not silent omissions —
`docs/vix-for-vim-users/index.md`'s "Known differences, in short" states the
same list in user-facing terms, and § Vim-parity table (T844) measures it.

## The audit

Everything today lives in `tui/src/app.rs`, as flat match-based methods on
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
by the project's own `docs/vix-for-vim-users/index.md`: "no counts (`3w`)").

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
{ }              — the empty line before / after a paragraph (Vim's rule, T846)
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

**Buffer access and cost (T843).** The motions are generic over
`vix_modal::text::Text` — a character count plus a forward and a backward
character iterator from any offset — implemented for `str`, `String`,
`ropey::Rope` and `ropey::RopeSlice`. The `&str` shape above still works
unchanged; the host passes the editor's rope (`Code::rope`, cloned in O(1))
so no motion or operator copies the buffer. Each motion walks outward from
`pos` only as far as its answer needs: `h l 0 ^ $ f t F T` read the cursor's
line, `w b e` read up to the next word boundary, `j k {count}gg { }` read
the lines they cross, `G` reads the last line, and a linewise operator reads
the two lines at its ends. A same-line motion therefore costs the same in an
8 MiB buffer as in a one-line one (`docs/performance/index.md`, *Vi motions
in a large file*). `(`/`)` (since T830) read back from the cursor over the
whitespace and closing marks just before it, then on to the sentence start
they land on (`vix_textops::sentence_start_from` /
`sentence_start_before`), so a sentence motion reads the sentences it
crosses, not the buffer; in text with no sentence end (code, say) that can
still be up to a buffer end, but without a copy. Text objects, `p`/`P`
and Visual operators still take a copy of the buffer — follow-ons, not part
of T843. Results are exactly those of the
original whole-text implementation, which the crate's tests keep as an
oracle and compare on every offset of thousands of generated texts, and on
proptest-generated texts over a wider alphabet (upper case, digits, wide,
zero-width and combining characters, an emoji, non-breaking and Unicode
line/paragraph separators, form feed; T831).

*Offsets* (RCA figure declaration): `pos` and every motion result are
character offsets, `usize`, valid range `0..=len_chars`. A motion clamps by
design (to the line's last character, the line start, the buffer end or 0),
so an offset sum that would overflow `usize` (a count near `usize::MAX`)
lands on that same clamp bound, and a difference below zero lands on 0, the
start; both are the answer Vim gives for an overlong count, not invalid
input. A huge count costs no more than reaching the bound: a repeated step
stops as soon as it stops moving.

**The Vi keymap's own rules (T838).** The Vi and Spacemacs keymaps' Normal
mode reads a separate set of motions (`motion::vi_*`, `display_column`,
`line_down_to_column`, `line_up_to_column`), so the shared scanner above
keeps its behavior for Helix (`w`/`b`/`e` selections) and every other
caller:

- **Graphemes.** A position is a grapheme start, computed over the current
  line only (`unicode-segmentation`): `h`/`l` step over a letter and its
  combining marks together, and a Normal-mode cursor never rests inside a
  grapheme or on a line's end (`vi_normal_cursor`, Vim's `adjust_cursor`):
  every Normal-mode motion result passes through it. An operator's range may
  still end at a line's end.
- **Word classes.** `w`/`b`/`e` follow Vim's `fwd_word`/`bck_word`/
  `end_word` step for step over three classes: blank (white space,
  including `\r`, and the line break), word (alphanumeric or `_`) and
  punctuation (everything else). A run of one class is a word and an empty
  line is a word, so `pkg::run(arg);` is `pkg`, `::`, `run`, `(`,
  `arg`, `);`. Simplification (decided here): Vim gives CJK scripts and
  emoji classes of their own; here they are word or punctuation characters
  by `char::is_alphanumeric`. Past the last word, `w` rests on the text's
  last character, as Vim's does.
- **`w` under an operator** (`vi_word_forward_for_operator`): when the
  last word moved over ends its line, the range stops at that line's end
  (`:help w`), so `dw` on a line's last word keeps the line break.
- **Preferred column.** `j`/`k` carry Vim's `curswant`: the display
  column (`display_column`: tabs to the next multiple of the tab width,
  wide characters two cells, a cursor on a tab at the tab's last cell) of
  where a run of vertical motions started, or "each line's end" after `$`.
  The target line's grapheme covering that column is chosen (a tab or wide
  character spanning it included), its last grapheme when it is shorter, so
  `4ljj` over a short line comes back to column 4. The pure functions take
  the column and the tab width; the host keeps the column while the cursor
  and buffer length are where the last `j`/`k`/`$` left them, and passes
  a tab width of 1 because the editor draws a tab as one cell (decided
  here: the column the user sees, Vim's own definition).

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

**Cursor destination vs. operator range (T838).** A motion used as an
operator's target may go where a Normal-mode cursor may not: `l` under an
operator (and so `x`) reaches the line's end (`motion::vi_chars_end`:
`count` graphemes, never the line break), so `$x` and `9x` delete the
last character and `x` takes a letter with its combining marks. The Vi
range (`operator::vi_operator_range`) adds Vim's rules to the half-open
one: an inclusive motion takes its whole last grapheme and nothing on a
line's end (`d$` on an empty line), and an exclusive motion ending at the
start of a later line ends at the line before's end instead, becoming
line-wise when it started in its line's indent (`:help exclusive`; `dw`
on an empty line deletes the line). A character-wise `d`/`y` over an
empty range changes nothing and leaves the register alone (`x` on an empty
line); after a character-wise `d` the cursor moves back onto the line's
new last character when the delete reached its end. `cw` keeps the white
space after the word (T837, `operator::change_word_end`).

`p`/`P` are not operators — they're Normal-mode commands that read the
active register and insert its content after/before the cursor (or replace
the Visual selection, in Visual mode). Line-wise vs. character-wise paste
follows how the register was written (`yy`/`dd` produce a line-wise
register; `yw`/`dw` produce a character-wise one) — the same distinction
real Vim makes, since it's what makes `p` "paste this as its own line vs.
paste this inline" behave correctly without the user thinking about it.

A count before `p`/`P` (`3p`, `"a3p`, T840) inserts that many copies as one
insertion, so one undo removes them all (`operator::paste_plan_counted`).
A put whose copies would exceed `operator::PUT_LIMIT_BYTES` (64 MiB) is
refused with a warning and the buffer is left unchanged, as Vim refuses it
with E1240: a count saturates rather than overflowing, so a long digit run
must not try to allocate the result.
A character-wise put lands on the last inserted character, a line-wise one
on the first pasted line's first non-blank, as in Vim.

A line-wise change (`cc`, Visual Line `c`) puts the whole lines in the
register but keeps the final line break, leaving one empty line to type
into, as Vim does (indentation is still not preserved).

### Design: Visual operators (T840)

In Visual and Visual Line mode `d`, `x` (the same as `d`), `c` and `y`
apply to the selection through the same host path a motion or text object
uses, then return to Normal mode (`c` on into Insert). `"{a-z}` typed
inside Visual mode selects the register, as does one typed before `v`/`V`.

- **Range.** Vim's Visual selection is inclusive: the anchor character and
  the cursor character both belong to it, whichever way it was extended.
  The host editor's selection is half-open and ends just before the cursor
  (forward) or the anchor (backward), so in both directions the operator
  range is the host selection plus one character, clamped to the buffer
  (`operator::visual_range`). Right after `v` the selection is the one
  character under the cursor. A selection made by a Visual text object
  (`viw`, `va(`) is already exact and is used as is while it is unchanged.
  Visual Line covers every line the selection touches, whole.
- **Register kind.** Character-wise Visual writes a character-wise register,
  Visual Line a line-wise one, so `p` afterward pastes inline or as lines.
- **Cursor.** `d`/`c`/`y` leave the cursor at the start of the range (a
  line-wise `d`/`y`: its first non-blank), as in Vim.
- **Not yet.** A Visual operator is not dot-repeatable: a Visual `d` or `c`
  clears what `.` replays (T837) rather than leave an older change to be
  repeated in its place (a Visual `y` changes nothing and leaves it), and a
  Visual `c` with the text typed after it is still one undo step. Visual mode's motions are still `h j k l` and the
  arrows (plus text objects), and `p`/`P` over a selection does not
  replace it. Visual Block and register macros are later work.

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

### Changes through Insert mode (T837)

- **Recording.** A Normal-mode command that enters Insert mode (`c{motion}`,
  `c` + a text object, `cc`, `i`, `a`, `I`, `A`, `o`, `O`, with any count
  or register prefix) starts a record holding its own keys; every key typed
  in Insert mode is added to it — text (any Unicode), `Enter`, `Tab`,
  `Backspace` and `Delete`, so a corrected typo replays as typed — and the
  `Esc` that leaves Insert mode makes the whole record what `.` replays.
  An Insert session that changed nothing (`i` then `Esc`) leaves the last
  change as it was.
- **Replay.** `.` re-runs the Normal-mode keys where the cursor now is, so
  a text object or motion is recomputed (`ciw` on a longer word changes
  that whole word), then — only if they entered Insert mode again — the
  typed keys and `Esc`. `{count}.` replaces the count among the
  Normal-mode keys (none recorded: the count goes first, after a register
  prefix), never a digit typed in Insert mode.
- **Undo.** On `Esc`, every edit since the command began — the deletion
  `c` made, the line `o` opened, each typed character — folds into one
  undo step (`History::squash_since` in `vix-editor-core`), so one `u`
  undoes the change and an earlier change stays its own step.
- **Invalidation.** When a change cannot be recorded, `.` is cleared
  rather than left to repeat an older change: an Insert session that used a
  cursor key, a function key or a modified key (Vim also starts a new undo
  step and a new insert there; the session then stays separate undo steps
  too), one in which something other than typing edited the buffer (a
  completion popup, ghost text, an Org table key), and a Visual `d`/`c`.
  A cancelled operator (`d` then `Esc`) changes nothing and is not a
  change.
- **`cw`.** On a non-blank, `cw` changes to the end of the word and keeps
  the white space after it (`:help cw`): `cw` on `old value` leaves
  ` value` (`operator::change_word_end`). On white space it is `dw`.

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

## Tree-sitter textobjects and motions (T616)

On top of v1's delimiter objects, the engine binds the syntax-aware
textobjects from `crates/vix-editor-core/spec/textobjects/index.md`, with
the letters nvim-treesitter-textobjects configurations conventionally use:

| Keys | Object / motion |
| --- | --- |
| `if` / `af` | the function around the cursor: its body / the whole function |
| `ic` / `ac` | the class (struct, enum, trait, impl, interface, …) around the cursor |
| `ia` / `aa` | the parameter or argument under the cursor / with its separating comma |
| `]f` / `[f` (also `]m` / `[m`) | to the start of the next / previous function |
| `]c` / `[c` | to the start of the next / previous class |
| `]a` / `[a` | to the start of the next / previous parameter |

- After an operator (`daf`, `cif`, `yac`, `d]f`, …) they compose exactly like
  the delimiter objects and motions: an object is a character-wise range, a
  `]`/`[` motion is exclusive like `w`.
- **Counts.** `2daf` acts on the second enclosing function (outward from the
  innermost); `3]f` jumps three functions ahead. A count before the operator
  and one after multiply, as everywhere else.
- **Visual mode** gains `i`/`a` + any object (this one and v1's — `viw`,
  `vi(` work too). For the Tree-sitter objects, repeating grows outward:
  after `vaf`, a second `af` selects the function enclosing that one.
- The motions wrap at the buffer's ends, like the Go menu's commands.
- These are not pure functions of `text: &str` like the rest of this crate:
  they need the parse tree, so the host (`tui/src/app/modal.rs`) asks
  `vix-editor-core` for the range and feeds it to the same operator path.
  Vim's own `]]`/`[[` section motions and `]c` (next diff hunk, in diff
  mode) are not bound; `]c` means class here, as in those configurations.

## Helix selections

The Helix keymap (`crates/vix-keymap-model/spec/index.md`, § Helix modes,
selections and leader) is selection-first: a motion selects, then an action
acts on the selection. It is not a second engine: the host
(`tui/src/app/helix.rs`) reuses this crate's `Mode` (Select mode is
`Mode::Visual`), `motion`'s `w`/`b`/`e`/`f`/`t`/`F`/`T` functions, and
`register`'s kinds, and keeps the selection in the editor's own
`[anchor, cursor)` selection. The `helix` module adds the pure pieces that
model needs, all over **character offsets** like everything else here:

- `effective(text, sel, cursor)` — the range an action acts on: the
  selection, or the one character under the cursor when nothing is
  selected (Helix never acts on nothing).
- `select_lines(text, start, end)` — `x`: every line the range touches,
  newline included; a range that already is exactly that grows by the next
  line, so `x x x` selects three lines.
- `long_word_forward` / `long_word_backward` / `long_word_end` — `W`/`B`/`E`
  over `WORD`s (runs of non-whitespace), the `w`/`b`/`e` shape.
- `swap_case` (`~`) and `replace_chars` (`r<char>`, line breaks kept).
- `paste(text, start, end, clip, before)` — `p`/`P`: text ending in a
  newline pastes as whole lines below the last selected line (or above the
  first), anything else right after the range end (or before its start);
  the result names the range to select afterward, the pasted text.
- `select_by(text, (anchor, cursor), Select { extend, inclusive, count },
  f)` — one selection after a selecting motion `f` (`w`, `e`, `f<char>`,
  …): run `count` times, each step selecting afresh where the last ended,
  the anchor kept in Select mode.
- `insert_point(text, (anchor, cursor), after)` — where `i`/`a` start
  typing for one selection.
- `join_selections(pieces)` / `split_selections(clip, n)` — several
  selections' texts as one register value (each piece followed by a
  newline unless it already ends with one) and back, one line per
  selection, for a paste into the same number of selections.

**Several selections.** Every helper takes one selection, so the host
applies it to each of the editor's carets: motions through
`Editor::map_carets`, edits as one `SelectionEdit` per caret through
`Editor::edit_selections` (one undo step), and `>`/`<` through
`Editor::indent_carets` (`crates/vix-editor-core/spec/index.md`, §
Multiple carets).

Counts and `.` reuse this crate too, host-side. A count accumulates in the
same `Count` the Vi keymap uses (`1`-`9` start it, `0` continues it) and
the next command reads it: the host runs a word motion that many times,
each step selecting afresh from where the last ended (`3w` selects the
third word), `select_lines` that many times (`2x`), and passes it straight
to `motion`'s `f`/`t`/`F`/`T` functions. `.` is **semantic**, unlike the Vi
keymap's keystroke replay (§ Design: dot-repeat): Helix acts on whatever
is selected when `.` is pressed, so the host records *what* the last change
was (delete, change with its typed text, insert with its typed text,
replace with a char, switch case, indent, outdent, paste with its count)
and applies it again with these same helpers.

## Vim-parity table (T844)

`tui/tests/integration/vim_parity.rs` holds a table of key sequences, each
with a buffer before and after (cursor marked `|`), the mode afterwards and,
for an operator or a put, the named register `a`'s text and kind. It is the
source of truth for every parity claim in `docs/vix-for-vim-users/index.md`,
whose table is generated from it (the test fails when the guide is stale;
`VIX_UPDATE_VIM_PARITY=1` rewrites it).

- **Kinds.** Each row is *stock Vim* (the keys mean the same in an
  unconfigured Vim), *optional Vix* (no stock Vim key does this: the
  Tree-sitter objects) or *external tool* (the result comes from another
  program: `:cn` over a task's problems).
- **Vix, always.** `every_row_does_what_the_table_says_in_vix` types every
  row into a fresh Vi-keymap `App` and compares text, cursor, mode
  (`App::vi_mode`) and register (`App::vi_register`). It runs in the normal
  test suite on every CI run.
- **Vim, in its own job.** `stock_rows_match_an_unconfigured_vim` (ignored by
  default; CI's `vim-parity` job, `spec/ci/index.md`) types every stock row
  into `vim -u NONE -i NONE -N` (`$VIX_VIM`, else `vim`), reading the state
  back through a `<Cmd>` snapshot so Insert and Visual mode survive, and
  compares it with what the row records for Vim.
- **Differences are rows, not fixes.** Where Vix differs, the row asserts
  Vix's current behavior, records Vim's result and says why (a gap in this
  spec's cut list, or a parity bug). Closing a gap means changing the row to
  "same as Vim" with the fix.
- **Decisions.** "Unconfigured Vim" means `-u NONE` with `nocompatible`
  (`-N`), so options such as `autoindent` are off in both. Rows put deletions
  and yanks in register `a`: Vix's unnamed register is the process-wide
  clipboard, which parallel tests would race on. The unnamed register is
  therefore not compared.

Parity bugs the table found (T844) were recorded as rows; T846 fixed them and
flipped the rows to Vim's result (each checked against `vim -u NONE`):

- **Modified keys never type.** In Vi/Spacemacs Normal (and Visual) mode a
  Ctrl- or Alt-held character the keymap does not bind is dropped before the
  editor widget, which would type it as text (`Ctrl-R` typed an `r`). The
  widget's own Ctrl chords (undo/redo, copy, paste, cut, delete line,
  duplicate/add occurrence, select all) still run.
- **`u`, `Ctrl-R`, `g-`, `g+`.** `u` undoes and `Ctrl-R` redoes along the
  current branch of the undo tree, each with a count. `g-`/`g+` step back and
  forward in time: to the live undo-tree state with the next lower (higher)
  `seq` — creation order — crossing branches, as Vim's chronological undo
  does. After any of them the cursor goes to the start of the text the last
  step changed (the lowest offset its edits touched), on a character as
  Normal mode wants. Decision: Vim restores the cursor saved with the undo
  entry when it is on the changed line; Vix's entries save the cursor after
  the operator set up its selection, so "the change's start" is the rule
  instead (the same result for `x`, `dw`, `ciw`, …).
- **`D` `C` `X` `s`** are `d$` `c$` `dh` `cl`, through the operator path
  (registers, counts and `.` included). **`r{char}`** replaces `count`
  characters, leaving the cursor on the last; with fewer than `count` left on
  the line it does nothing, as in Vim. **`r<CR>`** (T847) splits the line:
  the `count` characters become one line break (Vim's rule, not `count`
  breaks), with no indent (an unconfigured Vim has `noautoindent`), and the
  cursor goes to the start of the new line; any other non-character key
  cancels `r`. **`J`** joins `count`
  lines (at least two), Vim's way: leading white space of each joined line
  goes, and one space separates it unless the text so far is empty or ends
  in a space or tab, or the joined line is empty or starts with `)`; the
  cursor lands on the last join. Both are one undo step and repeat with `.`.
- **`<Esc>` from Insert mode** steps the cursor back one character, never past
  the line's start (also when `.` replays an Insert session).
- **`V`** selects the cursor's line and leaves the cursor where it is; `h j k
  l` move the cursor as Normal mode does (`j`/`k` keep the display column)
  and the selection is every line from the anchor line (the selection's other
  end) to the cursor's.
- **`}` / `{`** use Vim's paragraph search: from the cursor's
  line, past any empty lines, past at least one non-empty line, to the next
  empty line. An empty line is the boundary; a line of only white space is
  not (Vim's rule; Vix's Go → Paragraph keeps its own blank-line rule).
  Reaching the buffer's first or last line stops there (`}` then lands on the
  last line's last character, through it for an operator); a count that
  runs out of buffer before its last run fails and nothing moves.
- **`a"` `a'` `` a` ``** take the spaces and tabs after the closing quote, or,
  when there are none, those before the opening one (`:help a"`).
- **`/pattern<CR>`** lands on the first match after where `/` was typed
  (whatever the find bar's live preview moved through), closes the find bar
  and returns the keyboard to the buffer in Normal mode; `n`/`N` repeat it.
  Other keymaps' find bar is unchanged (`<CR>` finds the next match and the
  bar stays open).

The runnable migration exercise in the same guide (`tui/examples/vim-exercise`,
a tiny Rust crate) is walked step by step, with the real `cargo` and `git`,
by `tui/tests/integration/vim_exercise.rs`.

## v1 cut line, summarized

Explicit, per `tasks.md` T111 ("no ex commands, no macros — Vix already has
macros") plus the per-section cuts above, gathered in one place:

- **No ex-command scripting** (`:%s/…/…/g`, `:g/pattern/…`) — the existing
  simple `:` commands (`:w`, `:q`, `:42`, `:e path`, …) are unrelated to
  `vix-modal` and unchanged.
- **No macro-via-`q`/`@`** — Vix's existing Edit-menu macro recorder is a
  separate, already-shipped feature; not being duplicated or replaced.
- **No Visual Block.** Character and line-wise Visual only.
- **No WORD motions** (`W`/`B`/`E`) in the Vi keymap, no `;`/`,`, no
  `*`/`#`. (The Helix keymap's `W`/`B`/`E` use `helix::long_word_*`, § Helix
  selections; Vi doesn't bind them.)
- **No operators beyond `d c y (x) p P`** and the T846 shorthands `D C X s r
  J` (no `>`/`<`/`~`/`gu`/`gU`).
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
`docs/vix-for-vim-users/index.md`'s honest "where Vim still wins" section to
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
- `lib.rs` — the public surface `tui/src/app.rs` calls.

Unit tests drive motions/operators/text objects directly (pure functions,
no host needed) — "heavy unit tests" per T113, including a representative
operator×motion grid per T114 ("tests per operator×motion pair for a
representative grid"). Mode dispatch and dot-repeat need a small mock-host
harness, the same terminal-independent-testing principle as
`vix-script`/everywhere else (`spec/test/index.md`).
