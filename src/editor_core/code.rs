#![warn(clippy::pedantic)]
use anyhow::{Result, anyhow};
use ropey::{Rope, RopeSlice};
use streaming_iterator::StreamingIterator;
use tree_sitter::{InputEdit, Point, QueryCursor};
use tree_sitter::{Language, Parser, Query, Tree, Node};
use crate::editor_core::history::{History};
use crate::editor_core::selection::Selection;
use rust_embed::RustEmbed;
use std::collections::HashMap;
use crate::editor_core::utils::{indent, count_indent_units, comment as lang_comment, calculate_end_position};
use std::cell::RefCell;
use std::rc::Rc;
use unicode_segmentation::{GraphemeCursor, GraphemeIncomplete};
use unicode_width::{UnicodeWidthStr};

#[derive(RustEmbed)]
#[folder = "langs/"]
struct LangAssets;

/// A change event reported to a change callback:
/// `(start_line, start_col, end_line, end_col, text)`.
pub(crate) type ChangeEvent = (usize, usize, usize, usize, String);

/// Tree-sitter injection parsers and queries, each keyed by language name.
type Injections = (HashMap<String, Rc<RefCell<Parser>>>, HashMap<String, Query>);

/// The pieces of highlighting state that thread unchanged through the recursive
/// [`Code::highlight`] walk: the theme map and the optional injection parser and
/// query maps. Grouping them keeps `highlight` within the argument limit and
/// lets the recursion reuse one value. All fields are references, so the struct
/// is `Copy`.
struct HighlightCtx<'a, T> {
    /// Maps a capture name to the value the host wants applied to that span.
    theme: &'a HashMap<String, T>,
    /// Per-language parsers for embedded (injected) languages, if any.
    injection_parsers: Option<&'a HashMap<String, Rc<RefCell<Parser>>>>,
    /// Per-language highlight queries for embedded languages, if any.
    injection_queries: Option<&'a HashMap<String, Query>>,
}

impl<T> Clone for HighlightCtx<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for HighlightCtx<'_, T> {}

#[derive(Clone)]
/// A single primitive edit: either an insertion or a removal of text.
pub enum EditKind {
    /// Text inserted at a character `offset`.
    Insert {
        /// Character offset where the text was inserted.
        offset: usize,
        /// The inserted text.
        text: String,
    },
    /// Text removed starting at a character `offset`.
    Remove {
        /// Character offset where the text was removed.
        offset: usize,
        /// The removed text (kept for undo).
        text: String,
    },
}

/// A single edit within an [`EditBatch`].
#[derive(Clone)]
pub struct Edit {
    /// The insertion or removal this edit represents.
    pub kind: EditKind,
}

/// A group of edits committed together as one undo/redo step.
#[derive(Clone, Default)]
pub struct EditBatch {
    /// The edits applied in this batch, in order.
    pub edits: Vec<Edit>,
    /// Cursor/selection state before the batch, for restoring on undo.
    pub state_before: Option<EditState>,
    /// Cursor/selection state after the batch, for restoring on redo.
    pub state_after: Option<EditState>,
}

impl EditBatch {
    /// Create an empty edit batch with no recorded state.
    #[must_use] 
    pub fn new() -> Self {
        Self { 
            edits: Vec::new(), 
            state_before: None,
            state_after: None,
        }
    }

}

/// A snapshot of cursor and selection state captured around an edit.
#[derive(Clone, Copy)]
pub struct EditState {
    /// Cursor character offset.
    pub offset: usize,
    /// Active selection, if any.
    pub selection: Option<Selection>,
}


/// The text buffer with Tree-sitter highlighting and undo/redo support.
pub struct Code {
    content: ropey::Rope,
    lang: String,
    tree: Option<Tree>,
    parser: Option<Parser>,
    query: Option<Query>,
    applying_history: bool,
    history: History,
    current_batch: EditBatch,
    injection_parsers: Option<HashMap<String, Rc<RefCell<Parser>>>>,
    injection_queries: Option<HashMap<String, Query>>,
    change_callback: Option<Box<dyn Fn(Vec<ChangeEvent>)>>,
    custom_highlights: Option<HashMap<String, String>>,
    /// Overrides the per-language indent string when set (host configuration).
    indent_override: Option<String>,
    /// Monotonic counter bumped on every content insert/remove, so callers (e.g.
    /// the LSP client) can cheaply detect "the text changed since I last looked".
    revision: u64,
}

