//! A pure Language Server Protocol (LSP) client core.
//!
//! This crate owns the *protocol*, not the *process*: JSON-RPC 2.0 message
//! framing ([`frame`]), request/notification builders and response parsers
//! ([`message`]), and char↔encoding column maths ([`position`]). The host spawns
//! the language server, pumps its stdout bytes through [`frame::Decoder`], and
//! writes [`frame::encode`]d requests to its stdin — so everything here stays
//! synchronous and unit-testable with no IO.

// `message::initialize_params`'s single `json!` literal (the full advertised
// capabilities object) is large enough that the default limit trips on the
// macro's own expansion depth (T123f, adding a couple more fields pushed it
// over) — a compile-time-only ceiling, unrelated to any runtime recursion.
#![recursion_limit = "256"]

pub mod frame;
pub mod inline_completion;
pub mod message;
pub mod position;
pub mod sync;

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
    /// Secondary locations the server attached (e.g. "previous definition
    /// here" for a duplicate-symbol error) — T134 audit finding: declared
    /// unsupported and never parsed before this, so these were silently
    /// dropped. `(location, message)` pairs, in the order the server sent
    /// them.
    pub related: Vec<(Location, String)>,
}

/// One completion candidate.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct CompletionItem {
    /// Label shown in the list.
    pub label: String,
    /// Text inserted when chosen (falls back to the label).
    pub insert_text: String,
    /// Optional secondary detail (type/signature).
    pub detail: Option<String>,
    /// Opaque server payload round-tripped to `completionItem/resolve` to fetch
    /// fuller detail/documentation lazily.
    pub data: Option<serde_json::Value>,
    /// The range the item's `textEdit` replaces (T612) — a plain `TextEdit`'s
    /// `range`, or an `InsertReplaceEdit`'s `insert` range. `None` when the
    /// server sent no `textEdit`: the host then replaces the typed word.
    pub range: Option<Range>,
    /// The item's `additionalTextEdits` (T612): edits elsewhere in the
    /// document applied together with the item, e.g. an auto-import line.
    pub additional_edits: Vec<(Range, String)>,
    /// `filterText`: what typed text is matched against (falls back to the
    /// label — see [`CompletionItem::filter_key`]).
    pub filter_text: Option<String>,
    /// `sortText`: how the item orders against its peers (falls back to the
    /// label — see [`CompletionItem::sort_key`]).
    pub sort_text: Option<String>,
    /// The LSP `CompletionItemKind` number (1 = Text … 25 = `TypeParameter`).
    pub kind: Option<u32>,
    /// `commitCharacters`: typing one of these while the item is selected
    /// accepts it before the character is inserted.
    pub commit_chars: Vec<String>,
    /// The LSP `insertTextFormat` (T613): `1` plain text, `2` a snippet with
    /// `$1`/`${1:x}`/`$0` tabstops; `None` when the server sent none (plain
    /// text). See [`CompletionItem::is_snippet`].
    pub insert_text_format: Option<u32>,
}

impl CompletionItem {
    /// A plain candidate showing `label` and inserting `insert_text`, with no
    /// range, extra edits, filter/sort text, kind, or commit characters —
    /// what the host's own non-LSP completers (Org-roam node titles,
    /// Org-contacts) build.
    #[must_use]
    pub fn new(label: impl Into<String>, insert_text: impl Into<String>) -> Self {
        CompletionItem {
            label: label.into(),
            insert_text: insert_text.into(),
            ..CompletionItem::default()
        }
    }

    /// The text typed input is fuzzy-matched against: `filterText`, else the
    /// label.
    #[must_use]
    pub fn filter_key(&self) -> &str {
        self.filter_text.as_deref().unwrap_or(&self.label)
    }

    /// The text the item sorts by: `sortText`, else the label.
    #[must_use]
    pub fn sort_key(&self) -> &str {
        self.sort_text.as_deref().unwrap_or(&self.label)
    }

    /// Whether [`CompletionItem::insert_text`] is a snippet body
    /// (`insertTextFormat` 2) rather than plain text.
    #[must_use]
    pub fn is_snippet(&self) -> bool {
        self.insert_text_format == Some(2)
    }

    /// Whether typing `ch` commits (accepts) this item.
    #[must_use]
    pub fn is_commit_char(&self, ch: char) -> bool {
        self.commit_chars
            .iter()
            .any(|c| c.chars().eq(std::iter::once(ch)))
    }
}

/// A `textDocument/completion` result (T612): the candidates plus whether the
/// server considers the list incomplete (`isIncomplete`), meaning further
/// typing should re-request rather than only filter.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct CompletionList {
    /// Candidate items, in server order.
    pub items: Vec<CompletionItem>,
    /// `CompletionList.isIncomplete` (`false` for a bare `CompletionItem[]`).
    pub is_incomplete: bool,
}

impl CompletionList {
    /// A list of `items`, incomplete when `is_incomplete`.
    #[must_use]
    pub fn new(items: Vec<CompletionItem>, is_incomplete: bool) -> Self {
        CompletionList {
            items,
            is_incomplete,
        }
    }
}

/// A source location: a document URI and a range within it.
#[derive(Clone, Debug)]
pub struct Location {
    /// Document URI (`file://…`).
    pub uri: String,
    /// Range within the document.
    pub range: Range,
}

/// One decoded `textDocument/semanticTokens/full` token (T123a): an
/// absolute LSP position (already resolved from the response's
/// delta-encoded form), a code-unit length in the server's negotiated
/// [`Encoding`], and the server's own token-type name (resolved against its
/// `initialize`-time legend — see
/// [`crate::message::parse_semantic_tokens_legend`]). Token *modifiers*
/// (`readonly`, `deprecated`, …) are deliberately not decoded in v1 — see
/// `crates/vix-lsp/spec/index.md`'s T123a note.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticToken {
    /// Zero-based line.
    pub line: u32,
    /// Zero-based column, in the negotiated encoding's units.
    pub character: u32,
    /// Token length, in the negotiated encoding's units.
    pub length: u32,
    /// The server's own name for this token's type (e.g. `"variable"`,
    /// `"function"`), already resolved against the legend.
    pub token_type: String,
}
