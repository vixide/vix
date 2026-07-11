# Pomodoro

Module pomodoro_tool.

In menu "Tools", add menuitem "Pomodoro...".

This opens pomodoro_tool.

The user sees a timer that the user can set. Default is 25 minutes.

When the user clicks "Start", then the dialog box closes and the timer starts a countdown in the background async (the countdown keeps tracking while the dialog is closed).

When the user clicks "Stop", then the timer stops a countdown, and the time resets to the previous start time.

When the time reaches zero, then Vix displays an alert that says "Pomodoro break: 5 minutes" (with button "Cancel"). The break time counts down. When the break time reaches zero, then the alert automatically closes.