impl Code {
    /// Create a new `Code` instance with the given text and language.
    ///
    /// # Errors
    ///
    /// Returns an error if the language's highlight query cannot be loaded,
    /// the Tree-sitter parser cannot be configured for the language, or the
    /// highlight query fails to compile.
    pub fn new(
        text: &str,
        lang: &str,
        custom_highlights: Option<HashMap<String, String>>,
    ) -> Result<Self> {
        let mut code = Self {
            content: Rope::from_str(text),
            lang: lang.to_string(),
            tree: None,
            parser: None,
            query: None,
            applying_history: true,
            history: History::new(1000),
            current_batch: EditBatch::new(),
            injection_parsers: None,
            injection_queries: None,
            change_callback: None,
            custom_highlights,
            indent_override: None,
            revision: 0,
        };

        if let Some(language) = Self::get_language(lang) {
            let highlights = code.get_highlights(lang)?;
            let mut parser = Parser::new();
            parser.set_language(&language)?;
            let tree = parser.parse(text, None);
            let query = Query::new(&language, &highlights)?;
            let (iparsers, iqueries) = code.init_injections(&query)?;
            code.tree = tree;
            code.parser = Some(parser);
            code.query = Some(query);
            code.injection_parsers = Some(iparsers);
            code.injection_queries = Some(iqueries);
        }

        Ok(code)
    }

    fn get_language(lang: &str) -> Option<Language> {
        match lang {
            #[cfg(feature = "lang-rust")]
            "rust" => Some(tree_sitter_rust::LANGUAGE.into()),
            #[cfg(feature = "lang-javascript")]
            "javascript" => Some(tree_sitter_javascript::LANGUAGE.into()),
            #[cfg(feature = "lang-typescript")]
            "typescript" => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
            #[cfg(feature = "lang-python")]
            "python" => Some(tree_sitter_python::LANGUAGE.into()),
            #[cfg(feature = "lang-go")]
            "go" => Some(tree_sitter_go::LANGUAGE.into()),
            #[cfg(feature = "lang-java")]
            "java" => Some(tree_sitter_java::LANGUAGE.into()),
            #[cfg(feature = "lang-c-sharp")]
            "c_sharp" => Some(tree_sitter_c_sharp::LANGUAGE.into()),
            #[cfg(feature = "lang-c")]
            "c" => Some(tree_sitter_c::LANGUAGE.into()),
            #[cfg(feature = "lang-cpp")]
            "cpp" => Some(tree_sitter_cpp::LANGUAGE.into()),
            #[cfg(feature = "lang-html")]
            "html" => Some(tree_sitter_html::LANGUAGE.into()),
            #[cfg(feature = "lang-css")]
            "css" => Some(tree_sitter_css::LANGUAGE.into()),
            #[cfg(feature = "lang-yaml")]
            "yaml" => Some(tree_sitter_yaml::LANGUAGE.into()),
            #[cfg(feature = "lang-json")]
            "json" => Some(tree_sitter_json::LANGUAGE.into()),
            #[cfg(feature = "lang-toml")]
            "toml" => Some(tree_sitter_toml_ng::LANGUAGE.into()),
            #[cfg(feature = "lang-bash")]
            "shell" => Some(tree_sitter_bash::LANGUAGE.into()),
            #[cfg(feature = "lang-markdown")]
            "markdown" => Some(tree_sitter_md::LANGUAGE.into()),
            #[cfg(feature = "lang-markdown")]
            "markdown-inline" => Some(tree_sitter_md::INLINE_LANGUAGE.into()),
            _ => None,
        }
    }

    fn get_highlights(&self, lang: &str) -> anyhow::Result<String> {
        if let Some(highlights_map) = &self.custom_highlights
            && let Some(highlights) = highlights_map.get(lang) {
                return Ok(highlights.clone());
            }
        let p = format!("{lang}/highlights.scm");
        let highlights_bytes =
            LangAssets::get(&p).ok_or_else(|| anyhow!("No highlights found for {lang}"))?;
        let highlights_bytes = highlights_bytes.data.as_ref();
        let highlights = std::str::from_utf8(highlights_bytes)?;
        Ok(highlights.to_string())
    }

    fn init_injections(
        &self,
        query: &Query,
    ) -> anyhow::Result<Injections> {
        let mut injection_parsers = HashMap::new();
        let mut injection_queries = HashMap::new();

        for name in query.capture_names() {
            if let Some(lang) = name.strip_prefix("injection.content.") {
                if injection_parsers.contains_key(lang) {
                    continue;
                }
                if let Some(language) = Self::get_language(lang) {
                    let mut parser = Parser::new();
                    parser.set_language(&language)?;
                    let highlights = self.get_highlights(lang)?;
                    let inj_query = Query::new(&language, &highlights)?;

                    injection_parsers.insert(lang.to_string(), Rc::new(RefCell::new(parser)));
                    injection_queries.insert(lang.to_string(), inj_query);
                } else {
                    // Injection language not compiled in; skip it silently so
                    // we never write to stderr over the TUI.
                }
            }
        }

        Ok((injection_parsers, injection_queries))
    }

    /// Convert a character offset into a `(row, column)` position.
    #[must_use] 
    pub fn point(&self, offset: usize) -> (usize, usize) {
        let row = self.content.char_to_line(offset);
        let line_start = self.content.line_to_char(row);
        let col = offset - line_start;
        (row, col)
    }

