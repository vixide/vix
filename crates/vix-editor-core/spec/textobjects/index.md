# Textobjects (T616)

Syntax-aware textobjects: the **function**, **class** or **parameter**
around a position, and the next or previous one, taken from the buffer's
Tree-sitter parse tree. They work offline, without a language server, for
every language that bundles a query.

## Queries

Each language may ship `langs/<lang>/textobjects.scm` beside its
`highlights.scm`. A query marks an object with two captures per kind:

| Capture | Meaning |
| --- | --- |
| `@function.around` / `@function.inside` | A function, method, closure or lambda / its body |
| `@class.around` / `@class.inside` | A class, struct, enum, union, trait, interface or impl block (or a language's nearest equivalent) / its body |
| `@parameter.inside` | One parameter or argument in a list |

A quantified capture (`(_)+ @class.inside`) spans all of its nodes. Queries
are compiled once per language and cached with the highlight queries
(`compiled_query`).

Bundled, and checked to compile against each grammar compiled into the
build (`every_bundled_query_compiles`):

| Language | Function | Class | Parameter |
| --- | --- | --- | --- |
| Rust | `fn`, closures | struct, enum, union, trait, impl | parameters, closure and type parameters, type and call arguments |
| Python | `def`, `lambda` | `class` | parameters, lambda parameters, call arguments |
| JavaScript | declarations, expressions, generators, arrows, methods | class declarations and expressions | formal parameters, call arguments |
| TypeScript | as JavaScript | as JavaScript, plus abstract classes, interfaces, enums | as JavaScript, plus type parameters and arguments |
| Go | functions, methods, function literals | `type … struct` / `interface` | parameters, type parameters, call arguments |
| Java | methods, constructors, lambdas | class, interface, enum, record, annotation type | formal parameters, type parameters and arguments, call arguments |
| C# | methods, constructors, destructors, local functions, lambdas | class, struct, interface, enum, record | parameters, call arguments |
| C | function definitions | struct, union, enum | parameters, call arguments |
| C++ | as C, plus lambdas | as C, plus class | as C, plus template parameters and arguments |
| Shell | function definitions | — | — |
| HTML | — | an element | an attribute |
| CSS | — | a rule set | a declaration |
| TOML | — | a table or array-of-tables entry | a key/value pair |
| JSON | — | an object or array | a member or element |
| YAML | — | a mapping entry with a value | a mapping entry |
| Markdown | — | a heading's section | — |

Not bundled: `markdown-inline` (no structure to select) and `text`.
Kotlin, Lua and Zig ship only `highlights.scm` with no grammar compiled
into Vix, so a query could never run; they get textobjects when their
grammar lands. A language without a query reports "unavailable".

**Provenance.** Every query is written for Vix against the grammar's own
`node-types.json`. The capture names follow the convention Helix and
nvim-treesitter-textobjects share, so users of either recognize them, but
no query text is copied from either project (Helix is MPL-2.0 and
nvim-treesitter-textobjects Apache-2.0); the files are covered by Vix's own
license like the rest of `langs/`.

## Derived ranges

- **Inside of a braced body.** When `inside` captures a `{ … }` block, the
  braces and the blank space just inside them are dropped, so the inside of
  a function is its statements; an empty body gives the empty range just
  after `{`. Bodies without braces (Python's indented block, an arrow
  function's expression) are used as captured.
- **No inside capture.** The inside is the whole object.
- **Parameter around.** The parameter plus its separating comma: the
  following comma and the whitespace after it, or, for the last item, the
  preceding comma and the whitespace before the item — Vim's `aa`.

## API

`textobject.rs`:

- `TextObjectKind` (`Function`, `Class`, `Parameter`; `#[non_exhaustive]`)
  and `TextObjectDir` (`Next`, `Previous`).
- `Code::enclosing_textobjects(kind, pos) -> Vec<TextObject>`: every object
  of `kind` whose around range contains the character `pos`, innermost
  first; each `TextObject` carries `around` and `inside` character ranges.
- `Code::textobject_range(kind, inside, pos)`: the innermost one's inside or
  around range.
- `Code::next_textobject(kind, dir, pos)`: the start of the nearest object
  beginning strictly after (or before) `pos`, **wrapping** past the buffer's
  end (or start) when there is none that way. `None` when the buffer has no
  such object.
- `Code::has_textobjects()`: whether the language has a grammar and a query.

## Large buffers

Nothing here parses. Every lookup reads the parse tree the buffer already
has, through a `QueryCursor` limited to a byte window: a single byte at the
position for `enclosing_textobjects`, and for `next_textobject` a window
next to the position (16 KiB) that doubles until it finds a candidate or
reaches the buffer's edge. So the usual case touches only nearby nodes, at
any buffer size. While a large buffer's first parse still runs in the
background there is no tree, and every lookup returns nothing (the App says
textobjects are unavailable rather than parsing synchronously). The Large
File Viewer (`vix-large-file`) has no buffer or tree at all, so the
commands do not apply there.

## In the App

- Actions (Edit → Select, the palette): `edit.select_function_inside`,
  `edit.select_function_around`, `edit.select_class_inside`,
  `edit.select_class_around`, `edit.select_parameter_inside`,
  `edit.select_parameter_around`. Selecting skips an object equal to the
  current selection, so repeating grows outward; each selection is pushed on
  the offline expansion stack that **Shrink Selection** pops
  (`crates/vix-editor/spec/structural-selection/index.md`).
- Actions (Go → Function / Class, the palette): `nav.next_function`,
  `nav.prev_function`, `nav.next_class`, `nav.prev_class`; a jump that
  wrapped says so in the status bar.
- Vi and Spacemacs (the modal engine): `if`/`af`, `ic`/`ac`, `ia`/`aa` after
  an operator or in Visual mode, and `]f`/`[f` (also `]m`/`[m`), `]c`/`[c`,
  `]a`/`[a` as motions, all taking counts — see
  `crates/vix-modal/spec/index.md`.

Implementation: `crates/vix-editor-core/src/textobject.rs`,
`tui/src/app/textobjects.rs`, `tui/src/app/modal.rs`.
