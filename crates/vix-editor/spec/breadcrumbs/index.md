# Breadcrumbs

An optional breadcrumb bar above the editor showing the active file and the
enclosing symbol at the cursor (`file ▸ symbol`).

Toggle it from **View → Layout → Breadcrumbs** or the command palette (action
`view.breadcrumbs`). The preference persists in settings (`show_breadcrumbs`, off
by default).

## As implemented in Vix

`App::breadcrumb` builds the text: the active tab's file name (or `untitled`),
then the nearest enclosing symbol at the cursor line, found from the same symbol
scan the outline uses (`palette::symbols`). `ui::draw_breadcrumb` renders the row;
`ui::center_split` allocates it between the tab bar and the editor when enabled.
