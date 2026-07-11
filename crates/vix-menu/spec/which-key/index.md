# Which-Key Popup

No action id; a passive discovery overlay.

While a chorded key prefix is pending -- Emacs `Ctrl+X` or the Spacemacs `Space` leader -- a popup at the bottom-right lists the candidate next keys and the action each triggers.

Driven by `App::which_key`, which reads the shared `SPACEMACS_LEADER` / `EMACS_CTRL_X` tables; rendered by `draw_which_key`. See `crates/vix-keymap-model/spec/index.md`.

See `spec/index/index.md` for the project overview and `crates/vix-editor-core/spec/index.md` for the full action catalog.