    /// Convert a `(row, column)` position into a character offset.
    #[must_use] 
    pub fn offset(&self, row: usize, col: usize) -> usize {
        let line_start = self.content.line_to_char(row);
        line_start + col
    }
    
    /// Return the entire buffer contents as a `String`.
    #[must_use] 
    pub fn get_content(&self) -> String {
        self.content.to_string()
    }
    
    /// Return the text between two character offsets as a `String`.
    ///
    /// # Panics
    ///
    /// Panics if `start..end` is out of bounds for the buffer.
    #[must_use] 
    pub fn slice(&self, start: usize, end: usize) -> String {
        self.content.slice(start..end).to_string()
    }

    /// Return the number of characters in the buffer.
    #[must_use]
    pub fn len(&self) -> usize {
        self.content.len_chars()
    }

    /// Return `true` when the buffer contains no characters.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// A counter that increases on every content insert/remove. Two reads being
    /// equal means the text is unchanged between them (cheap edit detection).
    #[must_use] 
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Return the number of lines in the buffer.
    #[must_use] 
    pub fn len_lines(&self) -> usize {
        self.content.len_lines()
    }

    /// Return the number of characters in the buffer.
    #[must_use] 
    pub fn len_chars(&self) -> usize {
        self.content.len_chars()
    }

    /// Return the character offset of the start of line `line_idx`.
    #[must_use] 
    pub fn line_to_char(&self, line_idx: usize) -> usize {
        self.content.line_to_char(line_idx)
    }
    /// Convert a character index to its byte offset.
    #[must_use] 
    pub fn char_to_byte(&self, char_idx: usize) -> usize {
        self.content.char_to_byte(char_idx)
    }

    /// Return the length of line `idx` in characters, excluding the trailing newline.
    #[must_use] 
    pub fn line_len(&self, idx: usize) -> usize {
        let line = self.content.line(idx);
        let len = line.len_chars();
        if idx == self.content.len_lines() - 1 {
            len
        } else {
            len.saturating_sub(1)
        }
    }
    
