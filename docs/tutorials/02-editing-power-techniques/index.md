# Tutorial 2: Editing Power Techniques

Basic typing gets text in. This tutorial covers the techniques that make Vix
fast once you're past that: keyboard macros, snippets, text transforms
(case, align, sort/dedupe, increment, transpose, wrap), surround, and
rectangular (column) selection.

This tutorial does **not** cover multiple simultaneous cursors or find &
replace — those are
[Tutorial 3: Find & Replace and Multi-Cursor](../03-find-replace-and-multi-cursor/index.md)'s
subject. Where a technique below overlaps with multi-cursor editing (column
select does, under the hood), this page only goes as far as using it.

## Setup

Open the demo workspace at the repo root and use it for every example below:

```sh
vix examples/demo-workspace
```

See `examples/demo-workspace/README.md` for what's in it. This tutorial uses
`examples/demo-workspace/rust-app/src/main.rs` (from here on, just
`main.rs`), `scripts/hello.py`, and `data/sample.csv`.

## Keyboard macros

A macro is a recorded sequence of editor keys you can replay anywhere the
cursor happens to be — good for a small edit you need to repeat a few times
in slightly different spots, where find & replace doesn't apply because the
text around each spot differs.

Open `main.rs` and look at its last two lines of comments:

```rust
    // TODO: report the total word count too, not just the per-word breakdown.
    // FIXME: word_counts lowercases nothing, so "The" and "the" count separately.
```

Tag both with a reviewer note:

1. Click right before `TODO` (just after `// `).
2. **Edit → Macro → Record** (`macro.record`) — starts recording.
3. Type `REVIEW: `.
4. **Edit → Macro → Record** again — stops recording.
5. Click right before `FIXME` on the next line.
6. **Edit → Macro → Play** (`macro.play`) — replays the typed text at the new
   cursor position.

Both lines now read `// REVIEW: TODO: …` and `// REVIEW: FIXME: …`. Since
playback is just "the same keys, wherever the cursor is now," it works
equally well if you next jump to `scripts/hello.py`'s own `TODO`/`FIXME`
pair and play it there too.

To keep a macro past this session, **Edit → Macro → Save…** (`macro.save`)
prompts for a name and writes it to `macros.toml` in your config directory;
**Edit → Macro → Play Saved…** (`macro.play_saved`) opens a chooser of
everything you've saved. Saved macros are plain TOML you can hand-edit:

```toml
[[macro]]
name = "review-tag"
keys = ["R", "E", "V", "I", "E", "W", ":", "Space"]
```

Full reference (the key-token format, storage details): [`docs/macros/index.md`](../../macros/index.md).
Spec: `crates/vix-macros/spec/index.md`.

## Snippets

A snippet is reusable text with **tabstops** — fields you Tab between,
inserted either from a searchable picker or by typing a short prefix.

### Turn existing code into a snippet

`main.rs` has a line worth reusing:

```rust
println!("{word}: {count}");
```

1. Select that line.
2. **Tools → New Snippet from Selection…** (`tools.snippet_new_from_selection`).
3. Type a prefix at the prompt, e.g. `pln` — this becomes both the snippet's
   name and its expansion prefix.

The selection is now saved to your global snippets file
(`~/.config/vix/global/snippets/snippets.json`). Anywhere afterward — in this
file, in `scripts/hello.py`, anywhere — typing `pln` then **Tab** expands
back to `println!("{word}: {count}");` with the cursor ready to edit it.
(With nothing selected, **New Snippet from Selection…** is a no-op — there's
a status message rather than a prompt.)

### The picker

**Tools → Snippets…** (`tools.snippets`) lists every snippet in scope for
the active buffer's media type — type to filter by name, prefix, or
description; **Enter** inserts it. This is the same list `pln`-then-Tab
draws from, just browsable instead of typed from memory.

### Tabstops

A snippet body can define fields visited in order on **Tab**, e.g.
`${1:name}` (pre-filled and selected) or `$1` (empty), ending at `$0`. Vix's
own bundled and project snippet files use this syntax; write your own in
`<project>/config/snippets/snippets.json` or per-media-type under
`~/.config/vix/media-types/<type>/<subtype>/snippets/`.

Full reference (file locations, scope merging, the complete tabstop syntax):
[`docs/snippets/index.md`](../../snippets/index.md). Specs:
`crates/vix-snippets/spec/index.md`, `crates/vix-snippet-tool/spec/index.md`.

