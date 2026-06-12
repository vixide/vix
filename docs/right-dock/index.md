# Right Dock

The right dock is the **message drawer**: a list of advice and notifications,
each one individually dismissable. It sits on the right edge of the body.

## Show / Hide

Toggle the drawer with **View → Show/Hide Right Dock**, or by clicking the
right-edge dock icon in the menu bar. Its width is draggable, and the chosen
width persists in the `messages_width` setting.

## Messages

Each message has a level and a line of text. The level selects the row icon:

- **Info** — general information.
- **Advice** — a suggestion or hint.
- **Warn** — a warning.
- **Error** — an error.

Messages are listed oldest first, with one row selected.

## Navigation and Dismissal

- `↑` / `↓` move the selection between messages.
- Dismiss the highlighted message with the close `x` on its row, the `Delete`
  key, or `Enter`. The selection stays in range after a message is removed.

## Example

A background save error appears as an **Error** row in the drawer. Use `↑` / `↓`
to highlight it, read the text, then press `Enter` or click its `x` to dismiss
it.
