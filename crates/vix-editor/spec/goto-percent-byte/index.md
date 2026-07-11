# Go to Percent / Byte

Navigation actions `nav.goto_percent` and `nav.goto_byte`.

Go to Percent jumps to N% of the way through the file (by line, 0-100). Go to Byte jumps to a byte offset (clamped to the buffer end, mapped to a character index). Both prompt for the value.

From the **Go** menu or the command palette. Host `Editor::goto_percent` / `goto_byte`; `Code::len_bytes`. See `crates/vix-editor-core/spec/index.md`.

See `spec/index/index.md` for the project overview and `crates/vix-editor-core/spec/index.md` for the full action catalog.
