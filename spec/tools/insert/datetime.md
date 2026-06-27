# Tools: Insert: Date/Time

Inserts the current local date-time, formatted to a standard, at the cursor.
Helpers live in `clock_panel` (`iso8601`, `rfc3339`, `epoch_seconds`).

- menu "Tools"
  - submenu "Insert"
    - submenu "Date/Time"
      - menuitem "ISO 8601" -> insert `YYYY-MM-DDTHH:MM:SS` (local, no offset).
      - menuitem "RFC 3339" -> insert `YYYY-MM-DDTHH:MM:SS±HH:MM` (with the local offset).
      - menuitem "Epoch" -> insert the Unix time in whole seconds.
