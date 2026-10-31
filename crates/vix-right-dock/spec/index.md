# Right Dock

Right dock: a message drawer of advice and notifications + selection. This
crate holds the state: each message is individually dismissable, plus the
current selection.

Pure data — the host (the `vix` app) renders the drawer and routes keys and
clicks; this crate owns only the message list and selection logic.

## Links in messages (T628)

In a terminal that supports OSC 8 hyperlinks, the host makes each
`http://`, `https://` or `file://` URL in a message clickable, the same way
as the markdown preview (see `crates/vix-markdown-preview/spec/index.md`):
only URLs that occur in the messages' own text are linked, a URL cut short
by the dock edge links to the full URL, and only printable-ASCII URLs are
linked, so a message cannot inject an escape sequence.

## See also

- [bottom-dock spec](../../vix-bottom-dock/spec/) — shared dock behavior
