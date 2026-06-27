# Tools: Insert: Lorem ipsum

Inserts deterministic placeholder text at the cursor. Module `lorem` derives the
output from a fixed canonical passage so it is stable and testable.

- menu "Tools"
  - submenu "Insert"
    - submenu "Lorem ipsum"
      - menuitem "Words" -> insert the first several lorem words (`lorem::words`).
      - menuitem "Sentence" -> insert one lorem sentence (`lorem::sentence`).
      - menuitem "Paragraph" -> insert a full lorem paragraph (`lorem::paragraph`).
