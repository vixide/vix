//! A live regular-expression tester, plus the dialog's editing state.
//!
//! The dialog has two fields — the pattern and the subject text — and shows the
//! matches (or the compile error) as the user types. Matching uses the same
//! `regex` engine as Find/Replace. The host renders the [`Tester`]; this module
//! holds the fields and computes [`Tester::result`].

#![warn(clippy::pedantic)]

/// Which field the regex-tester dialog is editing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Field {
    /// The regular-expression pattern.
    #[default]
    Pattern,
    /// The subject text to match against.
    Subject,
}

/// The outcome of evaluating the current pattern against the subject.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The matched substrings (capped), in order.
    Matches(Vec<String>),
    /// The pattern failed to compile, with this message.
    Error(String),
}

/// The regex-tester dialog's editing state.
#[derive(Debug, Clone, Default)]
pub struct Tester {
    /// The pattern text.
    pub pattern: String,
    /// The subject text to search.
    pub subject: String,
    /// The focused field.
    pub focus: Field,
}

impl Tester {
    /// A new tester, optionally seeded with `subject`.
    #[must_use]
    pub fn new(subject: String) -> Self {
        Tester {
            pattern: String::new(),
            subject,
            focus: Field::Pattern,
        }
    }

    /// The focused field's text.
    #[must_use]
    pub fn current(&self) -> &str {
        match self.focus {
            Field::Pattern => &self.pattern,
            Field::Subject => &self.subject,
        }
    }

    /// Switch focus between the two fields.
    pub fn toggle_field(&mut self) {
        self.focus = match self.focus {
            Field::Pattern => Field::Subject,
            Field::Subject => Field::Pattern,
        };
    }

    /// Append a character to the focused field.
    pub fn push(&mut self, c: char) {
        match self.focus {
            Field::Pattern => self.pattern.push(c),
            Field::Subject => self.subject.push(c),
        }
    }

    /// Delete the last character of the focused field.
    pub fn backspace(&mut self) {
        match self.focus {
            Field::Pattern => self.pattern.pop(),
            Field::Subject => self.subject.pop(),
        };
    }

    /// Evaluate the pattern against the subject (capped at 100 matches). An empty
    /// pattern yields no matches; an invalid pattern yields an [`Outcome::Error`].
    #[must_use]
    pub fn result(&self) -> Outcome {
        if self.pattern.is_empty() {
            return Outcome::Matches(Vec::new());
        }
        match regex::Regex::new(&self.pattern) {
            Ok(re) => Outcome::Matches(
                re.find_iter(&self.subject)
                    .take(100)
                    .map(|m| m.as_str().to_string())
                    .collect(),
            ),
            Err(e) => Outcome::Error(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_matches() {
        let mut t = Tester::new("a1 b2 c3".to_string());
        for c in r"\d".chars() {
            t.push(c);
        }
        assert_eq!(
            t.result(),
            Outcome::Matches(vec!["1".into(), "2".into(), "3".into()])
        );
    }

    #[test]
    fn reports_compile_error() {
        let mut t = Tester::new("x".to_string());
        t.push('(');
        assert!(matches!(t.result(), Outcome::Error(_)));
    }

    #[test]
    fn empty_pattern_is_no_matches() {
        let t = Tester::new("anything".to_string());
        assert_eq!(t.result(), Outcome::Matches(Vec::new()));
    }

    #[test]
    fn focus_and_editing() {
        let mut t = Tester::new(String::new());
        t.push('a');
        assert_eq!(t.pattern, "a");
        t.toggle_field();
        t.push('b');
        assert_eq!(t.subject, "b");
        assert_eq!(t.current(), "b");
        t.backspace();
        assert_eq!(t.subject, "");
    }
}
