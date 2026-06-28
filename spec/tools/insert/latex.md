# Tools: Insert: LaTeX

Inserts Org/LaTeX-style markup snippets at the cursor (`App::insert_latex`).

- menu "Tools"
  - submenu "Insert"
    - submenu "LaTeX"
      - menuitem "Headline" -> `* Headline`
      - menuitem "Subheadline" -> `** Subheadline`
      - menuitem "Link" -> `[[https://org.mode][Org]]`
      - menuitem "Bold" -> `*hello*`
      - menuitem "Italic" -> `/hello/`
      - menuitem "Underline" -> `_hello_`
      - menuitem "Table" -> a three-column header/separator/rows table.
      - menuitem "Deadline" -> `DEADLINE: <YYYY-MM-DD Day>`
      - menuitem "Scheduled" -> `SCHEDULED: <YYYY-MM-DD Day>`
      - menuitem "Time Range" -> `<…>--<…>` active timestamp range.
      - menuitem "Timestamp" -> `<2006-11-02 Thu 10:00-12:00>`
      - menuitem "Timestamp Repeater" -> `<… +1w>`
      - menuitem "Quote" -> a `#+BEGIN_QUOTE … #+END_QUOTE` block.
      - menuitem "Verse" -> a `#+BEGIN_VERSE … #+END_VERSE` block.
      - menuitem "Center" -> a `#+BEGIN_CENTER … #+END_CENTER` block.
      - menuitem "Drawer" -> a `:DRAWERNAME: … :END:` drawer.

The placeholder text (URLs, dates, sample sentences) is meant to be edited after
insertion.
