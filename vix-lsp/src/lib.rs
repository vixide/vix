//! A pure Language Server Protocol (LSP) client core.
//!
//! This crate owns the *protocol*, not the *process*: JSON-RPC 2.0 message
//! framing ([`frame`]), request/notification builders and response parsers
//! ([`message`]), and char↔encoding column maths ([`position`]). The host spawns
//! the language server, pumps its stdout bytes through [`frame::Decoder`], and
//! writes [`frame::encode`]d requests to its stdin — so everything here stays
//! synchronous and unit-testable with no IO.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod frame;
pub mod message;
pub mod position;

pub use position::Encoding;

/// A zero-based LSP position: `line`, and `character` measured in the negotiated
/// [`Encoding`]'s code units within that line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Position {
    /// Zero-based line.
    pub line: u32,
    /// Zero-based column, in the negotiated encoding's units.
    pub character: u32,
}

/// A half-open `[start, end)` range of [`Position`]s.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Range {
    /// Inclusive start.
    pub start: Position,
    /// Exclusive end.
    pub end: Position,
}

/// Diagnostic severity (LSP numbers 1–4), highest first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    /// An error (1).
    Error,
    /// A warning (2).
    Warning,
    /// An informational note (3).
    Information,
    /// A hint (4).
    Hint,
}

impl Severity {
    /// Map the LSP integer (1–4) to a [`Severity`]; anything else is `Error`.
    #[must_use]
    pub fn from_lsp(n: i64) -> Self {
        match n {
            2 => Severity::Warning,
            3 => Severity::Information,
            4 => Severity::Hint,
            _ => Severity::Error,
        }
    }
}

/// One diagnostic published for a document.
#[derive(Clone, Debug)]
pub struct Diagnostic {
    /// The span the diagnostic covers.
    pub range: Range,
    /// Severity.
    pub severity: Severity,
    /// Human-readable message.
    pub message: String,
    /// Optional source (e.g. `"rustc"`, `"clippy"`).
    pub source: Option<String>,
}

/// One completion candidate.
#[derive(Clone, Debug)]
pub struct CompletionItem {
    /// Label shown in the list.
    pub label: String,
    /// Text inserted when chosen (falls back to the label).
    pub insert_text: String,
    /// Optional secondary detail (type/signature).
    pub detail: Option<String>,
}

/// A source location: a document URI and a range within it.
#[derive(Clone, Debug)]
pub struct Location {
    /// Document URI (`file://…`).
    pub uri: String,
    /// Range within the document.
    pub range: Range,
}
