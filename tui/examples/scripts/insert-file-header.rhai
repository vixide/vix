// Inserts a one-line header comment at the top of the buffer, e.g. a
// short description of what the file is for.
//
// Assumes a "//"-style line comment -- the scripting API has no way to ask
// what language the active buffer is in (v1 exposes buffer text only, not
// its path or file type), so this is deliberately generic. Edit the
// prefix below if you want a different comment style.
//
// Demonstrates: prompt/on_submit, and computing the whole new buffer text
// before writing it back with a single set_buffer_text call.

register_command("insert_header", "Insert File Header", "on_insert_header");

fn on_insert_header() {
    prompt("One-line file description:", "on_insert_header_desc");
}

fn on_insert_header_desc(desc) {
    if desc == "" {
        message("Cancelled: no description entered.");
        return;
    }
    let header = "// " + desc + "\n";
    set_buffer_text(header + buffer_text());
    set_cursor_offset(header.len());
}
