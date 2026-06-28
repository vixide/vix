# Macros

Record a sequence of keystrokes once and replay it — and now **save** macros so
they persist between sessions.

## Record and play

- **Edit → Record Macro** starts recording; run it again to stop.
- **Edit → Play Macro** replays what you just recorded at the cursor.

## Save and reuse

- **Edit → Save Macro…** asks for a name and stores the recording in
  `macros.toml` (in your config directory). Saving with an existing name updates
  that macro.
- **Edit → Play Saved Macro…** lists your saved macros; pick one to replay it.
  Use `↑` / `↓` to choose, `Enter` to play, `Esc` to cancel.

## The file

Saved macros live in `macros.toml`, which you can edit by hand:

```toml
[[macro]]
name = "wrap-parens"
keys = ["(", "Right", "Right"]
```

Each entry is a key token — the key plus modifier prefixes `C-` (Ctrl), `A-`
(Alt), `S-` (Shift): e.g. `C-c`, `S-Tab`, `Enter`, `A-Left`, `F5`, `Space`.

See the specification at `spec/macros/index.md`.
