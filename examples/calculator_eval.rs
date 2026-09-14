//! Evaluate a handful of expressions with the same engine the Tools ->
//! Calculator dialog uses (improvement plan T503) -- `vix_calculator_tool::eval`
//! is a pure `&str -> Result<String, String>` function.
//!
//! Run with: `cargo run --example calculator_eval -- "expression"`
//! With no argument, evaluates a few built-in samples.

#![warn(clippy::pedantic)]

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let exprs: Vec<&str> = if args.is_empty() {
        vec!["2 + 2 * 10", "(3 + 4) / 2", "2^10", "not a formula"]
    } else {
        args.iter().map(String::as_str).collect()
    };

    for expr in exprs {
        match vix::calculator_tool::eval(expr) {
            Ok(result) => println!("{expr} = {result}"),
            Err(e) => println!("{expr} -> error: {e}"),
        }
    }
}
