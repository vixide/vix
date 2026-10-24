// Inserts a signature line with today's date at the cursor (or replacing
// the selection): "-- 2026-09-04".
//
// Demonstrates: now() (§ API v1, "Clock") -- added specifically because
// this sample needed it; v1 originally had no way for a script to read
// the current date at all.

register_command("timestamp_signature", "Insert Timestamp Signature", "on_timestamp_signature");
bind_key("C-S-d", "timestamp_signature");

fn on_timestamp_signature() {
    set_selection_text("-- " + now());
}
