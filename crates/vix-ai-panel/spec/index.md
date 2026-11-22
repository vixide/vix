# Ai Panel

AI chat panel: a persistent conversation surface for the configured assistant.

The host (see `app.rs`) drives the actual CLI via the shared `spawn_ai`
machinery and the `ai_command` setting; this module is pure data. It holds the
conversation [`Turn`]s, the in-progress input line, a busy flag (a request is
in flight), and a scroll offset measured in wrapped lines up from the bottom.

Keeping it data-only makes the transcript layout (word wrapping, the visible
window) unit-testable without a terminal.

## Sub-specs

- [ai](ai/index.md)
- [vix-agent-panel](vix-agent-panel/index.md)
- [vix-notification-panel](vix-notification-panel/index.md)

## Checked arithmetic

The crate follows the `rust-checked-arithmetic` spec and, outside tests, denies `arithmetic_side_effects`, `integer_division`, `modulo_arithmetic`, `as_conversions`, `indexing_slicing` and `string_slice` (RCA-ENF-3). The scroll window uses `checked_*` and `get`; the old `saturating_*` calls are gone.

| ID | Failure | Declared fallback | Why it is correct |
| -- | ------- | ----------------- | ----------------- |
| `AIP-F1` | `scroll + 1` overflows | the current scroll | A scroll offset cannot go past the top of the transcript. |
| `AIP-F2` | A subtraction goes below zero (`scroll - 1`, `lines - height`, ...) | `0` | A transcript shorter than the viewport does not scroll, and the view stays at the bottom. |
| `AIP-F3` | The computed window is not a valid range | an empty window | `start <= end <= len` holds by construction, so an empty window is only the safe answer to a broken invariant. |
