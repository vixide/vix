# Matching Tag

Despite the crate name `vix-tags`, this is not Org-mode tags (see
[Org](../org/index.md)) and not a ctags/etags integration — it is **matching
HTML/XML tag navigation**: jumping the cursor between an opening tag and its
closing partner, or back again.

It is a single menu item, **Go → Matching Tag**, action id
`nav.matching_tag`.

## How to use

1. Put the cursor inside or on an HTML/XML opening or closing tag.
2. Open **Go → Matching Tag** (or run it from the command palette).

The cursor jumps to the partner tag: from an opening tag to its closing tag,
or from a closing tag back to its opener. Nesting is tracked correctly, so
jumping from the outer `<div>` in `<div><div>x</div></div>` lands on the
outer closing `</div>`, not the inner one.

- **Self-closing tags** (`<br/>`) and **unmatched tags** (no partner found) do
  nothing — the cursor stays put, and Vix reports "No matching tag" on the
  status line.
- There is no default keybinding; it runs from the **Go** menu or the command
  palette.

## Example

```
<section>
  <p>Hello <em>world</em></p>
</section>
```

With the cursor on the `<section>` opening tag, running **Matching Tag** moves
it to the closing `</section>` two lines down. With the cursor on `<em>`, it
jumps to `</em>` on the same line — the nearer, correctly-nested partner, not
the outer `</p>`.

## Implementation

The pure matching logic is `vix_tags::matching_tag`, which takes the buffer
text and a cursor offset and returns the matching offset (or `None`). The
menu action jumps there via `Editor::goto_offset`.

See the crate spec at `crates/vix-tags/spec/index.md`.

---

Vix™ and Vix IDE™ are trademarks.
