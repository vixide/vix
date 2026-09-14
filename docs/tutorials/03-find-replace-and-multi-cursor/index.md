# Tutorial 3: Find & Replace and Multi-Cursor

Tutorial 2 covered single-cursor editing moves. This one covers the two
techniques that scale that editing up: finding and replacing text — in one
buffer, or across an entire project — and driving several cursors at once so
one keystroke changes several places at the same time.

Everything below runs against the demo workspace. If you don't have it open
already:

```sh
vix examples/demo-workspace
```

The terse reference versions of this material live at
[`docs/find/index.md`](../../find/index.md) and
[`docs/multiple-cursors/index.md`](../../multiple-cursors/index.md) — this
page walks through the same features with worked examples against real
files, and links back to those pages for the full option tables.

## 1. The Find bar

Open `examples/demo-workspace/rust-app/src/main.rs` (from here on, just
`main.rs`) and press `Ctrl+F` (**Edit → Find → Find…**).
Type `counts` — matches highlight as you type. `Enter` (or `F3`) jumps to the
next match, `Shift+F3` to the previous one, and `Esc` closes the bar without
losing your place.

Four toggle buttons change how the query matches, each a click or an `Alt`
key:

| Toggle | Key | Effect |
| --- | --- | --- |
| Case | `Alt+C` | Match case exactly |
| Smart case | `Alt+S` | Case-insensitive, until the query has an uppercase letter |
| Word | `Alt+W` | Whole words only |
| Regex | `Alt+R` | Treat the query as a regular expression |

Try **Word**: with the bar still open, clear the query and type `word`, then
press `Alt+W`. The standalone `word`s scattered through the file (the loop
variable in `for word in text.split_whitespace()`, the `{word}` in the
`println!`, the one in the TODO comment, …) light up, but the two `word`s
that are only the first half of `word_counts` — the function's name — don't:
a whole-word match needs a non-word character on both sides, and there
`word` is immediately followed by `_`, itself a word character. Turn
`Alt+W` back off and the same query also lights up both of those.

## 2. Replace and Replace All

`Ctrl+R` opens the same bar with **Replace** already on, and the cursor in
the new Replace field below the query — the same thing as pressing `Alt+H`
(**Replace**) in an open Find bar. `Tab` (or clicking a row) moves between
Find and Replace. Three buttons appear once Replace is on:

- **Once** — replace only the current match.
- **Ask** — replace with a confirmation prompt per match (the same
  interactive flow as `Ctrl+Alt+R`, see below).
- **All** — replace every match at once. `Alt+Enter` from either field does
  the same thing.

Regex mode adds capture groups to the replacement: `$1`, `$2`, or `${name}`
for named groups, plus the escapes `\n`, `\t`, `\r`, `\\`. Try it on tabular
data — open `data/sample.csv`:

```
name,role,score
Alice,engineer,92
Bob,designer,81
Carol,engineer,88
Dave,manager,75
```

Open `Ctrl+R`, turn on **Regex** (`Alt+R`), and search for
`(\w+),(\w+),(\d+)` with replacement `$3,$2,$1`. Click **All** (or
`Alt+Enter`): every data row reorders to `score,role,name` — group 3 (the
score) first, group 2 (the role) unchanged in the middle, group 1 (the name)
last, so `Alice,engineer,92` becomes `92,engineer,Alice`. Undo (`Ctrl+Z`) to
put the file back before moving on.

## 3. Interactive query-replace

For a search where you want to look at each match before deciding, press
**`Ctrl+Alt+R`** (or run **Query Replace** from the command palette, `Ctrl+P`
then `>Query Replace`, or click **Ask** in an open Replace bar). Vix jumps to
each match and waits:

- `y` — replace this match, move to the next
- `n` — skip this match, move to the next
- `!` — replace this and every remaining match without asking again
- `q` — quit, leaving anything already replaced in place

Try it on `notes.md`: search for `TODO`, replace with `DONE`. There's only
one real `TODO:` line in that file, so `y` once and `q` (or letting it run
out) is enough to see the flow before you move to the multi-file version
below.

## 4. Workspace search — every file at once

`Ctrl+F` is scoped to the current buffer by default. The **In:** button
(`Alt+I`) widens that scope without retyping anything — query, replacement,
and every toggle travel with it:

**Buffer** → **Files** → **Workspace**