## Text transforms

Vix has a set of small, focused text transforms — most live under **Edit**,
case conversion lives under **Tools → Convert**. None of these has a default
keybinding in any keymap; reach them from their menu or the command palette
(`Ctrl+P`, then `>` to search by command name).

### Case conversion

**Tools → Convert → Case** turns the selection into one of seven cases.
Select the identifier `word_counts` in `main.rs` and try a few:

| Menu item | Action ID | `word_counts` becomes |
| --- | --- | --- |
| Upper (FOO BAR) | `edit.case_upper` | `WORD_COUNTS` |
| Lower (foo bar) | `edit.case_lower` | `word_counts` |
| Title (Foo Bar) | `edit.case_title` | `Word_Counts` |
| Kebab (foo-bar) | `edit.case_kebab` | `word-counts` |
| Snake (foo_bar) | `edit.case_snake` | `word_counts` |
| Camel (fooBar) | `edit.case_camel` | `wordCounts` |
| Pascal (FooBar) | `edit.case_pascal` | `WordCounts` |

Undo (`Ctrl+Z`) before saving — this is a demo, not a rename.

### Align on a delimiter

**Edit → Align** pads selected lines so each one's **first** occurrence of a
delimiter lands in a common column. Open `data/sample.csv` and select the
four data rows (not the header):

```
Alice,engineer,92
Bob,designer,81
Carol,engineer,88
Dave,manager,75
```

Choose **Edit → Align → On , (comma)** (`edit.align.comma`):

```
Alice , engineer,92
Bob   , designer,81
Carol , engineer,88
Dave  , manager,75
```

Only the first comma on each line moved — everything after it is untouched,
which is the point: align is for tidying the one column you're looking at,
not for reformatting a whole table (for that, see the CSV/TSV table editor
at **Edit → Mode → Table**, `tools.edit_table`). `edit.align.equals` / `.colon`
/ `.pipe` work the same way for `=`, `:`, and `|`.

Full reference: [`docs/align/index.md`](../../align/index.md). Spec:
`crates/vix-align/spec/index.md`.

### Sort, dedupe, and squeeze blank lines

**Edit → Lines** has a run of whole-line operations, applied to the
selection or (with nothing selected) the whole buffer:

| Menu item | Action ID | Does |
| --- | --- | --- |
| Sort | `edit.sort_lines` | Ascending, byte order |
| Sort Unique | `edit.sort_unique` | Sort, then drop duplicates |
| Shuffle | `edit.shuffle` | Random reorder |
| Reverse | `edit.reverse_lines` | Flip line order |
| Remove Duplicate Lines | `edit.remove_duplicate_lines` | Keep first occurrence of each |
| Squeeze Blank Lines | `edit.squeeze_blank_lines` | Collapse runs of blank lines to one |
| Trim Trailing Whitespace | `edit.trim_trailing_whitespace` | Strip trailing spaces per line |

Select the same four `data/sample.csv` rows again and try **Reverse** — Dave
moves to the top. **Sort** puts Alice back on top (the rows happen to
already be alphabetical by name, so **Sort** is a no-op there — a good
reminder that it sorts on the *whole line*, not a column; for column-aware
sorting use the table editor). **Sort Unique** and **Remove Duplicate Lines**
matter once there's a duplicate — try them on a throwaway selection like:

```
engineer
designer
engineer
manager
```

