# Notification Panel

The notification panel is the message drawer in the **right dock**. It collects
notifications and advice from the editor — each a single line of text that you
can read and dismiss individually.

## Show / Hide

Toggle the panel with **View → Show/Hide Right Dock**, or by clicking the
right-edge dock icon in the menu bar. Its width is draggable, and the chosen
width persists in the `messages_width` setting.

## Notification Levels

Each notification has a level, which selects its row icon:

- **Info** — general information.
- **Advice** — a suggestion or hint.
- **Warn** — a warning.
- **Error** — an error.

Notifications are listed oldest first, with one row selected.

## What lands here

As well as inline editor advice, the panel collects the **results of background
work**, so you have a durable record after the status-bar message fades:

- **Run Command** (and Git Pull / Push / Fetch, which use the same pipeline)
  post a completion notification — **Info** on success, **Error** on a non-zero
  exit — alongside their streamed output in the bottom dock.
- **AI menu** transforms (Summarize, Explain, Define, Annotate, Improve) post
  their done/failed outcome. The interactive AI chat panel keeps its own
  transcript and does not duplicate into the feed.

## Reading and Dismissing

- `↑` / `↓` move the selection between notifications.
- Dismiss the highlighted notification with the close `x` on its row, the
  `Delete` key, or `Enter`. The selection stays in range after a notification is
  removed, so you can clear several in a row by pressing `Delete` repeatedly.

## Example

After a failed action, an **Error** notification appears. Open the panel
(**View → Show/Hide Right Dock**), highlight the message with `↑` / `↓`, then
press `Delete` to dismiss it once you have read it.