    /// Return line `line_idx` as a rope slice.
    #[must_use] 
    pub fn line(&self, line_idx: usize) -> RopeSlice<'_> {
        self.content.line(line_idx)
    }

    /// Return the line index containing character `char_idx`.
    #[must_use] 
    pub fn char_to_line(&self, char_idx: usize) -> usize {
        self.content.char_to_line(char_idx)
    }

    /// Return the text between two character offsets as a rope slice.
    #[must_use] 
    pub fn char_slice(&self, start: usize, end: usize) -> RopeSlice<'_> {
        self.content.slice(start..end)
    }

    /// Return the text between two byte offsets as a rope slice.
    #[must_use] 
    pub fn byte_slice(&self, start: usize, end: usize) -> RopeSlice<'_> {
        self.content.byte_slice(start..end)
    }

    /// Return the line index containing byte `byte_idx`.
    #[must_use] 
    pub fn byte_to_line(&self, byte_idx: usize) -> usize {
        self.content.byte_to_line(byte_idx)
    }

    /// Convert a byte offset to its character index.
    #[must_use] 
    pub fn byte_to_char(&self, byte_idx: usize) -> usize {
        self.content.byte_to_char(byte_idx)
    }

    /// Begin a new edit transaction, discarding any uncommitted batch.
    pub fn tx(&mut self) {
        self.current_batch = EditBatch::new();
    }

    /// Record the cursor/selection state before the current transaction's edits.
    pub fn set_state_before(&mut self, offset: usize, selection: Option<Selection>) {
        self.current_batch.state_before = Some(EditState { offset, selection });
    }

    /// Record the cursor/selection state after the current transaction's edits.
    pub fn set_state_after(&mut self, offset: usize, selection: Option<Selection>) {
        self.current_batch.state_after = Some(EditState { offset, selection });
    }

    /// Commit the current transaction to history and notify change listeners.
    pub fn commit(&mut self) {
        if !self.current_batch.edits.is_empty() {
            self.notify_changes(&self.current_batch.edits);
            self.history.push(self.current_batch.clone());
            self.current_batch = EditBatch::new();
        }
    }
    
    /// Insert `text` at character offset `from`, updating the syntax tree.
    pub fn insert(&mut self, from: usize, text: &str) {
        let byte_idx = self.content.char_to_byte(from);
        let byte_len: usize = text.chars().map(char::len_utf8).sum();

        self.revision = self.revision.wrapping_add(1);
        self.content.insert(from, text);
        
        if self.applying_history {
            self.current_batch.edits.push(Edit {
                kind: EditKind::Insert {
                    offset: from,
                    text: text.to_string(),
                },
            });
        }
        
        if self.tree.is_some() {
            self.edit_tree(InputEdit {
                start_byte: byte_idx,
                old_end_byte: byte_idx,
                new_end_byte: byte_idx + byte_len,
                start_position: Point { row: 0, column: 0 },
                old_end_position: Point { row: 0, column: 0 },
                new_end_position: Point { row: 0, column: 0 },
            });
        }
    }

    /// Remove the characters in `from..to`, updating the syntax tree.
    pub fn remove(&mut self, from: usize, to: usize) {
        let from_byte = self.content.char_to_byte(from);
        let to_byte = self.content.char_to_byte(to);
        let removed_text = self.content.slice(from..to).to_string();

        self.revision = self.revision.wrapping_add(1);
        self.content.remove(from..to);
        
        if self.applying_history {
            self.current_batch.edits.push(Edit {
                kind: EditKind::Remove {
                    offset: from,
                    text: removed_text,
                },
            });
        }
        
        if self.tree.is_some() {
            self.edit_tree(InputEdit {
                start_byte: from_byte,
                old_end_byte: to_byte,
                new_end_byte: from_byte,
                start_position: Point { row: 0, column: 0 },
                old_end_position: Point { row: 0, column: 0 },
                new_end_position: Point { row: 0, column: 0 },
            });
        }
    }

    fn edit_tree(&mut self, edit: InputEdit) {
        if let Some(tree) = self.tree.as_mut() {
            tree.edit(&edit);
            self.reparse();
        }
    }

    fn reparse(&mut self) {
        if let Some(parser) = self.parser.as_mut() {
            let rope = &self.content;
            self.tree = parser.parse_with_options(
                &mut |byte, _| {
                    if byte <= rope.len_bytes() {
                        let (chunk, start, _, _) = rope.chunk_at_byte(byte);
                        &chunk.as_bytes()[byte - start..]
                    } else {
                        &[]
                    }
                },
                self.tree.as_ref(),
                None,
            );
        }
    }

    /// Return `true` if a Tree-sitter highlight query is loaded for this buffer.
    #[must_use] 
    pub fn is_highlight(&self) -> bool {
        self.query.is_some()
    }
    
    /// Highlights the interval between `start` and `end` char indices.
    /// Returns a list of (start byte, end byte, `token_name`) for highlighting.
    ///
    /// # Panics
    ///
    /// Panics if `start` is greater than `end`.
    #[must_use]
    pub fn highlight_interval<T: Copy>(
        &self, start: usize, end: usize, theme: &HashMap<String, T>,
    ) -> Vec<(usize, usize, T)> {
        assert!(start <= end, "Invalid range");

        let Some(query) = &self.query else { return vec![]; };
        let Some(tree) = &self.tree else { return vec![]; };

        let text = self.content.slice(..);
        let root_node = tree.root_node();

        let ctx = HighlightCtx {
            theme,
            injection_parsers: self.injection_parsers.as_ref(),
            injection_queries: self.injection_queries.as_ref(),
        };
        let mut results = Self::highlight(ctx, text, start, end, query, root_node);

        results.sort_by(|a, b| {
            let len_a = a.1 - a.0;
            let len_b = b.1 - b.0;
            match len_b.cmp(&len_a) {
                std::cmp::Ordering::Equal => b.2.cmp(&a.2),
                other => other,
            }
        });

        results
            .into_iter()
            .map(|(start, end, _, value)| (start, end, value))
            .collect()
    }

    /// Char ranges of `comment` and `string` tokens across the whole buffer,
    /// for spell-checking. Returns an empty list when the language has no
    /// Tree-sitter query or no parse tree.
    #[must_use] 
    pub fn comment_string_ranges(&self) -> Vec<(usize, usize)> {
        let Some(query) = &self.query else { return vec![] };
        let Some(tree) = &self.tree else { return vec![] };
        let text = self.content.slice(..);
        let root_node = tree.root_node();
        let names = query.capture_names();

        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(query, root_node, RopeProvider(text));
        let mut out = Vec::new();
        while let Some(m) = matches.next() {
            for capture in m.captures {
                let name = names[capture.index as usize];
                let is_prose = name == "comment"
                    || name.starts_with("comment.")
                    || name == "string"
                    || name.starts_with("string.");
                if is_prose {
                    let sc = self.byte_to_char(capture.node.start_byte());
                    let ec = self.byte_to_char(capture.node.end_byte());
                    if sc < ec {
                        out.push((sc, ec));
                    }
                }
            }
        }
        out
    }

    fn highlight<T: Copy>(
        ctx: HighlightCtx<'_, T>,
        text: RopeSlice<'_>,
        start_byte: usize,
        end_byte: usize,
        query: &Query,
        root_node: Node,
    ) -> Vec<(usize, usize, usize, T)> {
        let mut cursor = QueryCursor::new();
        cursor.set_byte_range(start_byte..end_byte);

        let mut matches = cursor.matches(query, root_node, RopeProvider(text));

        let mut results = Vec::new();
        let capture_names = query.capture_names();

        while let Some(m) = matches.next() {
            for capture in m.captures {
                let name = capture_names[capture.index as usize];
                if let Some(value) = ctx.theme.get(name) {
                    results.push((
                        capture.node.start_byte(),
                        capture.node.end_byte(),
                        capture.index as usize,
                        *value,
                    ));
                } else if let Some(lang) = name.strip_prefix("injection.content.") {
                    let Some(injection_parsers) = ctx.injection_parsers else { continue };
                    let Some(injection_queries) = ctx.injection_queries else { continue };
                    let Some(parser) = injection_parsers.get(lang) else { continue };
                    let Some(injection_query) = injection_queries.get(lang) else { continue };

                    let start = capture.node.start_byte();
                    let end = capture.node.end_byte();
                    let slice = text.byte_slice(start..end);

                    let mut parser = parser.borrow_mut();
                    let Some(inj_tree) = parser.parse(slice.to_string(), None) else { continue };

                    let injection_results = Self::highlight(
                        ctx,
                        slice,
                        0,
                        end - start,
                        injection_query,
                        inj_tree.root_node(),
                    );

                    for (s, e, i, v) in injection_results {
                        results.push((s + start, e + start, i, v));
                    }
                }
            }
        }

        results
    }


    /// Undo the last committed batch, reverting its edits; return the batch if any.
    pub fn undo(&mut self) -> Option<EditBatch> {
        let batch = self.history.undo()?;
        self.applying_history = false;
    
        for edit in batch.edits.iter().rev() {
            match edit.kind {
                EditKind::Insert { offset, ref text } => {
                    self.remove(offset, offset + text.chars().count());
                }
                EditKind::Remove { offset, ref text } => {
                    self.insert(offset, text);
                }
            }
        }
    
        self.applying_history = true;
        Some(batch)
    }
    
    /// Redo the next undone batch, reapplying its edits; return the batch if any.
    pub fn redo(&mut self) -> Option<EditBatch> {
        let batch = self.history.redo()?;
        self.applying_history = false;
    
        for edit in &batch.edits {
            match edit.kind {
                EditKind::Insert { offset, ref text } => {
                    self.insert(offset, text);
                }
                EditKind::Remove { offset, ref text } => {
                    self.remove(offset, offset + text.chars().count());
                }
            }
        }
    
        self.applying_history = true;
        Some(batch)
    }
    
    /// Return the (start, end) character offsets of the word containing `pos`.
    #[must_use] 
    pub fn word_boundaries(&self, pos: usize) -> (usize, usize) {
        let len = self.content.len_chars();
        if pos >= len {
            return (pos, pos);
        }
    
        let is_word_char = |c: char| c.is_alphanumeric() || c == '_';
    
        let mut start = pos;
        while start > 0 {
            let c = self.content.char(start - 1);
            if !is_word_char(c) {
                break;
            }
            start -= 1;
        }
    
        let mut end = pos;
        while end < len {
            let c = self.content.char(end);
            if !is_word_char(c) {
                break;
            }
            end += 1;
        }
    
        (start, end)
    }

    /// Return the (start, end) character offsets of the line containing `pos`.
    #[must_use] 
    pub fn line_boundaries(&self, pos: usize) -> (usize, usize) {
        let total_chars = self.content.len_chars();
        // `pos == total_chars` is the cursor sitting at end-of-buffer; it still
        // belongs to the last line (which has no trailing newline), so let it fall
        // through. Only a `pos` past the end has no line.
        if pos > total_chars {
            return (pos, pos);
        }

        let line = self.content.char_to_line(pos);
        let start = self.content.line_to_char(line);
        let end = start + self.content.line(line).len_chars();

        (start, end)
    }
    
    /// Return the indentation string to use, honoring any override.
    #[must_use] 
    pub fn indent(&self) -> String {
        self.indent_override
            .clone()
            .unwrap_or_else(|| indent(&self.lang))
    }

    /// The language identifier (e.g. `"rust"`, `"text"`).
    #[must_use] 
    pub fn lang(&self) -> &str {
        &self.lang
    }

    /// `"CRLF"` if the first line ends with `\r\n`, else `"LF"`.
    #[must_use] 
    pub fn first_line_ending(&self) -> &'static str {
        if self.content.len_lines() > 1 {
            let line = self.content.line(0);
            let n = line.len_chars();
            if n >= 2 && line.char(n - 2) == '\r' && line.char(n - 1) == '\n' {
                return "CRLF";
            }
        }
        "LF"
    }

    /// Override the indent string inserted by Tab / the `Indent` action (e.g. a
    /// run of spaces, or a tab). `None` restores the per-language default.
    pub fn set_indent(&mut self, indent: Option<String>) {
        self.indent_override = indent;
    }

    /// Return the line-comment prefix for the buffer's language.
    #[must_use] 
    pub fn comment(&self) -> String {
        lang_comment(&self.lang).to_string()
    }

    /// Return the number of indentation units before column `col` on `line`.
    #[must_use] 
    pub fn indentation_level(&self, line: usize, col: usize) -> usize {
        if self.lang == "unknown" || self.lang.is_empty() { return 0; }
        let line_str = self.line(line);
        count_indent_units(line_str, &self.indent(), Some(col))
    }

    /// Return `true` if only indentation precedes column `c` on line `r`.
    pub fn is_only_indentation_before(&self, r: usize, c: usize) -> bool {
        if self.lang == "unknown" || self.lang.is_empty() { return false; }
        if r >= self.len_lines() || c == 0 { return false; }
    
        let line = self.line(r);
        let indent_unit = self.indent();
    
        if indent_unit.is_empty() {
            return line.chars().take(c).all(char::is_whitespace);
        }
    
        let count_units = count_indent_units(line, &indent_unit, Some(c));
        
        count_units * indent_unit.chars().count() >= c
    }

    /// Return the column width of the leading indentation on `line_idx`, if any.
    #[must_use] 
    pub fn find_indent_at_line_start(&self, line_idx: usize) -> Option<usize> {
        if line_idx >= self.len_lines() { return None; }
    
        let line = self.line(line_idx);
        let indent_unit = self.indent();
        if indent_unit.is_empty() { return None; }
    
        let count_units = count_indent_units(line, &indent_unit, None);
        let col = count_units * indent_unit.chars().count();
        if col > 0 { Some(col) } else { None }
    }

    /// Paste text with **indentation awareness**.
    /// 
    /// 1. Determine the indentation level at the cursor (`base_level`).
    /// 2. The first line of the pasted block is inserted at the cursor level (trimmed).
    /// 3. Subsequent lines adjust their indentation **relative to the previous non-empty line in the pasted block**:
    ///    - Compute `diff` = change in indentation from the previous non-empty line in the source block (clamped ±1).
    ///    - Apply `diff` to `prev_nonempty_level` to calculate the new insertion level.
    /// 4. Empty lines are inserted as-is and do not affect subsequent indentation.
    /// 
    /// This ensures that pasted blocks keep their relative structure while aligning to the cursor.
    ///
    /// Inserts `text` with indentation-awareness at `offset`. Returns the number
    /// of characters inserted.
    pub fn smart_paste(&mut self, offset: usize, text: &str) -> usize {
        let (row, col) = self.point(offset);
        let base_level = self.indentation_level(row, col);
        let indent_unit = self.indent();

        if indent_unit.is_empty() {
            self.insert(offset, text);
            return text.chars().count();
        }

        let lines: Vec<&str> = text.lines().collect();
        if lines.is_empty() {
            return 0;
        }

        // Compute indentation levels of all lines in the source block
        let mut line_levels = Vec::with_capacity(lines.len());
        for line in &lines {
            let mut lvl = 0;
            let mut rest = *line;
            while rest.starts_with(&indent_unit) {
                lvl += 1;
                rest = &rest[indent_unit.len()..];
            }
            line_levels.push(lvl);
        }

        let mut result = Vec::with_capacity(lines.len());

        let first_line_trimmed = lines[0].trim_start();
        result.push(first_line_trimmed.to_string());

        let mut prev_nonempty_level = base_level;
        let mut prev_line_level_in_block = line_levels[0];

        for i in 1..lines.len() {
            let line = lines[i];

            if line.trim().is_empty() {
                result.push(line.to_string());
                continue;
            }

            // diff relative to previous non-empty line in the source block,
            // clamped to a single step in either direction.
            let cur = line_levels[i];
            let new_level = match cur.cmp(&prev_line_level_in_block) {
                std::cmp::Ordering::Greater => prev_nonempty_level + 1,
                std::cmp::Ordering::Less => prev_nonempty_level.saturating_sub(1),
                std::cmp::Ordering::Equal => prev_nonempty_level,
            };
            let indents = indent_unit.repeat(new_level);
            let result_line = format!("{}{}", indents, line.trim_start());
            result.push(result_line);

            // update levels only for non-empty line
            prev_nonempty_level = new_level;
            prev_line_level_in_block = line_levels[i];
        }

        let to_insert = result.join("\n");
        self.insert(offset, &to_insert);
        to_insert.chars().count()
    }

    /// Set the change callback function for handling document changes
    pub fn set_change_callback(&mut self, callback: Box<dyn Fn(Vec<ChangeEvent>)>) {
        self.change_callback = Some(callback);
    }

    /// Notify about document changes
    fn notify_changes(&self, edits: &[Edit]) {
        if let Some(callback) = &self.change_callback {
            let mut changes = Vec::new();
            
            for edit in edits {
                match &edit.kind {
                    EditKind::Insert { offset, text } => {
                        let (start_row, start_col) = self.point(*offset);
                        changes.push((start_row, start_col, start_row, start_col, text.clone()));
                    }
                    EditKind::Remove { offset, text } => {
                        let (start_row, start_col) = self.point(*offset);
                        let (end_row, end_col) = calculate_end_position(start_row, start_col, text);
                        changes.push((start_row, start_col, end_row, end_col, String::new()));
                    }
                }
            }
            
            if !changes.is_empty() {
                callback(changes);
            }
        }
    }
    
}

