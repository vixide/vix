# Zen Mode

Zen (focus) mode hides the surrounding chrome for distraction-free editing.

Toggle it from **View → Layout → Zen Mode** or the command palette (action
`view.zen`). It hides the file explorer, the messages dock, the bottom dock, and
the status bar, leaving the editor (and the one-row menu bar). Toggling again
restores the previous visibility.

## As implemented in Vix

`App::toggle_zen` saves the prior `(explorer, messages, bottom dock, status bar)`
visibility into `App::zen_saved` and sets each to hidden; the next toggle restores
from it. The change is runtime-only — it does not overwrite the saved settings.
`App::is_zen` reports whether it is active.
