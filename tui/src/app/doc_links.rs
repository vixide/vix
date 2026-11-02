//! Document colors and links (T621, `crates/vix-lsp/spec`): a language
//! server's `textDocument/documentColor` colors drawn as `■` swatches on the
//! end-of-line virtual-text layer (T614), and its `textDocument/documentLink`
//! links opened by `lsp.open_link` or Ctrl/Cmd+click.
//!
//! Refresh mirrors code lenses: a buffer is asked when first seen, after
//! edits (debounced) and after a save; an unsendable request (server still
//! starting) is retried a bounded number of times. Replies for an older
//! document version never reach this module (`vix-lsp` drops them).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use ratatui::style::Color;

use crate::editor_core::virtual_text::{EolText, VirtualTextLayer};
use crate::lsp_core::document_extras::{DocColor, DocLink, TargetKind, target_kind};

use super::App;
use super::code_lens::{Debounce, LENS_DEBOUNCE};

/// How many times an unsendable request is retried, [`LENS_DEBOUNCE`] apart.
const RETRIES: u8 = 20;

/// One buffer's colors and links, and when to ask again.
#[derive(Debug, Default)]
pub(crate) struct DocExtras {
    /// The server's colors.
    colors: Vec<DocColor>,
    /// The server's links (all have a target).
    links: Vec<DocLink>,
    /// When to re-request.
    refresh: Debounce,
    /// The buffer revision last seen, to notice edits.
    seen_rev: Option<u64>,
    /// Retries spent on an unsendable request.
    retries: u8,
}

impl DocExtras {
    /// Note the buffer is at `rev`: the first sighting makes a refresh due
    /// now, a change restarts the debounce.
    fn observe(&mut self, rev: u64, now: Instant) {
        match self.seen_rev {
            None => self.refresh.now(now),
            Some(r) if r != rev => self.refresh.after(now, LENS_DEBOUNCE),
            Some(_) => {}
        }
        self.seen_rev = Some(rev);
    }
}

/// Per-file colors and links.
pub(crate) type DocExtrasMap = HashMap<PathBuf, DocExtras>;

/// Hand `url` to the platform's opener, detached. Only called for
/// `http(s):` and `mailto:` targets.
#[cfg(feature = "tasks")]
fn open_external(url: &str) -> bool {
    use std::process::{Command, Stdio};
    let mut cmd = if cfg!(target_os = "macos") {
        let mut c = Command::new("open");
        c.arg(url);
        c
    } else if cfg!(windows) {
        let mut c = Command::new("rundll32");
        c.args(["url.dll,FileProtocolHandler", url]);
        c
    } else {
        let mut c = Command::new("xdg-open");
        c.arg(url);
        c
    };
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .is_ok()
}

/// Without the `tasks` capability the build starts no child processes
/// (T595), so an external link cannot be handed to the system opener.
#[cfg(not(feature = "tasks"))]
fn open_external(_url: &str) -> bool {
    false
}

impl App {
    /// Dispatch the link actions. Returns `true` if handled.
    pub(super) fn run_doc_link_action(&mut self, action: &str) -> bool {
        match action {
            "lsp.open_link" => self.open_link_at_cursor(),
            _ => return false,
        }
        true
    }

    /// Per-tick upkeep for the active buffer: notice edits and re-request
    /// colors and links when due. Called from [`App::tick`] after the
    /// language server is synced.
    pub(crate) fn poll_doc_extras(&mut self) {
        let Some(path) = self.active_path() else {
            return;
        };
        let Some(rev) = self.editor.active_tab().map(|t| t.editor.revision()) else {
            return;
        };
        let now = Instant::now();
        let due = {
            let st = self.doc_extras.entry(path.clone()).or_default();
            st.observe(rev, now);
            st.refresh.take(now)
        };
        if !due || !self.lsp.handles(&path) {
            return;
        }
        let sent = self.lsp.refresh_document_extras(&path);
        if !sent
            && let Some(st) = self.doc_extras.get_mut(&path)
            && st.retries < RETRIES
        {
            st.retries += 1;
            st.refresh.after(now, LENS_DEBOUNCE);
        }
    }

    /// Make the colors and links of `path` due for a refresh now (after a
    /// save).
    pub(crate) fn doc_extras_saved(&mut self, path: &Path) {
        if let Some(st) = self.doc_extras.get_mut(path) {
            st.refresh.now(Instant::now());
        }
    }