- **Files** hands the search to the workspace search panel — a hit list,
  with replace-all.
- **Workspace** hands it to the bottom dock instead, as click-to-jump lines.

`Ctrl+Shift+F` (**Search in Workspace**) opens the *Files* panel directly,
without a buffer search first.

Try it: `Ctrl+Shift+F`, type `FIXME`, turn on **Case** (`Alt+C`) so it
doesn't also match a stray lowercase `fixme`. Three hits come back, one in
each language the demo workspace uses: `main.rs`, `scripts/hello.py`, and
`notes.md`. Arrow keys move between hits, `Enter`
opens the selected one.

`Tab` switches to **Replace** mode, and adds two more fields you can cycle to
with `Tab`: **Include path** and **Exclude path**, each a regex matched
against each file's workspace-relative path. Include `\.py$` to search only
the Python file; exclude `(^|/)target/` to skip a build directory — an
invalid, half-typed regex is treated as no constraint rather than hiding
everything.

Type a replacement and press `Alt+Enter` (or `Enter` from the Replace field):
instead of writing immediately, this opens a **preview** — every affected
file with its match count. Nothing is written until you confirm with `y` /
`Enter`; `n` / `Esc` cancels. That preview step is what makes a
workspace-wide rewrite safe to attempt: you see the blast radius before
committing to it.

Open buffers are searched and replaced in their current (possibly unsaved)
state; everything else is read from disk. Files over 2 MB and binaries are
skipped, and results cap at 5,000 hits.

### The dock variant

