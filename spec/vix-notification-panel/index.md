# notification_panel

The **notification panel** is the message drawer in the right dock. It is a
persistent, reviewable feed of notifications — each a single line with a level
(Info, Advice, Warn, Error) that selects its row icon. Backed by the
`right_dock` module (`Messages`, `Message`, `Level`); rendering lives in `ui`.

## Behavior

- Show/hide with **View → Show/Hide Right Dock** or the menu-bar dock icon; the
  width persists in `messages_width`.
- Notifications are listed oldest first with one row selected.
- `↑` / `↓` move the selection; `Delete` / `Enter` / the row `x` dismiss the
  highlighted notification (the selection stays in range so several can be cleared
  in a row).

## Feed sources

Beyond inline editor advice, the panel collects the results of **background work**
so there is a durable record after the transient status-bar message scrolls away:

- **Run Command** completions — `command_finished` (Info on exit 0, Error
  otherwise), in addition to the streamed output in the bottom dock. This also
  covers Git Pull / Push / Fetch, which run through the same pipeline.
- **AI menu transforms** (Summarize, Explain, Define, Annotate, Improve) —
  `ai_done` (Info) / `ai_failed` (Error). The interactive AI chat panel keeps its
  own transcript and does **not** also post here, to avoid duplication.
- Failures from other actions (open/save errors, bad regex, …) as before.

## As implemented in Vix

`poll_command` posts the completion notification keyed by the command label stored
on `RunningCommand`; `poll_ai_replace` posts AI outcomes for every destination
except `AiDest::Panel`.
