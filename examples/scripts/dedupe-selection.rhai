// Removes duplicate lines from the selection, keeping the first
// occurrence of each and preserving overall order.
//
// Demonstrates: reading/writing the selection, and plain Rhai arrays and
// object maps (#{}) -- there's nothing buffer-specific about the
// deduplication itself, it's just Rhai working over a String.

register_command("dedupe_selection", "Deduplicate Selected Lines", "on_dedupe_selection");

fn on_dedupe_selection() {
    let text = selection_text();
    if text == "" {
        error("Select some lines first, then run Deduplicate Selected Lines.");
        return;
    }
    let seen = #{};
    let out = [];
    for line in text.split("\n") {
        if !seen.contains(line) {
            seen[line] = true;
            out.push(line);
        }
    }
    let result = "";
    for (line, i) in out {
        if i > 0 {
            result += "\n";
        }
        result += line;
    }
    set_selection_text(result);
}
