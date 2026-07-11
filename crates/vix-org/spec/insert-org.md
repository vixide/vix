# Tools: Insert: Org

Three Org-mode submenus under Tools → Insert.

## Org (`App::insert_org`)

Inserts a snippet at the cursor:

- "Title" -> `#+title: Hello World`
- "Author" -> `#+author: Alice Adams`
- "Headline" -> `* Headline`
- "Subheadline" -> `** Subheadline`
- "Link" -> `[[https://org.mode][Org]]`
- "Image" -> `[[https://example.com]]`
- "List" -> a `- ` bullet list (Alfa/Bravo/Charlie)
- "Ordered List" -> a `1.` numbered list
- "Check List" -> `- [ ]` / `- [-]` / `- [x]` items
- "Table" -> a three-column header/separator/rows table
- "TODO" -> `**** TODO A todo item.`
- "DONE" -> `**** DONE A todo item that has been done.`
- "Deadline" -> `DEADLINE: <YYYY-MM-DD Day>`
- "Scheduled" -> `SCHEDULED: <YYYY-MM-DD Day>`
- "Time Range" -> `<…>--<…>`
- "Timestamp" -> `<2006-11-02 Thu 10:00-12:00>`
- "Timestamp Repeater" -> `<… +1w>`
- "Drawer" -> `:DRAWERNAME: … :END:`
- "Properties" -> a `:PROPERTIES: … :END:` property drawer (sample metadata)

## Markers (`App::insert_marker` → `App::toggle_wrap`)

The marker items live **inside the Org submenu** (menus are three levels deep, so
they are leaf items under Tools → Insert → Org rather than a fourth-level
submenu). Each toggles a marker around the selection (via
[`crate::affix::toggle`]); with no selection, inserts the empty pair and places
the cursor between the halves.

- "Tag :" -> `:…:`
- "Bold *" -> `*…*`
- "Italic /" -> `/…/`
- "Underline _" -> `_…_`
- "Strikethrough +" -> `+…+`
- "Code ~" -> `~…~`
- "Verbatim =" -> `=…=`

## Begin-End (`App::insert_block` → `App::toggle_wrap`)

Toggles a `#+BEGIN_…` / `#+END_…` block around the selection:

- "Comment" -> `#+BEGIN_COMMENT … #+END_COMMENT`
- "Center" -> `#+BEGIN_CENTER … #+END_CENTER`
- "Quote" -> `#+BEGIN_QUOTE … #+END_QUOTE`
- "Verse" -> `#+BEGIN_VERSE … #+END_VERSE`

(Org's canonical closing keyword is `#+END_…` for every block; the spec uses it
uniformly.)
