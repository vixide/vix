//! The text buffer: a [`ropey::Rope`] plus the Tree-sitter parse tree, the
//! highlight queries (with language injections) and the undo/redo [`History`].
//!
//! [`Code`] is the engine under [`crate::editor::Editor`]. Every mutation goes
//! through [`Code::insert`] / [`Code::remove`] (or the batch/undo paths), which
//! keep three things in lockstep: the rope, the Tree-sitter tree (via
//! `InputEdit`), and the history. Small buffers reparse synchronously; buffers
//! over a size threshold reparse on a background `vix-parse` thread and the
//! host installs the result with [`Code::poll_parse`], using an edit generation
//! counter so a superseded parse is never installed. Grammars are compiled in
//! behind the `lang-*` Cargo features; highlight queries are embedded from
//! `langs/`. Offsets in the public API are *character* offsets unless named
//! `byte`. See `crates/vix-editor-core/spec/index.md` and
//! `crates/vix-editor-core/spec/syntax-highlighting/`.

use crate::history::History;
use crate::selection::Selection;
use crate::utils::{calculate_end_position, comment as lang_comment, count_indent_units, indent};
use anyhow::{Result, anyhow};
use ropey::{Rope, RopeSlice};
use rust_embed::RustEmbed;
use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::ControlFlow;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use streaming_iterator::StreamingIterator;
use tree_sitter::{InputEdit, Point, QueryCursor};
use tree_sitter::{Language, Node, ParseOptions, ParseState, Parser, Query, Tree};
use unicode_segmentation::{GraphemeCursor, GraphemeIncomplete};
use unicode_width::UnicodeWidthStr;

/// The bundled per-language `highlights.scm` queries (and injection data),
/// embedded from the crate's `langs/` directory at build time.
#[derive(RustEmbed)]
#[folder = "langs/"]
pub(crate) struct LangAssets;

thread_local! {
    /// Compiled Tree-sitter highlight queries, keyed by `(language, source hash)`.
    ///
    /// `Query::new` compiles a `.scm` query, which for a big grammar (Rust,
    /// TypeScript) costs tens of milliseconds — and it used to run on *every*
    /// [`Code::new`]: every file opened, every preview tab the explorer scans
    /// past with an arrow key, every split, plus one more per injected language.
    /// A compiled query is immutable and identical for a given language and
    /// source, so it is built once and shared.
    ///
    /// Thread-local rather than global because `Code` is already single-threaded
    /// (its injection parsers are `Rc`), which keeps the cache lock-free.
    static QUERY_CACHE: RefCell<HashMap<(String, u64), Rc<Query>>> =
        RefCell::new(HashMap::new());
}

/// The compiled highlight query for `lang`, from the cache when it is there.
///
/// Keyed by the query *source* as well as the language, so a custom-highlight
/// override never collides with the bundled query.
pub(crate) fn compiled_query(lang: &str, language: &Language, source: &str) -> Result<Rc<Query>> {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    let key = (lang.to_string(), hasher.finish());

    if let Some(cached) = QUERY_CACHE.with(|cache| cache.borrow().get(&key).cloned()) {
        return Ok(cached);
    }
    let query = Rc::new(Query::new(language, source)?);
    QUERY_CACHE.with(|cache| cache.borrow_mut().insert(key, Rc::clone(&query)));
    Ok(query)
}

thread_local! {
    /// How many whole-buffer passes ([`Code::get_content`],
    /// [`Code::max_line_chars`]) this thread has made: see [`full_text_scans`].
    static FULL_TEXT_SCANS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// `a - b`, or the declared `fallback` when it would go below zero. The
/// fallback is a parameter so each call names the declaration it comes from;
/// written inline, `checked_sub(..).unwrap_or(0)` is rewritten by Clippy to
/// `saturating_sub`, which the checked-arithmetic spec forbids.
fn sub_or(a: usize, b: usize, fallback: usize) -> usize {
    a.checked_sub(b).unwrap_or(fallback)
}

/// `a + b`, or the declared `fallback` when the sum does not fit (see
/// `sub_or` for why the fallback is a parameter).
fn add_or(a: usize, b: usize, fallback: usize) -> usize {
    a.checked_add(b).unwrap_or(fallback)
}

/// `a * b`, or the declared `fallback` when the product does not fit.
fn mul_or(a: usize, b: usize, fallback: usize) -> usize {
    a.checked_mul(b).unwrap_or(fallback)
}

/// `a + b` for the counters kept here, or the declared `fallback` when the sum does not fit.
fn add_u64_or(a: u64, b: u64, fallback: u64) -> u64 {
    a.checked_add(b).unwrap_or(fallback)
}

/// Bump this thread's [`full_text_scans`] counter (called by the whole-buffer
/// passes only).
fn note_full_text_scan() {
    // A diagnostic counter: after 2^64 scans it starts over at 0.
    FULL_TEXT_SCANS.with(|n| n.set(add_u64_or(n.get(), 1, 0)));
}

/// How many whole-buffer passes — a full copy of the text
/// ([`Code::get_content`]) or a full scan ([`Code::max_line_chars`]) — this
/// thread has made so far. A diagnostic counter for tests: an idle frame or
/// tick on an unchanged buffer must not raise it, because on a 100 MB file
/// each pass costs tens of milliseconds and, for a copy, 100 MB (T749).
#[must_use]
pub fn full_text_scans() -> u64 {
    FULL_TEXT_SCANS.with(std::cell::Cell::get)
}

/// A change event reported to a change callback:
/// `(start_line, start_col, end_line, end_col, text)`.
pub(crate) type ChangeEvent = (usize, usize, usize, usize, String);

/// Tree-sitter injection parsers and queries, each keyed by language name.
type Injections = (
    HashMap<String, Rc<RefCell<Parser>>>,
    HashMap<String, Rc<Query>>,
);

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
    injection_queries: Option<&'a HashMap<String, Rc<Query>>>,
}

impl<T> Clone for HighlightCtx<'_, T> {
    // Hand-written because `#[derive(Clone)]` would wrongly require `T: Clone`;
    // only references to `T` are held, so a bitwise copy is always sound.
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for HighlightCtx<'_, T> {}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
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
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Edit {
    /// The insertion or removal this edit represents.
    pub kind: EditKind,
}

/// A group of edits committed together as one undo/redo step.
#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
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
#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct EditState {
    /// Cursor character offset.
    pub offset: usize,
    /// Active selection, if any.
    pub selection: Option<Selection>,
}

/// Buffers at or above this size (bytes) reparse on a background thread after an
/// edit instead of synchronously, so typing stays responsive on large files.
/// Below it, parsing stays synchronous (identical behavior, no thread).
const ASYNC_PARSE_THRESHOLD: usize = 50_000;

/// Buffers larger than this many bytes (8 MiB) open with Tree-sitter off:
/// no parse tree, so no syntax highlighting, textobjects or structural
/// selection, and [`Code::syntax_off_for_size`] says so (T830).
///
/// Measured on generated Rust (`docs/performance`, § Tree-sitter on very
/// large files): the tree costs about 42 bytes of resident memory per byte of
/// text and the first parse about 140 ms per MiB, so 8 MiB is ~340 MiB and
/// ~1.1 s on the background worker, while 64 MiB would be ~2.7 GiB and ~10 s.
/// The size is checked once, when the buffer is created; a buffer edited past
/// it keeps its tree.
pub const SYNTAX_MAX_BYTES: usize = 8 << 20;

/// A reparse job sent to the background worker.
struct ParseRequest {
    /// Edit generation this snapshot corresponds to (for stale-result rejection).
    generation: u64,
    /// The buffer snapshot to parse (ropey clones share structure — cheap).
    rope: Rope,
    /// The previous tree, for incremental reparsing.
    old: Option<Tree>,
    /// Set to abort this parse when a newer edit supersedes it.
    cancel: Arc<AtomicBool>,
}

/// A completed reparse returned by the worker.
struct ParseResult {
    /// The generation this tree was parsed from.
    generation: u64,
    /// The new tree (`None` if the parse was cancelled or failed).
    tree: Option<Tree>,
}

/// Handle to the background reparse thread (created lazily for large buffers).
struct ParseWorker {
    requests: Sender<ParseRequest>,
    results: Receiver<ParseResult>,
    /// Cancel flag for the in-flight request; replaced when a new request is sent.
    cancel: Arc<AtomicBool>,
    /// Generation of the most recently *sent* request.
    requested: u64,
    /// Generation of the most recently *installed* result.
    installed: u64,
}

