# Word-Level Diff

Part of the Compare With File overlay (`tools.diff`).

Within a changed line, the compare view highlights the specific words that differ (bold/reversed) rather than coloring the whole line, making small edits easy to spot.

Built by `crate::diff_view::build` using `similar`'s inline (word-level) diff (the `inline` cargo feature); each `Line` carries `emphasis` char ranges rendered by `ui`.

See `spec/index/index.md` for the project overview and `spec/actions/index.md` for the full action catalog.
