# Align on Delimiter

Align pads the selected lines (or the whole buffer, if nothing is selected) so
that each line's first occurrence of a chosen delimiter lands in the same
column — a common editor convenience for tidying up assignment lists, key:
value blocks, CSV-ish text, and tables. It is a menu item under **Edit →
Align**; choosing a delimiter rewrites the affected lines.

## How to use

1. Select the lines you want to align (or select nothing to align the whole
   buffer).
2. Open **Edit → Align**.
3. Choose the delimiter to align on.

## Available delimiters

| Menu item       | Action ID           | Delimiter |
| --------------- | -------------------- | --------- |
| On = (equals)   | `edit.align.equals`  | `=`       |
| On : (colon)    | `edit.align.colon`   | `:`       |
| On , (comma)    | `edit.align.comma`   | `,`       |
| On \| (pipe)    | `edit.align.pipe`    | `\|`      |

There is no default keybinding for any of these — they run from the menu or
the command palette.

## Behavior

- Only the line's **first** occurrence of the delimiter is used as the
  alignment point; anything after it is left alone.
- Each line's delimiter is normalized to exactly one space before it and one
  space after, then the delimiters are padded into a common column across the
  selected lines.
- Lines that don't contain the delimiter at all are left unchanged.
- A trailing newline on the selection/buffer is preserved.

The pure alignment logic lives in `vix_align::on_delimiter`; the menu actions
apply it via `App::transform_selection_or_buffer`, which runs the transform
over the selection when there is one and over the whole buffer otherwise.

## Examples

Select these three lines and choose **On = (equals)**:

```
x=1
long_name=2
y=333
```

becomes:

```
x         = 1
long_name = 2
y         = 333
```

Select these lines and choose **On : (colon)**:

```
name:Alice
role :  Engineer
team: Platform
```

becomes:

```
name : Alice
role : Engineer
team : Platform
```

See the crate spec at `crates/vix-align/spec/index.md`.

---

Vix™ and Vix IDE™ are trademarks.