**Search in Workspace → Dock** (from the command palette; it isn't on the
menu, only `Ctrl+Shift+F`'s panel is) prompts for a term — `Alt+C` toggles
case, `Alt+R` toggles regex — then lists `relpath:line:col: text` lines in
the bottom dock, each click-to-jump. Handy when you want the hits to stay
visible in the dock alongside a terminal or test output, rather than taking
over a side panel.

## 5. Editable results ("wgrep")

Rather than clicking through hits one at a time, you can edit the whole hit
list as text. With the `Ctrl+Shift+F` panel open on a real search (not a
static list like go-to-definition), press **`Alt+E`**: the hit list becomes
a genuine, editable buffer, one line per hit in `path:line: text` form. This
is a real tab — normal editing keys work directly on it, no overlay in the
way.

Search `FIXME` again to get the same three hits, `Alt+E`, and edit one
line's text after its `path:line:` prefix — for example, change
`main.rs:19: FIXME: word_counts lowercases nothing...` to end with a note
you want to add. `Ctrl+S` previews a summary ("N edits in M files") before
writing, then `y` / `Enter` applies it, writing each affected source file
back at the recorded position — not by line number, so lines you didn't
touch are untouched. Delete a line instead of editing it to skip that hit
entirely, leaving its source alone; a line you type from scratch (not in
`path:line:` shape) is ignored, not misread as an edit.

## 6. Structural search & replace

Regex matches *text*. Structural replace matches *shape*, tolerating
whitespace and formatting differences the way a regex can't. Patterns use
two kinds of holes:

- `$NAME` — one token, or one whole bracketed group (`(...)`, `[...]`,
  `{...}`) when the next token opens one.
- `$$NAME` — a run of zero or more tokens (an argument list, a whole block
  body), never crossing an unmatched closing bracket.

Everything else in the pattern matches literally, ignoring formatting: `a +
b` matches `a+b` and `a  +  b` alike. A replacement template uses the same
syntax, and each placeholder is substituted with the **original source
text** the hole captured — so replacing preserves whatever formatting that
piece already had.

Try it on the loop in `main.rs`:

```rust
for word in text.split_whitespace() {
    *counts.entry(word).or_insert(0) += 1;
}
```

Run **Edit → Structural Replace…** (`edit.structural_replace`, no default
key — reachable from the menu or the command palette). It prompts for the
pattern first:

```
for $VAR in $$ITER { $$BODY }
```

then the replacement, reusing the captured pieces to add a debug line ahead
of the original body:

```
for $VAR in $$ITER { println!("visiting: {}", $VAR); $$BODY }
```

Press Enter after the replacement and you land in the same `y`/`n`/`!`/`q`
step-through as interactive query-replace, above — `y` here rewrites the
loop to print each word before counting it, with `$$BODY`'s capture
(`*counts.entry(word).or_insert(0) += 1;`) copied through unchanged,
formatting and all. With an active **selection** when you accept the
replacement, only matches inside it are offered; otherwise the whole buffer
is searched.

**Edit → Structural Replace in Workspace…** (`edit.structural_replace_workspace`)
runs the same two-prompt sequence across every file under the workspace
root, and reuses the exact preview-and-confirm step from workspace
search-and-replace above — "N replaced in M files" before anything is
written. That `for $VAR in $$ITER { $$BODY }` pattern only matches Rust
syntax, so run it workspace-wide here and the preview reports exactly one
file (`main.rs`) even though the scan reads every file in the
project.

This is deliberately **not** a full-language parser: matching is
token/bracket-based (identifiers, numbers, quoted strings, bracket pairs,
operator runs), the same mechanism for every language Vix supports rather
than per-language grammar handling. See
[`crates/vix-structural-replace/spec/index.md`](../../../crates/vix-structural-replace/spec/index.md)
for the full pattern syntax and its documented limitations.

## 7. Multi-cursor editing

A **primary** cursor is always active; extra carets stack on top of it, and
typing, `Backspace`/`Delete`, and arrow keys apply to all of them at once, as
one undo step.

### Add the next occurrence

`Ctrl+D` selects the word under the cursor; press it again to add the next
occurrence of that text as a new caret (repeat to keep collecting matches —
it wraps around the buffer). **This is the editor widget's own binding**, so
it's live in keymaps that leave `Ctrl+D` alone (VS Code, IntelliJ, …); the
default **Apple** keymap claims `Ctrl+D` for macOS forward-delete, so there
you add carets with `Alt`+click, or select every occurrence at once with
**Edit → Select → Select All Occurrences** (no default key).

Try the menu version on `word_counts` in `main.rs`: place the
cursor in either occurrence of the name (the call on line 9, or the
`fn word_counts` definition on line 22), then run **Select All Occurrences**.
Both spots get a selected caret — type `count_words` and both change
together. This is a purely *textual* rename (it would also touch a variable
that happened to share the name, in a way the LSP rename from tutorial 2, or
the structural replace above, would not) — useful for exactly this kind of
quick, unambiguous identifier, and a good contrast with those two more
structure-aware tools. Undo (`Ctrl+Z`) to put the name back.

### Add carets by clicking

`Alt`+click drops an extra caret wherever you click; a plain click collapses
back to one cursor. Try it on `data/sample.csv`: `Alt`+click at the end of
the `Alice,engineer,92` line, then `Alt`+click at the end of each remaining
row (`Bob,...,81`, `Carol,...,88`, `Dave,...,75`). Type `,active` — every row
gains a new trailing column in one motion. `Esc` drops the extra carets and
keeps the primary; undo to put the file back.

### Column (box) selection

`Alt+Shift+↑` / `Alt+Shift+↓` (also **Edit → Select → Column Select
Up/Down**) extend a vertical, rectangular selection one line at a time: a
caret is added on the next/previous line, spanning the same columns as the
current selection (or a bare caret with none), clamped to each line's
length. Try it on the same CSV: put the cursor at the very start of the
`Alice,...` line (column 0), then press `Alt+Shift+↓` three times to add a
caret at column 0 on each of the `Bob`/`Carol`/`Dave` lines too. Type `- ` —
every row gets the same prefix at once, turning the four data rows into a
rough Markdown list. Undo when you're done looking.

### Everyday cleanup

`Esc` is the one to remember day to day: it drops every extra caret and
keeps the primary, wherever it currently is. **Remove Multi-Cursor** and
**Remove All Multi-Cursors**, plus **Add Multi-Cursor Up** / **Add
Multi-Cursor Down** for a bare caret directly above or below without a
column span, are also available from the command palette (no default key —
palette-only actions).

See [`docs/multiple-cursors/index.md`](../../multiple-cursors/index.md) for
the complete action list, and
[`crates/vix-editor-core/spec/index.md`](../../../crates/vix-editor-core/spec/index.md)
for how multi-caret edits are implemented under the hood.

---

Previous: [Tutorial 2 — Editing Power Techniques](../02-editing-power-techniques/index.md)
Next: [Tutorial 4 — The Git Workflow](../04-the-git-workflow/index.md)
