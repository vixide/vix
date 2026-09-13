# Surround

Surround wraps the current selection in a matching pair of brackets or quotes
— or removes that pair if the selection is already wrapped. It is a menu item
under **Edit → Surround**; choosing one transforms the selection in place.

The feature is a thin editor wiring around the `vix-affix` crate, which is
pure text logic: adding, dropping, or toggling a `prefix`/`suffix` pair around
a string. `vix-affix` has no menu presence of its own — it does not do
anything with lines or files, only strings — the surrounding feature lives in
`App::surround` / `App::toggle_wrap` in `src/app.rs`.

## How to use

1. Select the text you want to wrap.
2. Open **Edit → Surround**.
3. Choose a pair.

With nothing selected, Surround still runs: it inserts the empty pair at the
cursor and leaves the cursor between the two halves, ready to type.

## Available pairs

| Menu item          | Action ID                     | Pair    |
| ------------------- | ------------------------------ | ------- |
| ( Parentheses )     | `edit.surround.paren`          | `(` `)` |
| [ Brackets ]         | `edit.surround.bracket`        | `[` `]` |
| { Braces }           | `edit.surround.brace`          | `{` `}` |
| < Angles >           | `edit.surround.angle`          | `<` `>` |
| " Double Quotes "    | `edit.surround.double_quote`   | `"` `"` |
| ' Single Quotes '    | `edit.surround.single_quote`   | `'` `'` |
| \` Backticks \`      | `edit.surround.backtick`       | `` ` `` `` ` `` |

There is no default keybinding for any of these — they are menu- and
palette-driven only.

## Toggle behavior

Each action toggles: if the selected text already starts with the pair's
opening character and ends with its closing character, running the action
again strips the pair instead of doubling it up. This is `vix_affix::toggle`
underneath — it checks whether the text is already wrapped before deciding
whether to add or drop.

## Examples

Select `alfa` and choose **( Parentheses )**:

- `alfa` → `(alfa)`
- Run **( Parentheses )** again on the still-selected `(alfa)` → `alfa`

Select `hello world` and choose **" Double Quotes "**:

- `hello world` → `"hello world"`

With no selection, choosing **{ Braces }** inserts `{}` and puts the cursor
between the two, so typing continues inside the pair.

See the crate spec at `crates/vix-affix/spec/index.md` for the underlying
`add`/`drop`/`toggle` functions.

---

Vix™ and Vix IDE™ are trademarks.