/// An iterator over byte slices of Rope chunks.
/// This is used to feed `tree-sitter` without allocating a full `String`.
pub struct ChunksBytes<'a> {
    chunks: ropey::iter::Chunks<'a>,
}

impl<'a> Iterator for ChunksBytes<'a> {
    type Item = &'a [u8];

    /// Returns the next chunk as a byte slice.
    /// Internally converts a `&str` to a `&[u8]` without allocation.
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.chunks.next().map(str::as_bytes)
    }
}

/// A lightweight wrapper around a `RopeSlice`
/// that implements `tree_sitter::TextProvider`.
/// This allows using `tree-sitter`'s `QueryCursor::matches`
/// directly on a `Rope` without converting it to a `String`.
pub struct RopeProvider<'a>(pub RopeSlice<'a>);

impl<'a> tree_sitter::TextProvider<&'a [u8]> for RopeProvider<'a> {
    type I = ChunksBytes<'a>;

    /// Provides an iterator over chunks of text corresponding to the given node.
    /// This avoids allocation by working directly with Rope slices.
    #[inline]
    fn text(&mut self, node: tree_sitter::Node) -> Self::I {
        let fragment = self.0.byte_slice(node.start_byte()..node.end_byte());
        ChunksBytes {
            chunks: fragment.chunks(),
        }
    }
}