/// The background reparse loop: owns a `Parser` for `language` and answers parse
/// requests until the request channel closes (the `Code` was dropped).
fn parse_worker_loop(
    language: &Language,
    requests: &Receiver<ParseRequest>,
    results: &Sender<ParseResult>,
) {
    let mut parser = Parser::new();
    if parser.set_language(language).is_err() {
        return;
    }
    while let Ok(req) = requests.recv() {
        let ParseRequest {
            generation,
            rope,
            old,
            cancel,
        } = req;
        let mut on_progress = |_: &ParseState| {
            if cancel.load(Ordering::Relaxed) {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        };
        let options = ParseOptions::new().progress_callback(&mut on_progress);
        let mut input = |byte: usize, _: Point| -> &[u8] {
            if byte <= rope.len_bytes() {
                let (chunk, start, _, _) = rope.chunk_at_byte(byte);
                chunk
                    .as_bytes()
                    .get(sub_or(byte, start, 0)..)
                    .unwrap_or(&[])
            } else {
                &[]
            }
        };
        let tree = parser.parse_with_options(&mut input, old.as_ref(), Some(options));
        if results.send(ParseResult { generation, tree }).is_err() {
            break;
        }
    }
}

/// The text buffer with Tree-sitter highlighting and undo/redo support.
///
/// Backed by [`ropey::Rope`]: insert/delete/index are O(log n) in the buffer
/// size, and cloning the rope itself is O(1) (structural sharing) — no
/// algorithmic risk scaling to a large file. The one trap is
/// [`Code::get_content`]/[`Code::slice`], which materialize an owned `String`
/// and are genuinely O(n); see their own doc comments before calling either
/// from a per-frame or per-keystroke path. (`Code` as a whole does not
/// implement `Clone` — it owns a background parse worker thread, a
/// `Box<dyn Fn>` change callback, and undo history, none of which would be
/// meaningful to duplicate; only the rope backing it has that property.)
pub struct Code {
    content: ropey::Rope,
    lang: String,
    tree: Option<Tree>,
    parser: Option<Parser>,
    query: Option<Rc<Query>>,
    applying_history: bool,
    history: History,
    current_batch: EditBatch,
    injection_parsers: Option<HashMap<String, Rc<RefCell<Parser>>>>,
    injection_queries: Option<HashMap<String, Rc<Query>>>,
    change_callback: Option<Box<dyn Fn(Vec<ChangeEvent>)>>,
    custom_highlights: Option<HashMap<String, String>>,
    /// Overrides the per-language indent string when set (host configuration).
    indent_override: Option<String>,
    /// Monotonic counter bumped on every content insert/remove, so callers (e.g.
    /// the LSP client) can cheaply detect "the text changed since I last looked".
    revision: u64,
    /// Edit generation, bumped on every `edit_tree`. Async parse results are only
    /// installed when they match the current generation (stale-result rejection).
    edit_gen: u64,
    /// Background reparse worker for large buffers (lazily created on first use).
    parse_worker: Option<ParseWorker>,
    /// Every local content change since the last [`Code::take_edit_log`], when
    /// logging is on (Cowork's editor bridge, T692). `None` = off.
    edit_log: Option<Vec<Splice>>,
    /// True while [`Code::apply_remote`] runs, so remote edits are neither
    /// logged (they would echo back) nor recorded in the undo history.
    applying_remote: bool,
    /// The language has a grammar but the buffer was over
    /// [`SYNTAX_MAX_BYTES`] when created, so Tree-sitter is off for it.
    syntax_off_for_size: bool,
}

/// One content change as a splice: delete `delete` characters at `offset`,
/// then insert `insert`. Offsets are character indices in the buffer as it
/// stood when the change was made, so a log replays in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Splice {
    /// Start, in characters.
    pub offset: usize,
    /// Characters removed first.
    pub delete: usize,
    /// Text inserted after the removal.
    pub insert: String,
}

