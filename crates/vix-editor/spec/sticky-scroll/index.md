# Sticky Scroll

Setting `sticky_scroll` (on by default).

Pin the header line of the enclosing scope (from the symbol scan) to the top row of the editor while scrolling, so the current function or section stays visible.

Toggled from **View -> Editor**. `App::sticky_header`; `ui` reserves the top editor row when active. See `crates/vix-editor/spec/index.md`.

## Finding the enclosing declaration (T858)

The header is the last declaration (as `palette::symbols` would find it) on a
line above the first visible one. It is found by reading lines up from that
line to the nearest declaration (`palette::symbol_in_line`, one line at a
time, straight from the rope), never by copying and scanning the whole
buffer: before T858 the frame after every edit did that, about 7 ms and one
copy of the text per MiB of buffer (0.4 s and 64 MiB per keystroke at
64 MiB). What a scan saw is remembered (`tui/src/app/enclosing_symbol.rs`):
the line it found and how far below it no line declares anything, for that
tab, path and buffer revision. After an edit the remembered lines above the
first changed one still hold (`Code::changed_since`), so typing below the
remembered range rereads nothing and typing inside it rereads only from the
changed line up. The breadcrumb (`App::breadcrumb`, the declaration at or
above the cursor line) does the same with its own record. Lines are the
editor's own: in a buffer with a lone `\r` or a Unicode line separator they
are numbered as the editor numbers them (before, the whole-buffer scan
numbered them as `str::lines` does, so the header could be off by a line
there).


See `spec/index/index.md` for the project overview and `crates/vix-editor-core/spec/index.md` for the full action catalog.
