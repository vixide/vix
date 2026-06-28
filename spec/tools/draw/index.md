# Tools: Draw (ditaa ASCII art)

**Tools → Draw** inserts ASCII-art shapes for drawing
[ditaa](https://ditaa.sourceforge.net/) diagrams (DIagrams Through Ascii Art) —
ASCII drawings that ditaa renders to bitmap images. Each item inserts a fragment
at the cursor (`App::draw_insert`); you arrange and connect them by hand.

## Shapes

| Item | Action | Inserts |
| ---- | ------ | ------- |
| Rectangle | `tools.draw.rectangle` | a `+`/`-`/`|` box |
| Rounded Rectangle | `tools.draw.rounded` | a `/`,`\` cornered box |
| Document | `tools.draw.document` | a box tagged `{d}` (ditaa document shape) |
| Storage | `tools.draw.storage` | a box tagged `{s}` (ditaa storage shape) |
| Horizontal Line | `tools.draw.line_h` | `--------` |
| Vertical Line | `tools.draw.line_v` | a column of `|` |
| Horizontal Dashed Line | `tools.draw.dashed_h` | `========` (ditaa dashes a `=` line) |
| Vertical Dashed Line | `tools.draw.dashed_v` | a column of `:` |
| Arrow Right / Left | `tools.draw.arrow_right` / `arrow_left` | `------->` / `<-------` |
| Arrow Up / Down | `tools.draw.arrow_up` / `arrow_down` | a `^`/`v`-tipped column |
| Point | `tools.draw.point` | `*` (a ditaa dot marker) |
| Two-Box Flow | `tools.draw.flow` | two boxes joined by an arrow |
| Colored Box | `tools.draw.color_box` | a rounded box with a `cBLU` color tag |

ditaa interprets `+`/`-`/`|` as square corners/edges, `/`,`\` as rounded corners,
`>`,`<`,`^`,`v` as arrowheads, `=`/`:` as dashed horizontal/vertical lines, color
tags like `cBLU`/`cRED`, and shape tags like `{d}` (document) and `{s}` (storage).