impl Drop for Code {
    /// Signal any in-flight background parse to stop (T533, Run H). Without
    /// this, closing a tab mid-parse of a very large buffer left the worker
    /// thread running `parser.parse_with_options` to completion purely for a
    /// result nothing would ever read — not a leak (the thread still exits
    /// cleanly once it next tries to send on `results`, and the channel is
    /// already gone with `self`), just wasted CPU on a now-irrelevant parse.
    /// The worker's `progress_callback` (see `parse_worker_loop`) already
    /// polls this same flag periodically to break out early; this only adds
    /// the one missing trigger for it — dropping `Code` itself, not just a
    /// newer request superseding an older one (`request_async_parse` already
    /// handles that case).
    fn drop(&mut self) {
        if let Some(worker) = &self.parse_worker {
            worker.cancel.store(true, Ordering::Relaxed);
        }
    }
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
        Self::from_rope(Rope::from_str(text), lang, custom_highlights)
    }

    /// [`Code::new`] over text already in a rope — built while a file was
    /// read, say — so no `String` copy of it is ever needed (T749).
    ///
    /// # Errors
    ///
    /// As [`Code::new`].
    pub fn from_rope(
        content: Rope,
        lang: &str,
        custom_highlights: Option<HashMap<String, String>>,
    ) -> Result<Self> {
        let len = content.len_bytes();
        let mut code = Self {
            content,
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
            edit_gen: 0,
            parse_worker: None,
            edit_log: None,
            applying_remote: false,
            syntax_off_for_size: false,
        };

        if len > SYNTAX_MAX_BYTES && Self::get_language(lang).is_some() {
            // Too large to parse (T830): plain text, no tree.
            code.syntax_off_for_size = true;
        } else if let Some(language) = Self::get_language(lang) {
            let highlights = code.get_highlights(lang)?;
            let mut parser = Parser::new();
            parser.set_language(&language)?;
            // The *first* parse of a large buffer is exactly as expensive as any
            // other parse of the same size, so it gets the same background-worker
            // treatment `edit_tree` already gives a reparse (T121, plan
            // `docs/performance/index.md`'s "opening 100 MB (5.05s) is the
            // clearest incremental/lazy-highlighting candidate") — skip the
            // synchronous parse and request an async one instead, leaving `tree`
            // `None` (every tree-dependent method already handles that
            // gracefully, returning no highlights/ranges) until `poll_parse`
            // installs the result. Below the threshold, behavior is unchanged.
            let tree = if len < ASYNC_PARSE_THRESHOLD {
                parser.parse(code.content.to_string(), None)
            } else {
                None
            };
            let query = compiled_query(lang, &language, &highlights)?;
            let (iparsers, iqueries) = code.init_injections(&query)?;
            code.tree = tree;
            code.parser = Some(parser);
            code.query = Some(query);
            code.injection_parsers = Some(iparsers);
            code.injection_queries = Some(iqueries);
            if code.tree.is_none() && len >= ASYNC_PARSE_THRESHOLD {
                // `request_async_parse` uses `self.edit_gen` as the request's
                // generation, and a freshly created worker's own `installed`
                // field also starts at 0 -- without this bump, generation 0's
                // request and "nothing installed yet" are indistinguishable,
                // so `parse_pending()` (`requested > installed`) would read
                // `0 > 0` = false even while this very request is in flight.
                // `edit_tree` already sidesteps the same trap by bumping
                // `edit_gen` *before* its own `request_async_parse` call; this
                // mirrors it for the initial parse instead of special-casing
                // generation 0 in `ParseWorker`/`poll_parse`/`parse_pending`.
                code.edit_gen = add_u64_or(code.edit_gen, 1, 0);
                if !code.request_async_parse() {
                    // The background worker failed to start (e.g. thread
                    // spawn failed) -- fall back to a synchronous parse
                    // rather than leaving this buffer permanently
                    // unhighlighted.
                    let text = code.content.to_string();
                    code.tree = code.parser.as_mut().and_then(|p| p.parse(&text, None));
                }
            }
        }

        Ok(code)
    }

    /// The Tree-sitter [`Language`] for the grammar key `lang`, or `None` when
    /// the key is unknown *or* the matching `lang-*` Cargo feature is off.
    pub(crate) fn get_language(lang: &str) -> Option<Language> {
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

    /// The highlight query source for `lang`: a host-supplied custom override
    /// if present, otherwise the bundled `langs/<lang>/highlights.scm`.
    ///
    /// # Errors
    /// No bundled query exists for `lang`, or it is not valid UTF-8.
    fn get_highlights(&self, lang: &str) -> anyhow::Result<String> {
        if let Some(highlights_map) = &self.custom_highlights
            && let Some(highlights) = highlights_map.get(lang)
        {
            return Ok(highlights.clone());
        }
        let p = format!("{lang}/highlights.scm");
        let highlights_bytes =
            LangAssets::get(&p).ok_or_else(|| anyhow!("No highlights found for {lang}"))?;
        let highlights_bytes = highlights_bytes.data.as_ref();
        let highlights = std::str::from_utf8(highlights_bytes)?;
        Ok(highlights.to_string())
    }

    /// Build parsers and queries for every language the main query injects
    /// (capture names of the form `injection.content.<lang>`), one per
    /// distinct language and only for grammars compiled in.
    fn init_injections(&self, query: &Query) -> anyhow::Result<Injections> {
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
                    let inj_query = compiled_query(lang, &language, &highlights)?;

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
        let col = sub_or(offset, line_start, 0);
        (row, col)
    }

    /// Convert a `(row, column)` position into a character offset.
    #[must_use]
    pub fn offset(&self, row: usize, col: usize) -> usize {
        let line_start = self.content.line_to_char(row);
        add_or(line_start, col, usize::MAX)
    }

    /// Return the entire buffer contents as a `String`.
    ///
    /// O(n) in the buffer size — a full materialization of the rope, not the
    /// cheap O(1) structural-sharing clone `Code` itself supports. Fine for a
    /// one-shot, user-invoked action (save, export); avoid calling this from a
    /// per-frame or per-keystroke path without caching the result and keying
    /// the cache on [`Code::revision`] (see T510/T511 in the app's own
    /// `tasks.md` for two real examples of this trap and its fix).
    #[must_use]
    pub fn get_content(&self) -> String {
        note_full_text_scan();
        // One allocation of the exact size. `Rope::to_string` goes through
        // `Display`, which grows the `String` by doubling: for a 100 MB buffer
        // that was ~256 MB of allocations per call, and the freed blocks stay
        // resident on some allocators (T749).
        let mut out = String::with_capacity(self.content.len_bytes());
        for chunk in self.content.chunks() {
            out.push_str(chunk);
        }
        out
    }

    /// The longest line's length in characters, not counting its line break
    /// (`\n` or `\r\n`), as `get_content().lines().map(|l| l.chars().count())
    /// .max()` would compute it — but scanned from the rope's chunks in place,
    /// without copying the text. O(n); callers on a per-frame path cache it on
    /// [`Code::revision`] (see `Editor::max_line_width`).
    #[must_use]
    pub fn max_line_chars(&self) -> usize {
        note_full_text_scan();
        let mut max = 0;
        // Characters so far of a line that continues into the next chunk.
        let mut cur = 0usize;
        let mut ends_cr = false;
        for chunk in self.content.chunks() {
            let mut pieces = chunk.split('\n').peekable();
            while let Some(piece) = pieces.next() {
                let line_ends = pieces.peek().is_some();
                if line_ends && cur == 0 && piece.len() <= max {
                    // A whole line no longer in bytes than the longest so
                    // far can't be longer in characters: skip counting.
                    continue;
                }
                if !piece.is_empty() {
                    // `chars().count()` is std's vectorized count.
                    cur = add_or(cur, piece.chars().count(), usize::MAX);
                    ends_cr = piece.ends_with('\r');
                }
                if line_ends {
                    // A line break follows: the line ends (`\r\n` counts as
                    // the break, as `str::lines` treats it).
                    max = max.max(sub_or(cur, usize::from(ends_cr), 0));
                    cur = 0;
                    ends_cr = false;
                }
            }
        }
        // A last line without a line break: a lone `\r` at its end is text
        // to `str::lines`, so it counts here too.
        max.max(cur)
    }

    /// Return the text between two character offsets as a `String`.
    ///
    /// O(n) in `end - start` — see [`Code::get_content`]'s note on avoiding
    /// this on a per-frame/per-keystroke path.
    ///
    /// # Panics
    ///
    /// Panics if `start..end` is out of bounds for the buffer.
    #[must_use]
    pub fn slice(&self, start: usize, end: usize) -> String {
        self.content.slice(start..end).to_string()
    }

    /// The buffer itself, for read-only walks that must not copy the text
    /// (`Rope::clone` is an O(1) structural share). The Vi motions read
    /// through this so a motion costs its line, not the file (T843).
    #[must_use]
    pub fn rope(&self) -> &Rope {
        &self.content
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
        if add_or(idx, 1, usize::MAX) == self.content.len_lines() {
            len
        } else {
            sub_or(len, 1, 0)
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

    /// The buffer's length in bytes.
    #[must_use]
    pub fn len_bytes(&self) -> usize {
        self.content.len_bytes()
    }

    /// Expand the char range `[start, end)` to the smallest enclosing Tree-sitter
    /// node (structural selection). Returns the node's char range, climbing to the
    /// parent when the range already matches a node exactly so repeated calls keep
    /// growing. `None` without a parse tree or when already at the root.
    #[must_use]
    pub fn expand_to_node(&self, start: usize, end: usize) -> Option<(usize, usize)> {
        let tree = self.tree.as_ref()?;
        let sb = self.content.char_to_byte(start);
        let eb = self.content.char_to_byte(end);
        let mut node = tree.root_node().descendant_for_byte_range(sb, eb)?;
        while node.start_byte() == sb && node.end_byte() == eb {
            node = node.parent()?;
        }
        Some((
            self.content.byte_to_char(node.start_byte()),
            self.content.byte_to_char(node.end_byte()),
        ))
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
        let byte_len: usize = text
            .chars()
            .map(char::len_utf8)
            .fold(0, |total, n| add_or(total, n, usize::MAX));

        self.revision = add_u64_or(self.revision, 1, 0);
        self.content.insert(from, text);
        if !self.applying_remote
            && let Some(log) = self.edit_log.as_mut()
        {
            log.push(Splice {
                offset: from,
                delete: 0,
                insert: text.to_string(),
            });
        }

        if self.applying_history {
            self.current_batch.edits.push(Edit {
                kind: EditKind::Insert {
                    offset: from,
                    text: text.to_string(),
                },
            });
        }

        // `self.parser`, not `self.tree` -- the latter can be `None` while a
        // grammar still applies, when the initial async parse (T121) hasn't
        // landed yet; an edit in that window must still bump the generation
        // and kick a fresh parse (`edit_tree` handles a `None` tree itself),
        // or the eventually-arriving stale, pre-edit result would still
        // match the unchanged generation and get installed as if current.
        if self.parser.is_some() {
            self.edit_tree(InputEdit {
                start_byte: byte_idx,
                old_end_byte: byte_idx,
                new_end_byte: add_or(byte_idx, byte_len, usize::MAX),
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

        self.revision = add_u64_or(self.revision, 1, 0);
        self.content.remove(from..to);
        if !self.applying_remote
            && let Some(log) = self.edit_log.as_mut()
        {
            log.push(Splice {
                offset: from,
                delete: sub_or(to, from, 0),
                insert: String::new(),
            });
        }

        if self.applying_history {
            self.current_batch.edits.push(Edit {
                kind: EditKind::Remove {
                    offset: from,
                    text: removed_text,
                },
            });
        }

        // See the matching comment in `insert` -- `self.parser`, not `self.tree`.
        if self.parser.is_some() {
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

    /// Tell Tree-sitter about `edit` and schedule a reparse: synchronous below
    /// `ASYNC_PARSE_THRESHOLD` bytes (or when no worker can be started),
    /// otherwise on the background worker. Always bumps the edit generation.
    fn edit_tree(&mut self, edit: InputEdit) {
        // Apply the edit to the current tree, if one exists, so its byte
        // offsets stay aligned for any consumer reading it before the
        // reparse completes. `self.tree` can be `None` here even though a
        // grammar applies (`insert`/`remove` gate on `self.parser`, not
        // `self.tree`) when the *initial* async parse (T121) hasn't landed
        // yet -- there's nothing to `.edit()` in that case (there's no old
        // tree to keep aligned), but the generation still needs to move and
        // a fresh parse still needs to happen, exactly as below.
        if let Some(tree) = self.tree.as_mut() {
            tree.edit(&edit);
        }
        self.edit_gen = add_u64_or(self.edit_gen, 1, 0);
        // Small/medium buffers reparse synchronously (identical to the original
        // behavior); large buffers reparse on the background worker so typing
        // stays responsive.
        if self.content.len_bytes() < ASYNC_PARSE_THRESHOLD || !self.request_async_parse() {
            self.reparse();
            // The buffer is now current via a synchronous reparse, so any
            // earlier in-flight async request is superseded. Clear the stale
            // pending flag so `parse_pending` can't latch true and make the
            // host fast-poll forever.
            if let Some(worker) = self.parse_worker.as_mut() {
                worker.requested = worker.installed;
            }
        }
    }

    /// Ensure the background reparse worker exists. Returns `false` (so the caller
    /// falls back to a synchronous reparse) when the language has no grammar.
    fn ensure_parse_worker(&mut self) -> bool {
        if self.parse_worker.is_some() {
            return true;
        }
        let Some(language) = Self::get_language(&self.lang) else {
            return false;
        };
        let (req_tx, req_rx) = std::sync::mpsc::channel::<ParseRequest>();
        let (res_tx, res_rx) = std::sync::mpsc::channel::<ParseResult>();
        let spawned = std::thread::Builder::new()
            .name("vix-parse".to_string())
            .spawn(move || parse_worker_loop(&language, &req_rx, &res_tx))
            .is_ok();
        if !spawned {
            return false;
        }
        self.parse_worker = Some(ParseWorker {
            requests: req_tx,
            results: res_rx,
            cancel: Arc::new(AtomicBool::new(false)),
            requested: 0,
            installed: 0,
        });
        true
    }

    /// Send the current buffer to the background worker for reparsing, cancelling
    /// any in-flight parse. Returns `false` if no worker is available.
    fn request_async_parse(&mut self) -> bool {
        if !self.ensure_parse_worker() {
            return false;
        }
        let rope = self.content.clone();
        let old = self.tree.clone();
        let generation = self.edit_gen;
        let cancel = Arc::new(AtomicBool::new(false));
        let Some(worker) = self.parse_worker.as_mut() else {
            return false;
        };
        worker.cancel.store(true, Ordering::Relaxed); // abort the superseded parse
        worker.cancel = Arc::clone(&cancel);
        worker.requested = generation;
        worker
            .requests
            .send(ParseRequest {
                generation,
                rope,
                old,
                cancel,
            })
            .is_ok()
    }

    /// Install any completed background reparse whose generation is still current.
    /// Returns `true` if the tree was updated (so the host can request a redraw).
    /// A no-op when no worker exists (small buffers parse synchronously).
    pub fn poll_parse(&mut self) -> bool {
        let mut newest: Option<ParseResult> = None;
        if let Some(worker) = self.parse_worker.as_mut() {
            while let Ok(res) = worker.results.try_recv() {
                // Acknowledge delivery of every result (even a superseded or
                // treeless one) by advancing `installed`. Without this, a result
                // that doesn't get installed would leave `requested != installed`
                // and `parse_pending` latched true forever.
                worker.installed = worker.installed.max(res.generation);
                newest = Some(res);
            }
        }
        let Some(res) = newest else { return false };
        // Only install a fresh tree that matches the latest edit (a cancelled or
        // superseded parse has an older generation, or returns no tree).
        if res.generation == self.edit_gen && res.tree.is_some() {
            self.tree = res.tree;
            return true;
        }
        false
    }

    /// Whether a background reparse is in flight (the host polls faster while so).
    ///
    /// Pending means a *newer* generation was requested than has been installed.
    /// Using `>` (not `!=`) keeps this correct even when a late async result
    /// advances `installed` past a `requested` that a synchronous reparse reset —
    /// otherwise the flag could latch true and make the host fast-poll forever.
    #[must_use]
    pub fn parse_pending(&self) -> bool {
        self.parse_worker
            .as_ref()
            .is_some_and(|w| w.requested > w.installed)
    }

    /// Synchronously reparse the whole buffer, feeding Tree-sitter rope chunks
    /// directly (no `String` copy) and reusing the previous tree for
    /// incremental parsing.
    fn reparse(&mut self) {
        if let Some(parser) = self.parser.as_mut() {
            let rope = &self.content;
            self.tree = parser.parse_with_options(
                &mut |byte, _| {
                    if byte <= rope.len_bytes() {
                        let (chunk, start, _, _) = rope.chunk_at_byte(byte);
                        chunk
                            .as_bytes()
                            .get(sub_or(byte, start, 0)..)
                            .unwrap_or(&[])
                    } else {
                        &[]
                    }
                },
                self.tree.as_ref(),
                None,
            );
        }
    }

    /// Whether Tree-sitter is off for this buffer because it was larger than
    /// [`SYNTAX_MAX_BYTES`] when created, although its language has a
    /// grammar: the host says so instead of leaving the plain text unexplained.
    #[must_use]
    pub const fn syntax_off_for_size(&self) -> bool {
        self.syntax_off_for_size
    }

    /// Return `true` if a Tree-sitter highlight query is loaded for this buffer.
    #[must_use]
    pub fn is_highlight(&self) -> bool {
        self.query.is_some()
    }

    /// Whether [`Code::highlight_interval`] can style this buffer: a
    /// Tree-sitter query is loaded, or it is an Org buffer, which has its own
    /// line-based highlighter ([`crate::org_highlight`]).
    #[must_use]
    pub fn has_highlighter(&self) -> bool {
        self.query.is_some() || self.lang == "org"
    }

    /// Highlights the interval between `start` and `end` char indices.
    /// Returns a list of (start byte, end byte, `token_name`) for highlighting.
    ///
    /// # Panics
    ///
    /// Panics if `start` is greater than `end`.
    #[must_use]
    pub fn highlight_interval<T: Copy>(
        &self,
        start: usize,
        end: usize,
        theme: &HashMap<String, T>,
    ) -> Vec<(usize, usize, T)> {
        assert!(start <= end, "Invalid range");

        // Org has no Tree-sitter grammar here: a line-based highlighter.
        if self.query.is_none() && self.lang == "org" {
            return self.org_highlight_interval(start, end, theme);
        }
        let Some(query) = &self.query else {
            return vec![];
        };
        let Some(tree) = &self.tree else {
            return vec![];
        };

        let text = self.content.slice(..);
        let root_node = tree.root_node();

        let ctx = HighlightCtx {
            theme,
            injection_parsers: self.injection_parsers.as_ref(),
            injection_queries: self.injection_queries.as_ref(),
        };
        let mut results = Self::highlight(ctx, text, start, end, query, root_node);

        // Longest span first; ties broken by the third tuple field (query
        // capture index, descending). A painter applying spans in order then
        // lets shorter, more specific spans overwrite the broader ones.
        results.sort_by(|a, b| {
            let len_a = sub_or(a.1, a.0, 0);
            let len_b = sub_or(b.1, b.0, 0);
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

    /// [`Code::highlight_interval`] for an Org buffer: the spans
    /// [`crate::org_highlight::line_spans`] gives each line touching the byte
    /// range `[start, end)`, named entries looked up in `theme`.
    fn org_highlight_interval<T: Copy>(
        &self,
        start: usize,
        end: usize,
        theme: &HashMap<String, T>,
    ) -> Vec<(usize, usize, T)> {
        let total = self.content.len_bytes();
        let (start, end) = (start.min(total), end.min(total));
        if start >= end {
            return vec![];
        }
        let first = self.content.byte_to_line(start);
        let last = self.content.byte_to_line(sub_or(end, 1, 0));
        let mut out = Vec::new();
        for i in first..=last {
            let line_start = self.content.line_to_byte(i);
            let line = self.content.line(i).to_string();
            let text = line.trim_end_matches(['\n', '\r']);
            for (s, e, name) in crate::org_highlight::line_spans(text) {
                if let Some(&value) = theme.get(name) {
                    out.push((
                        add_or(line_start, s, usize::MAX),
                        add_or(line_start, e, usize::MAX),
                        value,
                    ));
                }
            }
        }
        out
    }

    /// Char ranges of `comment` and `string` tokens across the whole buffer,
    /// for spell-checking. Returns an empty list when the language has no
    /// Tree-sitter query or no parse tree.
    #[must_use]
    pub fn comment_string_ranges(&self) -> Vec<(usize, usize)> {
        let Some(query) = &self.query else {
            return vec![];
        };
        let Some(tree) = &self.tree else {
            return vec![];
        };
        let text = self.content.slice(..);
        let root_node = tree.root_node();
        let names = query.capture_names();

        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(query, root_node, RopeProvider(text));
        let mut out = Vec::new();
        while let Some(m) = matches.next() {
            for capture in m.captures() {
                let Some(name) = usize::try_from(capture.index)
                    .ok()
                    .and_then(|i| names.get(i).copied())
                else {
                    continue;
                };
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

    /// Recursive core of [`Code::highlight_interval`]: run `query` over
    /// `root_node` restricted to bytes `start_byte..end_byte`, recursing into
    /// injected languages via `ctx`. Returns `(start, end, capture_index,
    /// value)`, all offsets in bytes of `text`.
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
            for capture in m.captures() {
                let Some(name) = usize::try_from(capture.index)
                    .ok()
                    .and_then(|i| capture_names.get(i).copied())
                else {
                    continue;
                };
                if let Some(value) = ctx.theme.get(name) {
                    results.push((
                        capture.node.start_byte(),
                        capture.node.end_byte(),
                        usize::try_from(capture.index).unwrap_or(usize::MAX),
                        *value,
                    ));
                } else if let Some(lang) = name.strip_prefix("injection.content.") {
                    let Some(injection_parsers) = ctx.injection_parsers else {
                        continue;
                    };
                    let Some(injection_queries) = ctx.injection_queries else {
                        continue;
                    };
                    let Some(parser) = injection_parsers.get(lang) else {
                        continue;
                    };
                    let Some(injection_query) = injection_queries.get(lang) else {
                        continue;
                    };

                    let start = capture.node.start_byte();
                    let end = capture.node.end_byte();
                    let slice = text.byte_slice(start..end);

                    let mut parser = parser.borrow_mut();
                    let Some(inj_tree) = parser.parse(slice.to_string(), None) else {
                        continue;
                    };

                    let injection_results = Self::highlight(
                        ctx,
                        slice,
                        0,
                        sub_or(end, start, 0),
                        injection_query,
                        inj_tree.root_node(),
                    );

                    for (s, e, i, v) in injection_results {
                        results.push((
                            add_or(s, start, usize::MAX),
                            add_or(e, start, usize::MAX),
                            i,
                            v,
                        ));
                    }
                }
            }
        }

        results
    }

    /// Start recording every local content change (idempotent). The log holds
    /// the changes of typing, undo, redo and whole-buffer replacement alike,
    /// but never edits applied with [`Code::apply_remote`].
    pub fn start_edit_log(&mut self) {
        self.edit_log.get_or_insert_with(Vec::new);
    }

    /// Whether the edit log is on (the buffer is shared).
    #[must_use]
    pub const fn is_edit_logging(&self) -> bool {
        self.edit_log.is_some()
    }

    /// Stop recording and discard anything pending.
    pub fn stop_edit_log(&mut self) {
        self.edit_log = None;
    }

    /// Take the changes recorded since the last call (empty when logging is
    /// off).
    pub fn take_edit_log(&mut self) -> Vec<Splice> {
        self.edit_log.as_mut().map_or_default(std::mem::take)
    }

    /// Apply edits that came from another participant: in order, clamped to
    /// the buffer, with no undo-history entry and no log entry.
    pub fn apply_remote(&mut self, edits: &[Splice]) {
        let was_history = std::mem::replace(&mut self.applying_history, false);
        self.applying_remote = true;
        for e in edits {
            let len = self.len();
            let start = e.offset.min(len);
            let end = add_or(start, e.delete, usize::MAX).min(len);
            if end > start {
                self.remove(start, end);
            }
            if !e.insert.is_empty() {
                self.insert(start, &e.insert);
            }
        }
        self.applying_remote = false;
        self.applying_history = was_history;
    }

    /// Undo the last committed batch, reverting its edits; return the batch if any.
    ///
    /// Does nothing while the edit log is on (the buffer is shared, T692):
    /// remote edits leave this history's offsets stale, so a shared buffer's
    /// undo goes through the collaboration layer's per-participant stack.
    pub fn undo(&mut self) -> Option<EditBatch> {
        if self.edit_log.is_some() {
            return None;
        }
        let batch = self.history.undo()?;
        self.applying_history = false;

        for edit in batch.edits.iter().rev() {
            match edit.kind {
                EditKind::Insert { offset, ref text } => {
                    self.remove(offset, add_or(offset, text.chars().count(), usize::MAX));
                }
                EditKind::Remove { offset, ref text } => {
                    self.insert(offset, text);
                }
            }
        }

        self.applying_history = true;
        Some(batch)
    }

    /// Merge every undo state after `seq` into one, so one undo reverts
    /// them together (see [`History::squash_since`]). Refused (`false`)
    /// while the edit log is on, as `undo` is.
    pub fn squash_history_since(&mut self, seq: usize) -> bool {
        if self.edit_log.is_some() {
            return false;
        }
        self.history.squash_since(seq)
    }

    /// Cycle which undo-tree branch `redo` will follow from the current state.
    /// Returns `true` if the current state has more than one branch. See
    /// [`crate::history::History`].
    pub fn switch_undo_branch(&mut self) -> bool {
        self.history.switch_branch()
    }

    /// Make `child` the undo-tree branch the next `redo` follows from the
    /// current state; `false` when it is not a child of the current state.
    pub fn select_redo_branch(&mut self, child: usize) -> bool {
        self.history.select_redo_branch(child)
    }

    /// The undo-tree history, for persisting it (see `crate::undo_store`).
    #[must_use]
    pub fn history(&self) -> &History {
        &self.history
    }

    /// Replace the undo-tree history (restoring a persisted one). The caller must
    /// ensure the buffer content matches the history's current state.
    pub fn set_history(&mut self, history: History) {
        self.history = history;
    }

    /// Redo the next undone batch, reapplying its edits; return the batch if any.
    /// Does nothing while the edit log is on (see [`Code::undo`]).
    pub fn redo(&mut self) -> Option<EditBatch> {
        if self.edit_log.is_some() {
            return None;
        }
        let batch = self.history.redo()?;
        self.applying_history = false;

        for edit in &batch.edits {
            match edit.kind {
                EditKind::Insert { offset, ref text } => {
                    self.insert(offset, text);
                }
                EditKind::Remove { offset, ref text } => {
                    self.remove(offset, add_or(offset, text.chars().count(), usize::MAX));
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
        while let Some(before) = start.checked_sub(1) {
            let c = self.content.char(before);
            if !is_word_char(c) {
                break;
            }
            start = before;
        }

        let mut end = pos;
        while end < len {
            let c = self.content.char(end);
            if !is_word_char(c) {
                break;
            }
            end = add_or(end, 1, usize::MAX);
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
        let end = add_or(start, self.content.line(line).len_chars(), usize::MAX);

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

    /// The current parse tree, if one is installed (`None` without a grammar,
    /// or while a large buffer's first background parse is still running).
    pub(crate) fn syntax_tree(&self) -> Option<&Tree> {
        self.tree.as_ref()
    }

    /// `"CRLF"` if the first line ends with `\r\n`, else `"LF"`.
    #[must_use]
    pub fn first_line_ending(&self) -> &'static str {
        if self.content.len_lines() > 1 {
            let line = self.content.line(0);
            let n = line.len_chars();
            if n >= 2 && line.char(sub_or(n, 2, 0)) == '\r' && line.char(sub_or(n, 1, 0)) == '\n' {
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
        if self.lang == "unknown" || self.lang.is_empty() {
            return 0;
        }
        let line_str = self.line(line);
        count_indent_units(line_str, &self.indent(), Some(col))
    }

    /// Return `true` if only indentation precedes column `c` on line `r`.
    pub fn is_only_indentation_before(&self, r: usize, c: usize) -> bool {
        if self.lang == "unknown" || self.lang.is_empty() {
            return false;
        }
        if r >= self.len_lines() || c == 0 {
            return false;
        }

        let line = self.line(r);
        let indent_unit = self.indent();

        if indent_unit.is_empty() {
            return line.chars().take(c).all(char::is_whitespace);
        }

        let count_units = count_indent_units(line, &indent_unit, Some(c));

        mul_or(count_units, indent_unit.chars().count(), usize::MAX) >= c
    }

    /// Return the column width of the leading indentation on `line_idx`, if any.
    #[must_use]
    pub fn find_indent_at_line_start(&self, line_idx: usize) -> Option<usize> {
        if line_idx >= self.len_lines() {
            return None;
        }

        let line = self.line(line_idx);
        let indent_unit = self.indent();
        if indent_unit.is_empty() {
            return None;
        }

        let count_units = count_indent_units(line, &indent_unit, None);
        let col = mul_or(count_units, indent_unit.chars().count(), usize::MAX);
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
            while let Some(after) = rest.strip_prefix(indent_unit.as_str()) {
                lvl = add_or(lvl, 1, usize::MAX);
                rest = after;
            }
            line_levels.push(lvl);
        }

        let mut result = Vec::with_capacity(lines.len());

        let (Some(first_line), Some(&first_level)) = (lines.first(), line_levels.first()) else {
            return 0;
        };
        result.push(first_line.trim_start().to_string());

        let mut prev_nonempty_level = base_level;
        let mut prev_line_level_in_block = first_level;

        for (&line, &cur) in lines.iter().zip(line_levels.iter()).skip(1) {
            if line.trim().is_empty() {
                result.push(line.to_string());
                continue;
            }

            // diff relative to previous non-empty line in the source block,
            // clamped to a single step in either direction.
            let new_level = match cur.cmp(&prev_line_level_in_block) {
                std::cmp::Ordering::Greater => add_or(prev_nonempty_level, 1, usize::MAX),
                std::cmp::Ordering::Less => sub_or(prev_nonempty_level, 1, 0),
                std::cmp::Ordering::Equal => prev_nonempty_level,
            };
            let indents = indent_unit.repeat(new_level);
            let result_line = format!("{}{}", indents, line.trim_start());
            result.push(result_line);

            // update levels only for non-empty line
            prev_nonempty_level = new_level;
            prev_line_level_in_block = cur;
        }

        // `lines()` drops the block's trailing newline; put it back, so pasting a
        // whole line stays a whole line instead of running into the next paste.
        let mut to_insert = result.join("\n");
        if text.ends_with('\n') {
            to_insert.push('\n');
        }
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
    /// The underlying rope chunk iterator (UTF-8 `&str` pieces).
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
    /// The slice being segmented.
    text: ropey::RopeSlice<'a>,
    /// Remaining chunks after `cur_chunk`.
    chunks: ropey::iter::Chunks<'a>,
    /// The chunk the cursor is currently inside.
    cur_chunk: &'a str,
    /// Byte offset of `cur_chunk` within `text`.
    cur_chunk_start: usize,
    /// Grapheme-boundary state machine; asked for more context when a cluster
    /// straddles a chunk boundary.
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

    /// Yields the next grapheme cluster as a (possibly multi-chunk) rope slice.
    ///
    /// A cluster that runs off the end of a rope chunk is decided on a
    /// *window*: the cluster's text so far followed by the next chunk, as one
    /// string starting at the cluster's own start. `GraphemeCursor` mishandles
    /// the other way of continuing, a chunk that begins exactly where the
    /// cursor now is: it asks for pre-context while it already holds a running
    /// count of regional indicators and adds the two, so a flag whose two
    /// indicators fall either side of a chunk boundary was split in two.
    fn next(&mut self) -> Option<RopeSlice<'a>> {
        let a = self.cursor.cur_cursor();
        // Set once the cluster has run past `cur_chunk`: its text from `a` on,
        // and the byte offset it starts at (`a`).
        let mut window: Option<String> = None;
        let b;
        loop {
            let (chunk, chunk_start) = match window.as_deref() {
                Some(w) => (w, a),
                None => (self.cur_chunk, self.cur_chunk_start),
            };
            match self.cursor.next_boundary(chunk, chunk_start) {
                Ok(None) => {
                    return None;
                }
                Ok(Some(n)) => {
                    b = n;
                    break;
                }
                Err(GraphemeIncomplete::NextChunk) => {
                    // What is already read of this cluster, from `a` to the end
                    // of the chunk the cursor was in.
                    let mut grown =
                        String::from(chunk.get(sub_or(a, chunk_start, 0)..).unwrap_or(""));
                    self.cur_chunk_start =
                        add_or(self.cur_chunk_start, self.cur_chunk.len(), usize::MAX);
                    self.cur_chunk = self.chunks.next().unwrap_or("");
                    grown.push_str(self.cur_chunk);
                    window = Some(grown);
                }
                Err(GraphemeIncomplete::PreContext(idx)) => {
                    // The chunk the cursor asked for must end exactly at `idx`.
                    let (chunk, byte_idx, _, _) = self.text.chunk_at_byte(sub_or(idx, 1, 0));
                    let upto = sub_or(idx, byte_idx, 0);
                    self.cursor
                        .provide_context(chunk.get(..upto).unwrap_or(chunk), byte_idx);
                }
                _ => unreachable!(),
            }
        }

        if a < self.cur_chunk_start {
            let a_char = self.text.byte_to_char(a);
            let b_char = self.text.byte_to_char(b);

            Some(self.text.slice(a_char..b_char))
        } else {
            let a2 = sub_or(a, self.cur_chunk_start, 0);
            let b2 = sub_or(b, self.cur_chunk_start, 0);
            Some(self.cur_chunk.get(a2..b2).unwrap_or("").into())
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

    /// T749: the chunk scan agrees with the `String`-based definition it
    /// replaced, on line breaks, multi-byte text, and a buffer large enough
    /// to span many rope chunks.
    #[test]
    fn max_line_chars_matches_lines_of_the_text() {
        let long = "é".repeat(3000) + "\r\n" + &"x".repeat(10) + "\n";
        // Many `\r\n`s, so some straddle the rope's chunk boundaries.
        let crlf = "abcdefghij\r\n".repeat(3000) + "k\r";
        let short_then_long = "x\r\n".repeat(4000) + &"y".repeat(5000);
        for text in [
            "",
            "\n",
            "abc",
            "abc\n",
            "a\r\nbcd\r\n",
            "ab\r",
            "a\r\r\nb",
            "日本語\nab\n",
            long.as_str(),
            crlf.as_str(),
            short_then_long.as_str(),
        ] {
            let code = Code::new(text, "", None).unwrap();
            let want = text.lines().map(|l| l.chars().count()).max().unwrap_or(0);
            assert_eq!(code.max_line_chars(), want, "{text:?}");
            assert_eq!(code.get_content(), text);
        }
    }

    /// Both whole-buffer passes are counted, so a test can assert that an
    /// idle frame makes none (T749).
    #[test]
    fn whole_buffer_passes_are_counted() {
        let code = Code::new("one\ntwo\n", "", None).unwrap();
        let before = full_text_scans();
        let _ = code.get_content();
        let _ = code.max_line_chars();
        assert_eq!(full_text_scans() - before, 2);
    }

    #[test]
    fn test_insert() {
        let mut code = Code::new("", "", None).unwrap();
        code.insert(0, "Hello ");
        code.insert(6, "World");
        assert_eq!(code.content.to_string(), "Hello World");
    }

    /// Poll `code` until a parse installs (or a generous deadline passes),
    /// returning whether one did. Shared by every test below that needs the
    /// background worker to actually finish before asserting on `code.tree`.
    #[cfg(feature = "lang-rust")]
    fn wait_for_parse(code: &mut Code) -> bool {
        use std::time::{Duration, Instant};
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if code.poll_parse() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        false
    }

    #[cfg(feature = "lang-rust")]
    #[test]
    fn a_large_buffers_initial_parse_is_async_too() {
        // T121: opening a buffer over the threshold must not block on a
        // synchronous whole-buffer parse -- `Code::new` itself returns with
        // no tree yet, exactly like a post-edit async reparse in flight.
        let big = "fn f() { let x = 1; }\n".repeat(3000);
        assert!(
            big.len() > ASYNC_PARSE_THRESHOLD,
            "fixture exceeds the threshold"
        );
        let code = Code::new(&big, "rust", None).unwrap();
        assert!(code.tree.is_none(), "the initial parse hasn't landed yet");
        assert!(
            code.parse_pending(),
            "the initial parse was requested async, not run synchronously"
        );
    }

    #[cfg(feature = "lang-rust")]
    #[test]
    fn a_buffer_over_the_syntax_limit_opens_with_tree_sitter_off() {
        // T830: past `SYNTAX_MAX_BYTES` there is no parse at all, sync or
        // async, so no tree, no highlights and nothing pending.
        let line = "fn f() { let x = 1; }\n";
        let mut big = line.repeat(SYNTAX_MAX_BYTES / line.len());
        big.push_str(&"x".repeat(SYNTAX_MAX_BYTES - big.len()));
        assert_eq!(big.len(), SYNTAX_MAX_BYTES);
        let at_limit = Code::new(&big, "rust", None).unwrap();
        assert!(
            !at_limit.syntax_off_for_size(),
            "the limit itself still parses"
        );
        assert!(at_limit.parse_pending());
        drop(at_limit);

        big.push('\n');
        let mut code = Code::new(&big, "rust", None).unwrap();
        assert!(code.syntax_off_for_size());
        assert!(code.tree.is_none() && !code.is_highlight() && !code.parse_pending());
        let theme: HashMap<String, u8> = [("keyword".to_string(), 1)].into();
        assert_eq!(code.highlight_interval(0, 100, &theme), Vec::new());
        // Editing it is plain-text editing: no reparse is requested.
        code.insert(0, "// edit\n");
        assert!(!code.parse_pending() && code.tree.is_none());
        assert!(code.syntax_off_for_size());

        // A language with no grammar is not "off for size": it never had one.
        assert!(!Code::new(&big, "text", None).unwrap().syntax_off_for_size());
    }

    #[cfg(feature = "lang-rust")]
    #[test]
    fn large_buffer_reparses_on_background_thread() {
        // A buffer over the async threshold reparses off-thread after edits,
        // same as its own initial parse (previous test).
        let big = "fn f() { let x = 1; }\n".repeat(3000);
        assert!(
            big.len() > ASYNC_PARSE_THRESHOLD,
            "fixture exceeds the threshold"
        );
        let mut code = Code::new(&big, "rust", None).unwrap();
        assert!(wait_for_parse(&mut code), "the initial parse landed");
        assert!(code.tree.is_some());

        code.insert(0, "// edit\n");
        // The edited tree stays available (highlighting isn't lost) while the
        // background reparse is in flight.
        assert!(
            code.tree.is_some(),
            "edited tree remains during async reparse"
        );
        assert!(code.parse_pending(), "a background reparse was requested");

        assert!(
            wait_for_parse(&mut code),
            "the background reparse completed and installed"
        );
        assert!(
            !code.parse_pending(),
            "nothing pending once the latest result lands"
        );
        assert_eq!(code.content.char(0), '/', "edit applied");
    }

    #[cfg(feature = "lang-rust")]
    #[test]
    fn an_edit_during_the_initial_async_parse_is_not_silently_dropped() {
        // T121 regression coverage: making an edit *before* the initial
        // parse has landed (`code.tree` still `None`) must still bump the
        // generation and re-request a parse of the *current* content --
        // otherwise the stale, pre-edit parse the initial request already
        // has in flight would land, match the unchanged generation, and get
        // installed as if it reflected the buffer's real (edited) content.
        let big = "fn f() { let x = 1; }\n".repeat(3000);
        let mut code = Code::new(&big, "rust", None).unwrap();
        assert!(code.tree.is_none(), "initial parse still in flight");

        code.insert(0, "// edit\n");
        assert!(wait_for_parse(&mut code), "a parse landed");
        assert!(code.tree.is_some());
        // If the edit had been dropped, this would be the *original* buffer
        // (no leading comment) rather than the edited one.
        assert_eq!(
            code.content.char(0),
            '/',
            "the edit is part of what was parsed"
        );
    }

    #[test]
    fn dropping_code_mid_parse_signals_the_worker_to_cancel() {
        // T533 (Run H): closing a tab (dropping its `Code`) mid-background-
        // parse used to leave the worker thread running the current parse to
        // completion purely for a result nothing would ever read. `Code`'s
        // `Drop` impl now signals the same cancel flag `parse_worker_loop`'s
        // `progress_callback` already polls, so it can break out early
        // instead — this checks the flag transitions on drop, deterministic
        // and independent of how far the worker thread has actually gotten.
        let big = "fn f() { let x = 1; }\n".repeat(3000);
        let code = Code::new(&big, "rust", None).unwrap();
        let cancel = Arc::clone(
            &code
                .parse_worker
                .as_ref()
                .expect("over the async threshold, so the worker exists")
                .cancel,
        );
        assert!(
            !cancel.load(Ordering::Relaxed),
            "not cancelled while `code` is still alive"
        );
        drop(code);
        assert!(
            cancel.load(Ordering::Relaxed),
            "dropping `code` signals the worker to cancel"
        );
    }

    #[cfg(feature = "lang-rust")]
    #[test]
    fn parse_pending_clears_after_a_sync_reparse_supersedes_an_async_one() {
        use std::time::{Duration, Instant};
        // Start over the async threshold so the initial parse itself is async.
        let big = "fn f() { let x = 1; }\n".repeat(3000);
        assert!(big.len() > ASYNC_PARSE_THRESHOLD);
        let mut code = Code::new(&big, "rust", None).unwrap();
        assert!(
            code.parse_pending(),
            "the initial parse was requested async"
        );

        // Shrink the buffer below the threshold: the next edit reparses
        // synchronously and must NOT leave the pending flag latched forever.
        let len = code.content.len_bytes();
        code.remove(0, len - 10);
        assert!(code.content.len_bytes() < ASYNC_PARSE_THRESHOLD);

        // Drain any late async result (there may be more than one still in
        // flight); the flag must settle to false promptly either way.
        let deadline = Instant::now() + Duration::from_secs(5);
        while code.parse_pending() && Instant::now() < deadline {
            code.poll_parse();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            !code.parse_pending(),
            "parse_pending latched true after a superseding sync reparse"
        );
    }

    #[test]
    fn small_buffer_parses_synchronously_without_a_worker() {
        let mut code = Code::new("fn f() {}", "rust", None).unwrap();
        code.insert(0, "// c\n");
        // Below the threshold there is no background worker and nothing pending.
        assert!(!code.parse_pending());
        assert!(!code.poll_parse());
    }

    #[test]
    fn grapheme_iteration_handles_unicode_stress_fixture() {
        // The fixture (test-data/unicode-stress.txt) bundles ZWJ emoji, CJK,
        // combining marks, tabs, RTL, and symbols — the cases TUI text rendering
        // most often gets wrong. Iterating it must never panic, and widths must be
        // grapheme-aware (a ZWJ family is one grapheme; CJK glyphs are 2 cells).
        let fixture = include_str!("../../../test-data/unicode-stress.txt");
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
    fn history_serde_round_trip_restores_undo() {
        use crate::history::History;
        // Record one committed edit ("a" -> "ab").
        let mut code = Code::new("a", "", None).unwrap();
        code.tx();
        code.set_state_before(1, None);
        code.insert(1, "b");
        code.set_state_after(2, None);
        code.commit();
        assert_eq!(code.content.to_string(), "ab");

        // Serialize the undo tree and restore it onto a matching buffer.
        let json = serde_json::to_string(code.history()).unwrap();
        let restored: History = serde_json::from_str(&json).unwrap();
        let mut reopened = Code::new("ab", "", None).unwrap();
        reopened.set_history(restored);

        // Undo on the reopened buffer reverts the persisted edit.
        assert!(
            reopened.undo().is_some(),
            "restored history has an edit to undo"
        );
        assert_eq!(reopened.get_content(), "a");
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

        let expected = "fn foo() {\n    let x = 1;\n    if start == end && start == self.code.len() {\n        return;\n    }\n}";
        assert_eq!(code.get_content(), expected);
    }

    #[test]
    fn test_smart_paste_2() {
        let initial = "fn foo() {\n    let x = 1;\n    \n}";
        let mut code = Code::new(initial, "rust", None).unwrap();

        let offset = 30;
        let paste = "    if start == end && start == self.code.len() {\n        return;\n    }";
        code.smart_paste(offset, paste);

        let expected = "fn foo() {\n    let x = 1;\n    if start == end && start == self.code.len() {\n        return;\n    }\n}";
        assert_eq!(code.get_content(), expected);
    }

    // ----- accessors and coordinate conversions -----------------------------

    #[test]
    fn coordinate_and_size_accessors_agree() {
        let code = Code::new("ab\ncdé\nf", "text", None).unwrap();
        assert_eq!(code.point(4), (1, 1));
        assert_eq!(code.offset(1, 1), 4);
        assert_eq!(code.offset(2, 0), 7);
        assert_eq!(code.len(), 8);
        assert!(!code.is_empty());
        assert_eq!(code.len_bytes(), 9, "é is two bytes");
        assert_eq!(code.byte_to_line(4), 1);
        assert_eq!(code.byte_to_char(7), 6);
        assert_eq!(code.byte_slice(3, 5).to_string(), "cd");
        assert_eq!(code.char_slice(3, 6).to_string(), "cdé");
        assert!(Code::new("", "text", None).unwrap().is_empty());
    }

    #[test]
    fn word_and_line_boundaries() {
        let code = Code::new("foo_bar baz\nqux", "text", None).unwrap();
        assert_eq!(code.word_boundaries(2), (0, 7));
        assert_eq!(code.word_boundaries(7), (0, 7));
        assert_eq!(code.word_boundaries(99), (99, 99), "past the end");
        assert_eq!(code.line_boundaries(2), (0, 12));
        assert_eq!(code.line_boundaries(12), (12, 15));
        assert_eq!(code.line_boundaries(15), (12, 15), "end of buffer");
        assert_eq!(code.line_boundaries(99), (99, 99));
    }

    #[test]
    fn line_ending_comment_and_indent_conventions() {
        let mut code = Code::new("a\r\nb", "rust", None).unwrap();
        assert_eq!(code.first_line_ending(), "CRLF");
        assert_eq!(
            Code::new("a\nb", "rust", None).unwrap().first_line_ending(),
            "LF"
        );
        assert_eq!(
            Code::new("ab", "rust", None).unwrap().first_line_ending(),
            "LF"
        );
        assert_eq!(code.comment(), "//");
        assert_eq!(code.lang(), "rust");
        assert_eq!(code.indent(), "    ");
        code.set_indent(Some("\t".into()));
        assert_eq!(code.indent(), "\t");
        code.set_indent(None);
        assert_eq!(code.indent(), "    ");
    }

    #[test]
    fn indentation_queries_for_unknown_and_empty_indent() {
        let mut unknown = Code::new("    x", "unknown", None).unwrap();
        assert_eq!(unknown.indentation_level(0, 4), 0);
        assert!(!unknown.is_only_indentation_before(0, 4));
        unknown.set_indent(Some(String::new()));
        let mut code = Code::new("   \tx", "rust", None).unwrap();
        code.set_indent(Some(String::new()));
        // With an empty indent unit any leading whitespace counts.
        assert!(code.is_only_indentation_before(0, 4));
        assert!(!code.is_only_indentation_before(0, 5));
        assert_eq!(code.find_indent_at_line_start(0), None);
        // Out-of-range rows and column 0 are never "only indentation".
        let code = Code::new("    x", "rust", None).unwrap();
        assert!(!code.is_only_indentation_before(5, 2));
        assert!(!code.is_only_indentation_before(0, 0));
        assert_eq!(code.find_indent_at_line_start(0), Some(4));
        assert_eq!(code.find_indent_at_line_start(9), None);
        assert_eq!(code.find_indent_at_line_start(0).unwrap(), 4);
    }

    #[test]
    fn smart_paste_edge_cases() {
        // No indent unit: inserted verbatim.
        let mut code = Code::new("ab", "rust", None).unwrap();
        code.set_indent(Some(String::new()));
        assert_eq!(code.smart_paste(1, "x\n  y"), 5);
        assert_eq!(code.get_content(), "ax\n  yb");
        // Empty text pastes nothing.
        let mut code = Code::new("ab", "rust", None).unwrap();
        assert_eq!(code.smart_paste(1, ""), 0);
        assert_eq!(code.get_content(), "ab");
        // A trailing newline is kept; a dedent drops a level; blank lines are
        // inserted as they are.
        let mut code = Code::new("fn f() {\n    \n}", "rust", None).unwrap();
        let n = code.smart_paste(13, "if a {\n    b;\n\n}\n");
        assert_eq!(
            code.get_content(),
            "fn f() {\n    if a {\n        b;\n\n    }\n\n}"
        );
        assert_eq!(n, "if a {\n        b;\n\n    }\n".chars().count().max(n));
    }

    #[test]
    fn expand_to_node_grows_to_the_enclosing_syntax_node() {
        let code = Code::new("fn main() { let x = 1; }", "rust", None).unwrap();
        // "x" (an identifier) grows to its enclosing pattern/statement ranges.
        let (s, e) = code.expand_to_node(16, 17).expect("a node");
        assert!(s <= 16 && e >= 17);
        let (s2, e2) = code.expand_to_node(s, e).expect("a parent");
        assert!(s2 <= s && e2 >= e && (s2, e2) != (s, e));
        // The root has no parent to grow into.
        let all = code.len();
        assert_eq!(code.expand_to_node(0, all), None);
        // Without a grammar there is no tree.
        let plain = Code::new("a b", "text", None).unwrap();
        assert_eq!(plain.expand_to_node(0, 1), None);
    }

    #[test]
    fn change_callback_reports_inserts_and_removals_as_ranges() {
        use std::cell::RefCell;
        use std::rc::Rc;
        let seen: Rc<RefCell<Vec<Vec<ChangeEvent>>>> = Rc::default();
        let sink = Rc::clone(&seen);
        let mut code = Code::new("ab\ncd", "text", None).unwrap();
        code.set_change_callback(Box::new(move |c| sink.borrow_mut().push(c)));
        code.tx();
        code.insert(1, "X");
        code.remove(3, 5);
        code.commit();
        let seen = seen.borrow();
        let all: Vec<_> = seen.iter().flatten().cloned().collect();
        assert!(all.contains(&(0, 1, 0, 1, "X".to_string())), "{all:?}");
        assert!(
            all.contains(&(0, 3, 1, 1, String::new())),
            "a removal spans its start to its computed end with no text: {all:?}"
        );
    }

    #[test]
    fn highlight_interval_recurses_into_injected_languages() {
        let md = "**bold** and `code`\n";
        let code = Code::new(md, "markdown", None).unwrap();
        let mut theme: HashMap<String, u8> = HashMap::new();
        theme.insert("string".to_string(), 1);
        let spans = code.highlight_interval(0, md.len(), &theme);
        // The markdown grammar alone has no string for `**bold**`; the inline
        // injection does, and offsets are mapped back into the whole buffer.
        assert!(
            spans.iter().any(|&(s, e, v)| v == 1 && s == 0 && e == 8),
            "{spans:?}"
        );
        assert!(
            spans.iter().any(|&(s, e, _)| s == 13 && e == 19),
            "code span {spans:?}"
        );
        // A theme without the capture styles nothing, injected or not.
        let empty: HashMap<String, u8> = HashMap::new();
        assert_eq!(code.highlight_interval(0, md.len(), &empty).len(), 0);
    }

    #[test]
    fn highlight_interval_orders_longer_spans_first() {
        let src = "fn main() { let s = \"hi\"; }";
        let code = Code::new(src, "rust", None).unwrap();
        let mut theme: HashMap<String, usize> = HashMap::new();
        for (i, name) in ["function", "keyword", "string", "variable"]
            .iter()
            .enumerate()
        {
            theme.insert((*name).to_string(), i);
        }
        let spans = code.highlight_interval(0, src.len(), &theme);
        assert_ne!(spans.len(), 0);
        let lens: Vec<usize> = spans.iter().map(|s| s.1 - s.0).collect();
        assert!(lens.windows(2).all(|w| w[0] >= w[1]), "{lens:?}");
    }

    #[test]
    fn highlight_interval_without_grammar_or_tree_is_empty() {
        let code = Code::new("plain", "text", None).unwrap();
        let theme: HashMap<String, u8> = HashMap::new();
        assert_eq!(code.highlight_interval(0, 5, &theme).len(), 0);
        assert_eq!(code.comment_string_ranges().len(), 0);
    }

    #[test]
    fn org_buffers_highlight_by_line() {
        let src = "* Heading\nplain\n";
        let code = Code::new(src, "org", None).unwrap();
        let mut theme: HashMap<String, u8> = HashMap::new();
        for name in ["org.heading", "org.heading.1", "type", "keyword", "title"] {
            theme.insert(name.to_string(), 7);
        }
        let all = code.highlight_interval(0, src.len(), &theme);
        let none = code.highlight_interval(5, 5, &theme);
        assert!(none.is_empty(), "an empty byte range highlights nothing");
        // Spans, when the theme names them, never extend past their line.
        assert!(all.iter().all(|&(s, e, _)| s <= e && e <= src.len()));
    }

    #[test]
    fn comment_string_ranges_find_comments_and_strings() {
        let src = "// note\nlet s = \"hello\";\n";
        let code = Code::new(src, "rust", None).unwrap();
        let ranges = code.comment_string_ranges();
        assert!(ranges.contains(&(0, 7)), "line comment: {ranges:?}");
        assert!(
            ranges
                .iter()
                .any(|&(s, e)| &src[s..e] == "\"hello\"" || &src[s..e] == "hello"),
            "string: {ranges:?}"
        );
        // No query -> nothing to scan.
        let plain = Code::new(src, "text", None).unwrap();
        assert_eq!(plain.comment_string_ranges().len(), 0);
    }

    #[test]
    fn custom_highlights_override_the_bundled_query() {
        let mut custom = HashMap::new();
        custom.insert("rust".to_string(), "(identifier) @custom".to_string());
        let code = Code::new("fn main() {}", "rust", Some(custom)).unwrap();
        let mut theme: HashMap<String, u8> = HashMap::new();
        theme.insert("custom".to_string(), 3);
        let spans = code.highlight_interval(0, 12, &theme);
        assert_eq!(spans, vec![(3, 7, 3)]);
    }

    #[test]
    fn graphemes_straddling_rope_chunks_are_kept_whole() {
        // Far more than one rope chunk of text, built from clusters that need
        // context on both sides of a chunk boundary: combining marks and ZWJ
        // emoji, flags (a pair of regional indicators) and Indic conjuncts.
        // The pads move the text across every alignment with the chunk
        // boundaries, including the one that falls between a flag's two halves.
        for unit in [
            "e\u{301}",
            "\u{1F468}\u{200D}\u{1F469}",
            "ab",
            "\u{1F1EB}\u{1F1F7}",
            "\u{1F1EB}\u{1F1F7}\u{1F1E9}\u{1F1EA}\u{1F1EF}",
            "\u{0915}\u{094D}\u{0937}",
        ] {
            for pad in [
                "",
                "x",
                "xy",
                "xyz",
                "xyzw",
                "\u{e9}",
                "\u{e9}x",
                "\u{1F600}",
            ] {
                let text = format!("{pad}{}", unit.repeat(2500));
                let rope = ropey::Rope::from_str(&text);
                let slice = rope.slice(..);
                let got: Vec<String> = RopeGraphemes::new(&slice).map(|g| g.to_string()).collect();
                let want: Vec<&str> =
                    unicode_segmentation::UnicodeSegmentation::graphemes(text.as_str(), true)
                        .collect();
                if got.len() != want.len() {
                    let i = got
                        .iter()
                        .zip(want.iter())
                        .position(|(a, b)| a != b)
                        .unwrap();
                    panic!(
                        "unit {unit:?} pad {pad:?}: first diff at {i}: got {:?} want {:?} (bytes {})",
                        got[i],
                        want[i],
                        got[..i].iter().map(String::len).sum::<usize>()
                    );
                }
                assert!(got.iter().map(String::as_str).eq(want.iter().copied()));
            }
        }
        let empty = ropey::Rope::from_str("");
        assert_eq!(RopeGraphemes::new(&empty.slice(..)).count(), 0);
    }

    #[test]
    fn grapheme_widths_and_lengths() {
        let rope = ropey::Rope::from_str("a\u{4e2d}e\u{301}\t");
        let gs: Vec<_> = RopeGraphemes::new(&rope.slice(..)).collect();
        assert_eq!(gs.len(), 4);
        assert_eq!(grapheme_width_and_chars_len(gs[0]), (1, 1));
        assert_eq!(grapheme_width_and_chars_len(gs[1]), (2, 1));
        assert_eq!(grapheme_width_and_chars_len(gs[2]), (1, 2));
        assert_eq!(grapheme_width_and_bytes_len(gs[1]), (2, 3));
        assert_eq!(grapheme_width_and_bytes_len(gs[2]), (1, 3));
        assert_eq!(grapheme_width(gs[1]), 2);
        // A grapheme spread over several rope chunks takes the allocating path.
        let big = ropey::Rope::from_str(&"x".repeat(5000));
        let spanning = big.slice(..);
        assert!(spanning.as_str().is_none());
        assert_eq!(grapheme_width(spanning), 5000);
        assert_eq!(grapheme_width_and_chars_len(spanning), (5000, 5000));
        assert_eq!(grapheme_width_and_bytes_len(spanning), (5000, 5000));
    }
}
