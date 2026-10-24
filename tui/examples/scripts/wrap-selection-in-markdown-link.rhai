// Wraps the current selection in Markdown link syntax: [text](url).
//
// Select some text, run "Wrap Selection in Markdown Link" (Ctrl+Shift+L or
// the command palette), then type the URL and press Enter.
//
// Demonstrates: register_command + bind_key, prompt/on_submit, and reading
// the selection fresh in the answer handler rather than trying to carry it
// across the two calls (see docs/scripting/index.md's "Prompting for
// input" section for why).

register_command("wrap_link", "Wrap Selection in Markdown Link", "on_wrap_link");
bind_key("C-S-l", "wrap_link");

fn on_wrap_link() {
    if selection_text() == "" {
        error("Select some text first, then run Wrap Selection in Markdown Link.");
        return;
    }
    prompt("Link URL:", "on_wrap_link_url");
}

fn on_wrap_link_url(url) {
    if url == "" {
        message("Cancelled: no URL entered.");
        return;
    }
    // Re-read the selection here rather than reusing what on_wrap_link saw --
    // this is a fresh call, not a resumed one (§ "Prompting for input").
    let text = selection_text();
    set_selection_text("[" + text + "](" + url + ")");
}
