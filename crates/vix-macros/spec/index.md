# Macros

Persisted keyboard macros: named recordings of editor key events. Vix records
and replays them, and recordings can be **saved by name** to `macros.toml` and
replayed in later sessions.

## Recording & playback

- **Edit → Macro → Record** (`macro.record`) starts/stops recording editor keys.
- **Edit → Macro → Play** (`macro.play`) replays the just-recorded keys at the
  cursor.

## Persistence

- **Edit → Macro → Save…** (`macro.save`) prompts for a name and writes the recorded
  key sequence to `macros.toml` in the config directory (`Settings::macros_path`).
  No-ops with a status note when nothing has been recorded; re-using a name
  replaces that macro.
- **Edit → Macro → Play Saved…** (`macro.play_saved`) opens a chooser of saved
  macros; choosing one loads its keys into the active macro buffer and plays it.

## Storage format

```toml
[[macro]]
name = "wrap-parens"
keys = ["(", "Right", "Right"]
```

Each key is a token: the key name plus modifier prefixes — `C-` (ctrl), `A-`
(alt), `S-` (shift, for named keys; an uppercase char already implies shift).
Examples: `C-c`, `S-Tab`, `Enter`, `A-Left`, `F5`, `Space`.

## As implemented in Vix

The `macros` module owns the `KeyEvent`↔token codec (`encode`/`decode`, unit
tested), the `Macro` schema, and `load`/`upsert` over `macros.toml`. The host
owns `begin_save_macro`/`save_macro`, `open_macro_chooser`/`macro_key`/
`run_selected_macro`, and the `SaveMacro` prompt; `ui::draw_macro_chooser`
renders the chooser.