/// An implementation of a graphemes iterator, for iterating over the graphemes of a `RopeSlice`.
pub struct RopeGraphemes<'a> {
    text: ropey::RopeSlice<'a>,
    chunks: ropey::iter::Chunks<'a>,
    cur_chunk: &'a str,
    cur_chunk_start: usize,
    cursor: GraphemeCursor,
}

impl RopeGraphemes<'_> {
    /// Create an iterator over the grapheme clusters of `slice`.
    #[must_use] 
    pub fn new<'b>(slice: &RopeSlice<'b>) -> RopeGraphemes<'b> {
        let mut chunks = slice.chunks();
        let first_chunk = chunks.next().unwrap_or("");
        RopeGraphemes {
            text: *slice,
            chunks,
            cur_chunk: first_chunk,
            cur_chunk_start: 0,
            cursor: GraphemeCursor::new(0, slice.len_bytes(), true),
        }
    }
}

impl<'a> Iterator for RopeGraphemes<'a> {
    type Item = RopeSlice<'a>;

    fn next(&mut self) -> Option<RopeSlice<'a>> {
        let a = self.cursor.cur_cursor();
        let b;
        loop {
            match self
                .cursor
                .next_boundary(self.cur_chunk, self.cur_chunk_start)
            {
                Ok(None) => {
                    return None;
                }
                Ok(Some(n)) => {
                    b = n;
                    break;
                }
                Err(GraphemeIncomplete::NextChunk) => {
                    self.cur_chunk_start += self.cur_chunk.len();
                    self.cur_chunk = self.chunks.next().unwrap_or("");
                }
                Err(GraphemeIncomplete::PreContext(idx)) => {
                    let (chunk, byte_idx, _, _) = self.text.chunk_at_byte(idx.saturating_sub(1));
                    self.cursor.provide_context(chunk, byte_idx);
                }
                _ => unreachable!(),
            }
        }

