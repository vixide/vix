//! Print every command available in the command palette's `>` mode: the i18n
//! label key (translated at runtime via `t!`) and the action id it dispatches.
//!
//! Run with: `cargo run --example list_commands`

#![warn(clippy::pedantic)]

use vix::palette::COMMANDS;

fn main() {
    let width = COMMANDS.iter().map(|(key, _)| key.len()).max().unwrap_or(0);
    println!("Vix command palette — `>` commands\n");
    println!("  {:<width$}  action id", "label key (i18n)");
    println!("  {:<width$}  ---------", "-".repeat(width.min(16)));
    for (label_key, action) in COMMANDS {
        println!("  {label_key:<width$}  {action}");
    }
    println!("\n{} commands", COMMANDS.len());
}
