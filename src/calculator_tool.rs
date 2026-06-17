#![allow(clippy::pedantic)] // folded subcrate: kept at its original (non-pedantic) lint level
//! Evaluate a typed formula, plus the Calculator dialog's editing state.
//!
//! [`eval`] runs an arithmetic/boolean expression through `evalexpr` and renders
//! the result as a string (so `2 * (3 + 4)` → `"14"`, `1 > 0` → `"true"`). The
//! [`Calculator`] type holds the dialog's formula text, the focused control, and
//! the last result or error: the host types into it, calls [`Calculator::run`]
//! to evaluate, and inserts the result into the editor.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use evalexpr::Value;

/// Evaluate `formula` and render the result.
///
/// # Errors
/// Returns the evaluator's error message when the formula is malformed or
/// references unknown variables/functions.
pub fn eval(formula: &str) -> Result<String, String> {
    match evalexpr::eval(formula).map_err(|e| e.to_string())? {
        Value::Int(i) => Ok(i.to_string()),
        Value::Float(f) => Ok(f.to_string()),
        Value::Boolean(b) => Ok(b.to_string()),
        Value::String(s) => Ok(s),
        Value::Empty => Ok(String::new()),
        other => Ok(other.to_string()),
    }
}

/// Which Calculator control has focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Focus {
    /// The formula text area.
    #[default]
    Input,
    /// The **Run** button.
    Run,
    /// The **Insert** button (only meaningful once there is a result).
    Insert,
}

/// The outcome of the most recent [`Calculator::run`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The formula evaluated to this display string.
    Ok(String),
    /// The formula failed with this error message.
    Err(String),
}

/// The Calculator dialog's editing state.
#[derive(Debug, Clone, Default)]
pub struct Calculator {
    /// The formula the user is typing.
    pub input: String,
    /// The most recent evaluation outcome, or `None` before the first Run.
    pub outcome: Option<Outcome>,
    /// The focused control.
    pub focus: Focus,
}

impl Calculator {
    /// A fresh calculator with an empty formula focused on the input.
    #[must_use]
    pub fn new() -> Self {
        Calculator::default()
    }

    /// Append a character to the formula and clear any stale outcome.
    pub fn push(&mut self, c: char) {
        self.input.push(c);
        self.outcome = None;
    }

    /// Delete the last character of the formula and clear any stale outcome.
    pub fn backspace(&mut self) {
        self.input.pop();
        self.outcome = None;
    }

    /// Evaluate the current formula, storing the outcome.
    pub fn run(&mut self) {
        if self.input.trim().is_empty() {
            self.outcome = None;
            return;
        }
        self.outcome = Some(match eval(&self.input) {
            Ok(v) => Outcome::Ok(v),
            Err(e) => Outcome::Err(e),
        });
    }

    /// The successful result string, if the last run produced one.
    #[must_use]
    pub fn result(&self) -> Option<&str> {
        match &self.outcome {
            Some(Outcome::Ok(v)) => Some(v),
            _ => None,
        }
    }

    /// Move focus to the next control (Input → Run → Insert → Input).
    pub fn focus_next(&mut self) {
        self.focus = match self.focus {
            Focus::Input => Focus::Run,
            Focus::Run => Focus::Insert,
            Focus::Insert => Focus::Input,
        };
    }

    /// Move focus to the previous control.
    pub fn focus_prev(&mut self) {
        self.focus = match self.focus {
            Focus::Input => Focus::Insert,
            Focus::Run => Focus::Input,
            Focus::Insert => Focus::Run,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluates_arithmetic() {
        assert_eq!(eval("2 * (3 + 4)"), Ok("14".to_string()));
        // Integer operands use integer division; floats give a real quotient.
        assert_eq!(eval("10 / 4"), Ok("2".to_string()));
        assert_eq!(eval("10.0 / 4.0"), Ok("2.5".to_string()));
        assert_eq!(eval("2 > 1"), Ok("true".to_string()));
    }

    #[test]
    fn reports_errors() {
        assert!(eval("2 +").is_err());
        assert!(eval("nope(").is_err());
    }

    #[test]
    fn run_sets_outcome_and_result() {
        let mut c = Calculator::new();
        for ch in "3*3".chars() {
            c.push(ch);
        }
        c.run();
        assert_eq!(c.result(), Some("9"));
        assert_eq!(c.outcome, Some(Outcome::Ok("9".to_string())));
    }

    #[test]
    fn run_on_bad_formula_records_error() {
        let mut c = Calculator::new();
        c.push('(');
        c.run();
        assert!(matches!(c.outcome, Some(Outcome::Err(_))));
        assert_eq!(c.result(), None);
    }

    #[test]
    fn editing_clears_stale_outcome() {
        let mut c = Calculator::new();
        c.push('1');
        c.run();
        assert!(c.outcome.is_some());
        c.push('2');
        assert!(c.outcome.is_none(), "typing invalidates the old result");
    }

    #[test]
    fn focus_cycles() {
        let mut c = Calculator::new();
        assert_eq!(c.focus, Focus::Input);
        c.focus_next();
        assert_eq!(c.focus, Focus::Run);
        c.focus_prev();
        assert_eq!(c.focus, Focus::Input);
    }
}
