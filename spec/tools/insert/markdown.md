# Tools: Insert: Markdown

Inserts small Markdown templates at the cursor (`App::insert_markdown`).

- menu "Tools"
  - submenu "Insert"
    - submenu "Markdown"
      - menuitem "Headline 1" -> insert `# Headline 1` followed by a blank line.
      - menuitem "Headline 2" -> insert `## Headline 2` followed by a blank line.
      - menuitem "Headline 3" -> insert `### Headline 3` followed by a blank line.
      - menuitem "Link" -> insert `[Example](https://www.example.com)`.
      - menuitem "List" -> insert a three-item `- Item` bullet list.
      - menuitem "Table" -> insert a three-column header/separator/rows table.
      - menuitem "Todos" -> insert a three-item `- [ ] Todo` checklist.
