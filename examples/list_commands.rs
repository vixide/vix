//! Print every command available in the command palette's `>` mode, alongside
//! the action identifier each one dispatches.
//!
//! Run with: `cargo run --example list_commands`

use stride::palette::COMMANDS;

fn main() {
    let width = COMMANDS.iter().map(|(label, _)| label.len()).max().unwrap_or(0);
    println!("STRIDE command palette — `>` commands\n");
    for (label, action) in COMMANDS {
        println!("  {label:<width$}  {action}");
    }
    println!("\n{} commands", COMMANDS.len());
}
