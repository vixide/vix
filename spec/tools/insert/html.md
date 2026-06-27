# Tools: Insert: HTML

Inserts small HTML templates at the cursor (`App::insert_html`).

- menu "Tools"
  - submenu "Insert"
    - submenu "HTML"
      - menuitem "Headline 1" -> insert `<h1>Headline</h1>` followed by a blank line.
      - menuitem "Headline 2" -> insert `<h2>Headline</h2>` followed by a blank line.
      - menuitem "Headline 3" -> insert `<h3>Headline</h3>` followed by a blank line.
      - menuitem "Link" -> insert `<a href="https://www.example.com">Example</a>`.
      - menuitem "List" -> insert a `<ul>` with three `<li>Item</li>` entries.
      - menuitem "Table" -> insert a `<table>` with thead, tbody (two rows), and tfoot.
