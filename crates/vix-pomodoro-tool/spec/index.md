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

The crate follows the `rust-checked-arithmetic` spec and, outside tests, denies `arithmetic_side_effects`, `integer_division`, `modulo_arithmetic`, `as_conversions`, `indexing_slicing` and `string_slice` (RCA-ENF-3). The work length is private, so it is always within 1 to 180 minutes (`work_minutes()` reads it); the minutes-to-seconds product and the countdown subtraction are bounded by that and carry `RCA-EXC-2` expectations. `adjust_minutes` no longer clamps: a new length outside 1 to 180 is refused with `TimerError::OutOfRange { figure, range }`, a step that overflows `i64` with `TimerError::Overflow`, and the length is left unchanged either way. A running countdown still ignores the call.
