# Choice List

A small titled list of rows with a selection, for overlays that let you pick
one line: Local History snapshots, run configurations, a definition peek.

The crate holds only presentation state: `Panel { title, hint, rows, selected,
scroll }`. Titles, hints and rows arrive already translated. Navigation (`up`,
`down`, `page_up`, `page_down`, `select_index`, `ensure_visible`) delegates to
`vix_list_state`, so selection and scroll clamp the same way as every other
list panel. `with_selected` starts the highlight on a given row (the peek
overlay starts on the definition's line).

What a chosen row *means* is the host's business: `tui/src/app/ide_parity.rs`
wraps a `Panel` in `ChoiceList { panel, purpose }` where the purpose is one of
`LocalHistory`, `RunConfig` or `Peek`. One overlay and one draw function
(`tui/src/ui/choice_list.rs`) therefore serve all three features.

## Out of scope

- Filtering or typing into the list; the three users are short lists.
- Multi-select.
