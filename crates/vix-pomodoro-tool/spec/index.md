# Pomodoro

A Pomodoro work/break countdown as a small state machine. The module is
`pomodoro_tool`.

In menu "Tools", add menuitem "Pomodoro...".

This opens pomodoro_tool.

The user sees a timer that the user can set. Default is 25 minutes.

When the user clicks "Start", then the dialog box closes and the timer starts a countdown in the background async (the countdown keeps tracking while the dialog is closed).

When the user clicks "Stop", then the timer stops a countdown, and the time resets to the previous start time.

When the time reaches zero, then Vix displays an alert that says "Pomodoro break: 5 minutes" (with button "Cancel"). The break time counts down. When the break time reaches zero, then the alert automatically closes.

## Checked arithmetic

The crate follows the `rust-checked-arithmetic` spec and, outside tests, denies `arithmetic_side_effects`, `integer_division`, `modulo_arithmetic`, `as_conversions`, `indexing_slicing` and `string_slice` (RCA-ENF-3). Minutes to seconds, the countdown and the label use `checked_*`; `adjust_minutes` no longer uses `saturating_add` and `clamp`, and states its declared bounds (1 to 180 minutes) in a `match`.

| ID | Failure | Declared fallback | Why it is correct |
| -- | ------- | ----------------- | ----------------- |
| `TIMER-F1` | Minutes times 60 does not fit | the maximum work length, 180 minutes | The work length is bounded by 180 minutes. |
| `TIMER-F2` | The adjusted length is outside 1 to 180, or the sum overflows | the nearer bound | The declared bounds are 1 and 180 minutes. |
| `TIMER-F3` | A tick would take the countdown below zero | `0` | The countdown stops at zero; the phase change is handled by the caller. |
| `TIMER-F4` | A division or remainder by 60 fails | `0` | The divisor is a constant above zero, so this cannot happen. |
