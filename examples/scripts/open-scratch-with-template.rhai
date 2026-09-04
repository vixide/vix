// Fills the active buffer with a scratch-note template: a heading with
// today's date, ready to type into.
//
// The scripting API operates on the active buffer only -- deliberately,
// see docs/scripting/index.md's "What's deliberately not in v1" -- so a
// script can't open a new tab itself. Open a fresh scratch buffer first
// (Ctrl+N), then run "Insert Scratch Note Template".
//
// Demonstrates: now() again, and a simple guard (refusing to overwrite a
// buffer that already has content) rather than trusting the caller.

register_command("scratch_template", "Insert Scratch Note Template", "on_scratch_template");

fn on_scratch_template() {
    if buffer_text() != "" {
        error("The active buffer isn't empty -- open a new buffer first (Ctrl+N), then run this.");
        return;
    }
    let template = "# " + now() + "\n\n";
    set_buffer_text(template);
    set_cursor_offset(template.len());
}