    /// Store a `documentColor` reply and redraw the swatches.
    pub(crate) fn on_document_colors(&mut self, path: &Path, colors: Vec<DocColor>) {
        self.doc_extras
            .entry(path.to_path_buf())
            .or_default()
            .colors = colors;
        self.apply_color_swatches(path);
    }

    /// Store a `documentLink` reply.
    pub(crate) fn on_document_links(&mut self, path: &Path, links: Vec<DocLink>) {
        self.doc_extras.entry(path.to_path_buf()).or_default().links = links;
    }

    /// Draw `path`'s colors as swatches on its tab, or remove them when the
    /// `document_colors` setting is off.
    fn apply_color_swatches(&mut self, path: &Path) {
        let on = self.settings.language_features.document_colors;
        let texts: Vec<EolText> = if on {
            let mut colors: Vec<&DocColor> = self
                .doc_extras
                .get(path)
                .map(|st| st.colors.iter().collect())
                .unwrap_or_default();
            colors.sort_by_key(|c| (c.range.start.line, c.range.start.character));
            colors
                .into_iter()
                .map(|c| {
                    let (r, g, b) = c.rgb;
                    EolText::new(c.range.start.line as usize, "\u{25a0}")
                        .with_color(Color::Rgb(r, g, b))
                })
                .collect()
        } else {
            Vec::new()
        };
        if let Some(tab) = self
            .editor
            .tabs
            .iter_mut()
            .find(|t| t.path.as_deref() == Some(path))
        {
            tab.editor
                .set_eol_texts(VirtualTextLayer::ColorSwatches, texts);
        }
    }

    /// The link of the active file covering char offset `offset`, or, when
    /// none does and `fallback_line` is given, the only link on that line.
    fn link_at(&self, offset: usize, fallback_line: Option<usize>) -> Option<DocLink> {
        let path = self.active_path()?;
        let st = self.doc_extras.get(&path)?;
        let enc = self.lsp.encoding_for(&path);
        let code = self.editor.active_tab()?.editor.code_ref();
        let covering = st.links.iter().find(|l| {
            let s = super::lsp_pos_to_char(code, l.range.start.line, l.range.start.character, enc);
            let e = super::lsp_pos_to_char(code, l.range.end.line, l.range.end.character, enc);
            (s..e).contains(&offset)
        });
        if let Some(l) = covering {
            return Some(l.clone());
        }
        let line = fallback_line?;
        let mut on_line = st
            .links
            .iter()
            .filter(|l| l.range.start.line as usize == line);
        let first = on_line.next()?;
        on_line.next().is_none().then(|| first.clone())
    }

    /// `lsp.open_link`: open the link under the cursor, or the only link on
    /// its line.
    fn open_link_at_cursor(&mut self) {
        let Some((offset, cursor_line)) = self
            .editor
            .active_tab()
            .map(|t| (t.editor.get_cursor(), t.editor.cursor_line()))
        else {
            return;
        };
        match self.link_at(offset, Some(cursor_line)) {
            Some(link) => self.open_link_target(&link.target),
            None => self.status = t!("status.link_none").to_string(),
        }
    }

    /// Ctrl/Cmd+click at screen cell (`column`, `row`): open the link there.
    /// Returns whether a link was under the pointer (and so the click is
    /// consumed).
    pub(super) fn open_link_at_mouse(&mut self, column: u16, row: u16) -> bool {
        let area = self.layout.editor;
        let Some(offset) = self
            .editor
            .active_tab()
            .and_then(|t| t.editor.cursor_from_mouse(column, row, &area))
        else {
            return false;
        };
        let Some(link) = self.link_at(offset, None) else {
            return false;
        };
        self.open_link_target(&link.target);
        true
    }

    /// Open a link `target` and say what happened.
    pub(crate) fn open_link_target(&mut self, target: &str) {
        match target_kind(target) {
            TargetKind::File { path, line } => {
                let path = crate::lsp::uri_to_path(&format!("file://{path}"));
                self.lsp_jump(&path, line.map_or(0, |l| l.saturating_sub(1)), 0);
                self.status = t!("status.link_opened", target = target.to_string()).to_string();
            }
            TargetKind::External(url) => {
                if open_external(&url) {
                    self.status = t!("status.link_opened", target = url).to_string();
                } else {
                    self.status = t!("status.link_open_failed", target = url).to_string();
                }
            }
            TargetKind::Unsupported => {
                self.status =
                    t!("status.link_unsupported", target = target.to_string()).to_string();
            }
        }
    }
}
