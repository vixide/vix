# Zen Mode

Zen mode is a focus mode that hides the surrounding chrome so the editor fills the
screen for distraction-free writing.

## Using it

Toggle it from **View → Layout → Zen Mode** or the command palette (*Zen Mode*).
It hides the file explorer, the messages drawer, the bottom dock, and the status
bar — leaving the editor and the slim one-row menu bar. Toggle it again to bring
everything back exactly as it was.

## Notes

Zen mode is a runtime view state: it remembers and restores your previous dock and
status-bar visibility, and does not overwrite your saved settings, so it never
changes what you see after a restart. See the specification at
`crates/vix-editor/spec/zen-mode/index.md`.

---

Vix™ and Vix IDE™ are trademarks.