**Sort Unique** → `designer` / `engineer` / `manager`. **Remove Duplicate
Lines** (which doesn't also sort) → `engineer` / `designer` / `manager`,
i.e. the second `engineer` dropped, order otherwise kept.

### Increment and decrement numbers

**Edit → Increment Number** (`edit.increment_number`) and **Decrement
Number** (`edit.decrement_number`) bump the next integer at or after the
cursor, on the current line, leaving the cursor on it — no selection needed.
Put the cursor anywhere before `92` on `data/sample.csv`'s `Alice` row and
run **Increment Number** three times: `92` → `95`.

### Transpose

**Edit → Transpose** swaps the text unit under the cursor with the one
before it, for six granularities: Chars, Words, Lines, Sentences,
Paragraphs, Sections (`edit.transpose_chars` / `_words` / `_lines` /
`_sentences` / `_paragraphs` / `_sections`). Put the cursor in `lowercases`
on `main.rs`'s FIXME line and run **Edit → Transpose → Words**:

```
// FIXME: word_counts lowercases nothing, …
```
becomes
```
// FIXME: lowercases word_counts nothing, …
```

### Smart toggle

**Toggle Value**, directly under **Edit** (`edit.toggle_value`, no
submenu), flips the boolean-ish token at the cursor to its opposite:
`true`/`false`, `yes`/`no`, `&&`/`||`, `==`/`!=`, case preserved. Type a
scratch line `let ok = true;`, put the cursor in `true`, and run it:
`let ok = false;`. It's a no-op (with a status note) when there's nothing
togglable under the cursor.

### Wrap paragraph

**Edit → Wrap** (`edit.wrap`) hard-wraps text at the `wrap_column` setting
(80 by default — see [`docs/configuration/index.md`](../../configuration/index.md)).
With a selection, it wraps just that text; with none, it wraps the paragraph
around the cursor — the run of non-blank lines holding it — and keeps each
line's indentation, shared comment marker, and any hanging bullet indent.
Put the cursor anywhere in `scripts/hello.py`'s module docstring and run it
to see the two lines re-flowed to the current wrap width; it's a no-op (with
a status note) if the paragraph is already wrapped or the cursor isn't in
one.

Everything above lives in `crates/vix-textops/spec/index.md`, applied by
`App::transform_selection_or_buffer` (whole selection/buffer) or
`App::rewrite_at_cursor` (cursor-relative).

## Surround

**Edit → Surround** wraps the selection in a bracket or quote pair — or
strips that pair if the selection is already wrapped, since each action
toggles:

| Menu item | Action ID | Pair |
| --- | --- | --- |
| ( Parentheses ) | `edit.surround.paren` | `(` `)` |
| [ Brackets ] | `edit.surround.bracket` | `[` `]` |
| { Braces } | `edit.surround.brace` | `{` `}` |
| < Angles > | `edit.surround.angle` | `<` `>` |
| " Double Quotes " | `edit.surround.double_quote` | `"` `"` |
| ' Single Quotes ' | `edit.surround.single_quote` | `'` `'` |
| \` Backticks \` | `edit.surround.backtick` | `` ` `` … `` ` `` |

Try it on `main.rs`: select the whole string literal
`"the quick brown fox jumps over the lazy dog the fox runs"` and run
**Edit → Surround → ( Parentheses )**: it gains an outer `(…)`. Run the same
action again on the still-selected text and the parentheses come back off.
With nothing selected, Surround still runs: it inserts the empty pair at the
cursor with the cursor left between the two halves, ready to type. Undo
(`Ctrl+Z`) afterward — wrapping that particular literal in parentheses
doesn't compile.

This is editor wiring around the pure `vix-affix` crate (`add`/`drop`/`toggle`
over a prefix/suffix pair) — full reference:
[`docs/affix/index.md`](../../affix/index.md). Spec:
`crates/vix-affix/spec/index.md`.

## Column (rectangular) selection

**Edit → Select → Column Select Up** / **Column Select Down**
(`edit.column_select_up` / `edit.column_select_down`) extend a *rectangular*
selection one line at a time: a caret is added on the next line at the same
column as the current selection's edge (clamped to that line's length), and
every resulting caret edits together — type once and it types at every
caret in the block simultaneously.

Try it on the two comment lines in `main.rs`:

```rust
    // TODO: report the total word count too, not just the per-word breakdown.
    // FIXME: word_counts lowercases nothing, so "The" and "the" count separately.
```

1. Click right before `TODO` (same spot as the macro example above), with no
   selection.
2. **Edit → Select → Column Select Down** — adds a second caret on the next
   line, at the same column: right before `FIXME`.
3. Type `NOTE: `.

Both lines gain the text at once:

```rust
    // NOTE: TODO: report the total word count too, not just the per-word breakdown.
    // NOTE: FIXME: word_counts lowercases nothing, so "The" and "the" count separately.
```

Under the hood, column select is one way of building a set of multiple
cursors — the general mechanism, including adding carets freely and matching
by selection, is
[Tutorial 3: Find & Replace and Multi-Cursor](../03-find-replace-and-multi-cursor/index.md)'s
subject; this page only covers the up/down rectangle shown here.

---

**Previous:** [Tutorial 1 — Your First Session](../01-your-first-session/index.md)
**Next:** [Tutorial 3 — Find & Replace and Multi-Cursor](../03-find-replace-and-multi-cursor/index.md)

---

Vix™ and Vix IDE™ are trademarks.
