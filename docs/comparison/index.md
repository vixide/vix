# Comparison

Vix is a terminal text editor and TUI IDE. This page positions Vix against four
well-known editors so you can decide whether it fits your workflow.

## Simpler than Fresh Editor

Vix is deliberately simpler than Fresh Editor:

- No plugins.
- No JavaScript.
- No TypeScript.
- No remote editing.

The result is a smaller, self-contained editor with no extension runtime to learn
or configure.

## More advanced than Micro

Vix offers several features beyond the Micro editor:

- A **left-side drawer** that is a file explorer.
- A **right-side drawer** that is a message explorer.
- A **bottom dock** for logs, output, and data (Run Command, workspace search).
- **Switchable themes** — Dark, Light, and additional JSON themes.
- A **switchable UI language** (internationalization).

## Easier for novices than vim

Vim is modal by default, which novices often find unfamiliar. Vix is easier
because:

- The default keymap (**Apple**) is not modal. Keyboard shortcuts follow typical
  macOS and Windows conventions — for example, `Ctrl-C` for Copy.
- Power users who prefer modal editing can opt into the **Vim keymap** via
  **View → Keymap…**. See [keymaps](../keymaps/index.md).

## Easier for novices than emacs

Emacs relies on chorded commands and its own vocabulary. Vix is easier because:

- The default keymap (**Apple**) uses macOS/Windows shortcuts — for example,
  `Ctrl-C` for Copy.
- The menu bar is on by default.
- Command names are conventional (Copy/Paste, not Kill/Yank).
- Power users who prefer chords can opt into the **Emacs keymap**.

## Summary

Vix aims at a middle ground: more capable than a minimal editor like Micro, but
simpler and more approachable than fully extensible or modal editors. Novices get
familiar shortcuts and a visible menu bar by default, while power users can switch
to Vim or Emacs keymaps when they want them.
