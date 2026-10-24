// Title-cases the line the cursor is on: "the quick fox" -> "The Quick Fox".
//
// The scripting API has no "replace the current line" primitive -- only
// current_line() to read it and set_buffer_text/set_selection_text to
// write. So this finds the line's boundaries in the whole buffer text
// using cursor_offset(), builds the new buffer text in full, and writes
// it back in one call. That "compute everything, then write once" shape
// matters for any handler, not just this one: set_buffer_text isn't
// transactional, so a handler that mutates incrementally can't roll back
// partway through if something later goes wrong (see docs/scripting/
// index.md's "Error handling" section).
//
// Demonstrates: buffer_text/cursor_offset/set_cursor_offset, and plain
// Rhai string/char manipulation (no special "line" API needed).

register_command("title_case_line", "Title Case Current Line", "on_title_case_line");
bind_key("C-S-t", "title_case_line");

fn on_title_case_line() {
    let text = buffer_text();
    let pos = cursor_offset();

    let start = pos;
    while start > 0 && text[start - 1] != '\n' {
        start -= 1;
    }
    let end = pos;
    while end < text.len() && text[end] != '\n' {
        end += 1;
    }

    let line = text.sub_string(start, end - start);
    let words = line.split(" ");
    let titled = "";
    for word in words {
        if word.len() > 0 {
            titled += word[0].to_upper() + word.sub_string(1);
        }
        titled += " ";
    }
    // Rhai's trim() mutates in place and returns (); don't assign its result.
    titled.trim();

    let new_text = text.sub_string(0, start) + titled + text.sub_string(end);
    set_buffer_text(new_text);
    set_cursor_offset(start + titled.len());
}
