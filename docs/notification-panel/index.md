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

## Reading and Dismissing

- `↑` / `↓` move the selection between notifications.
- Dismiss the highlighted notification with the close `x` on its row, the
  `Delete` key, or `Enter`. The selection stays in range after a notification is
  removed, so you can clear several in a row by pressing `Delete` repeatedly.

## Example

After a failed action, an **Error** notification appears. Open the panel
(**View → Show/Hide Right Dock**), highlight the message with `↑` / `↓`, then
press `Delete` to dismiss it once you have read it.