        if a < self.cur_chunk_start {
            let a_char = self.text.byte_to_char(a);
            let b_char = self.text.byte_to_char(b);

            Some(self.text.slice(a_char..b_char))
        } else {
            let a2 = a - self.cur_chunk_start;
            let b2 = b - self.cur_chunk_start;
            Some((&self.cur_chunk[a2..b2]).into())
        }
    }
}

/// Return the display width and character count of a single grapheme.
#[must_use] 
pub fn grapheme_width_and_chars_len(g: RopeSlice) -> (usize, usize) {
    if let Some(g_str) = g.as_str() {
        (UnicodeWidthStr::width(g_str), g_str.chars().count())
    } else {
        let g_string = g.to_string();
        let g_str = g_string.as_str();
        (UnicodeWidthStr::width(g_str), g_str.chars().count())
    }
}

/// Return the display width and byte length of a single grapheme.
#[must_use] 
pub fn grapheme_width_and_bytes_len(g: RopeSlice) -> (usize, usize) {
    if let Some(g_str) = g.as_str() {
        (UnicodeWidthStr::width(g_str), g_str.len())
    } else {
        let g_string = g.to_string();
        let g_str = g_string.as_str();
        (UnicodeWidthStr::width(g_str), g_str.len())
    }
}

/// Return the terminal display width of a single grapheme.
#[must_use] 
pub fn grapheme_width(g: RopeSlice) -> usize {
    if let Some(s) = g.as_str() {
        UnicodeWidthStr::width(s)
    } else {
        let s = g.to_string();
        UnicodeWidthStr::width(s.as_str())
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert() {
        let mut code = Code::new("", "", None).unwrap();
        code.insert(0, "Hello ");
        code.insert(6, "World");
        assert_eq!(code.content.to_string(), "Hello World");
    }

    #[test]
    fn grapheme_iteration_handles_unicode_stress_fixture() {
        // The fixture (test-data/unicode-stress.txt) bundles ZWJ emoji, CJK,
        // combining marks, tabs, RTL, and symbols — the cases TUI text rendering
        // most often gets wrong. Iterating it must never panic, and widths must be
        // grapheme-aware (a ZWJ family is one grapheme; CJK glyphs are 2 cells).
        let fixture = include_str!("../../test-data/unicode-stress.txt");
        let rope = Rope::from_str(fixture);
        for line_idx in 0..rope.len_lines() {
            let line = rope.line(line_idx);
            // Width sum must not panic and must be finite for every line.
            let _w: usize = RopeGraphemes::new(&line).map(grapheme_width).sum();
        }
        // The ZWJ family is a single grapheme cluster, not its 7 codepoints.
        let family = Rope::from_str("👨‍👨‍👧‍👧");
        assert_eq!(RopeGraphemes::new(&family.slice(..)).count(), 1);
        // A CJK ideograph occupies two terminal cells.
        let cjk = Rope::from_str("日");
        assert_eq!(grapheme_width(cjk.slice(..)), 2);
    }

    #[test]
    fn test_remove() {
        let mut code = Code::new("Hello World", "", None).unwrap();
        code.remove(5, 11);
        assert_eq!(code.content.to_string(), "Hello");
    }

    #[test]
    fn test_undo() {
        let mut code = Code::new("", "", None).unwrap();

        code.tx();
        code.insert(0, "Hello ");
        code.commit();

        code.tx();
        code.insert(6, "World");
        code.commit();

        code.undo();
        assert_eq!(code.content.to_string(), "Hello ");

        code.undo();
        assert_eq!(code.content.to_string(), "");
    }

    #[test]
    fn test_redo() {
        let mut code = Code::new("", "", None).unwrap();

        code.tx();
        code.insert(0, "Hello");
        code.commit();

        code.undo();
        assert_eq!(code.content.to_string(), "");

        code.redo();
        assert_eq!(code.content.to_string(), "Hello");
    }

    #[test]
    fn test_indentation_level0() {
        let mut code = Code::new("", "unknown", None).unwrap();
        code.insert(0, "    hello world");
        assert_eq!(code.indentation_level(0, 10), 0);
    }

    #[test]
    fn test_indentation_level() {
        let mut code = Code::new("", "python", None).unwrap();
        code.insert(0, "    print('Hello, World!')");
        assert_eq!(code.indentation_level(0, 10), 1);
    }

    #[test]
    fn test_indentation_level2() {
        let mut code = Code::new("", "python", None).unwrap();
        code.insert(0, "        print('Hello, World!')");
        assert_eq!(code.indentation_level(0, 10), 2);
    }

    #[test]
    fn test_is_only_indentation_before() {
        let mut code = Code::new("", "python", None).unwrap();
        code.insert(0, "    print('Hello, World!')");
        assert!(code.is_only_indentation_before(0, 4));
        assert!(!code.is_only_indentation_before(0, 10));
    }

    #[test]
    fn test_is_only_indentation_before2() {
        let mut code = Code::new("", "", None).unwrap();
        code.insert(0, "    Hello, World");
        assert!(!code.is_only_indentation_before(0, 4));
        assert!(!code.is_only_indentation_before(0, 10));
    }

    #[test]
    fn test_smart_paste_1() {
        let initial = "fn foo() {\n    let x = 1;\n    \n}";
        let mut code = Code::new(initial, "rust", None).unwrap();

        let offset = 30;
        let paste = "if start == end && start == self.code.len() {\n    return;\n}";
        code.smart_paste(offset, paste);

        let expected =
            "fn foo() {\n    let x = 1;\n    if start == end && start == self.code.len() {\n        return;\n    }\n}";
        assert_eq!(code.get_content(), expected);
    }

    #[test]
    fn test_smart_paste_2() {
        let initial = "fn foo() {\n    let x = 1;\n    \n}";
        let mut code = Code::new(initial, "rust", None).unwrap();

        let offset = 30;
        let paste = "    if start == end && start == self.code.len() {\n        return;\n    }";
        code.smart_paste(offset, paste);

        let expected =
            "fn foo() {\n    let x = 1;\n    if start == end && start == self.code.len() {\n        return;\n    }\n}";
        assert_eq!(code.get_content(), expected);
    }
}
