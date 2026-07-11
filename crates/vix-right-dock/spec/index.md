# Right Dock

State for the right dock: a message drawer of advice and notifications, each
individually dismissable, plus the current selection.

Pure data — the host (the `vix` app) renders the drawer and routes keys and
clicks; this crate owns only the message list and selection logic.

## See also

- [bottom-dock spec](../../vix-bottom-dock/spec/) — shared dock behavior
