//! Where a run's results go besides the dock (T734e, T734g): an Org block's
//! `#+RESULTS:` (written, appended to or cleared as one undo step), an
//! image saved for an Org `[[file:…]]` link, and an image opened in a tab.

use std::path::{Component, Path};

use vix_jupyter::core::babel::{self, Handling};
use vix_jupyter::core::image::{self as kernel_image, Image, ImageError};
use vix_jupyter::core::message::ReplyStatus;
use vix_jupyter::core::org::{self, ResultValue};

use super::InFlight;
use crate::app::App;

impl App {
    /// Write an Org run's `#+RESULTS:` (T734e) into its buffer, following
    /// the block's `:results` and `:file` header arguments. Nothing is
    /// written for a run the kernel did not run (`aborted`), or when the
    /// block can no longer be found.
    pub(super) fn jupyter_write_results(&mut self, run: &InFlight, status: ReplyStatus) {
        let Some(org_run) = &run.org else {
            return;
        };
        if status == ReplyStatus::Aborted {
            return;
        }
        let Some(index) = self.jupyter_tab_index(run.path.as_ref(), run.tab) else {
            return;
        };
        let text = self.editor.tabs[index].editor.get_content();
        let Some(end_src) = locate_block(&text, org_run.begin, &run.code) else {
            self.messages
                .warn(t!("msg.jupyter_block_moved").to_string());
            return;
        };
        let header = &org_run.header;
        let value = match babel::first_image(&run.outputs) {
            Some((mime, b64)) => match self.jupyter_save_org_image(
                run.path.as_deref(),
                header.file.as_deref(),
                mime,
                &b64,
            ) {
                Some(link) => ResultValue::File(link),
                None => ResultValue::Text(babel::result_text(&run.outputs, header.collect)),
            },
            None => ResultValue::Text(babel::result_text(&run.outputs, header.collect)),
        };
        let block = org::results_block(&value);
        let edit = if header.handling == Handling::Append {
            org::results_append_edit(&text, end_src, &block)
        } else {
            org::results_edit(&text, end_src, Some(&block))
        };
        self.jupyter_apply_edit(index, &text, &edit);
    }

    /// **Clear Results** in an Org `src` block: remove its `#+RESULTS:`,
    /// as one undo step. Returns whether the cursor was in a block.
    pub(super) fn jupyter_clear_org_results(&mut self) -> bool {
        let index = self.editor.active;
        let Some(tab) = self.editor.tabs.get(index).filter(|t| !t.is_image()) else {
            return false;
        };
        let text = tab.editor.get_content();
        let Some((_, end_src, _)) = crate::org::src_block_at(&text, tab.editor.cursor_line())
        else {
            return false;
        };
        if org::find_results(&text, end_src).is_some() {
            let edit = org::results_edit(&text, end_src, None);
            self.jupyter_apply_edit(index, &text, &edit);
        }
        true
    }

    /// Apply `edit` to tab `index` (whose text is `text`) as one undo step,
    /// keeping the cursor on the same text.
    fn jupyter_apply_edit(&mut self, index: usize, text: &str, edit: &org::ResultsEdit) {
        let new = org::apply(text, edit);
        if new == text {
            return;
        }
        let start = char_offset_of_line(text, edit.start);
        let old_end = char_offset_of_line(text, edit.end);
        let delta = new.chars().count().cast_signed() - text.chars().count().cast_signed();
        let tab = &mut self.editor.tabs[index];
        let cursor = tab.editor.get_cursor();
        tab.editor.set_content(&new);
        let cursor = if cursor < start {
            cursor
        } else if cursor >= old_end {
            cursor.saturating_add_signed(delta)
        } else {
            start
        };
        tab.editor.set_cursor(cursor);
        tab.dirty = true;
    }

    /// Save an Org run's image for its `[[file:…]]` link: to the block's
    /// `:file` path, or `<org-file-stem>-images/<hash>.<ext>`, both relative
    /// to the Org file's folder. Returns the link target, or `None` (with a
    /// dock line or a message saying why) when nothing was saved.
    fn jupyter_save_org_image(
        &mut self,
        org_path: Option<&Path>,
        file: Option<&str>,
        mime: &str,
        b64: &str,
    ) -> Option<String> {
        let image = match kernel_image::decode(mime, b64) {
            Ok(image) => image,
            Err(e) => {
                self.bottom_dock.push(refused_line(mime, &e));
                return None;
            }
        };
        let dir = org_path
            .and_then(Path::parent)
            .map_or_else(|| self.root.clone(), Path::to_path_buf);
        let link = if let Some(file) = file {
            if !is_contained(file) {
                self.messages
                    .warn(t!("msg.jupyter_file_refused", path = file).to_string());
                return None;
            }
            file.to_owned()
        } else {
            let stem = org_path
                .and_then(Path::file_stem)
                .map_or_else(|| "org".into(), |s| s.to_string_lossy().into_owned());
            format!("{stem}-images/{}", image.file_name())
        };
        let target = dir.join(&link);
        if let Err(e) = write_file(&target, &image) {
            self.messages.error(
                t!(
                    "msg.jupyter_file_write_failed",
                    path = target.display(),
                    error = e
                )
                .to_string(),
            );
            return None;
        }
        self.bottom_dock.push(
            t!(
                "ui.jupyter_image_saved",
                mime = image.mime,
                path = target.display()
            )
            .to_string(),
        );
        Some(link)
    }

    /// Show an image a run sent (T734g) and return the dock line about it:
    /// opened in an image tab, refused (too large or invalid), or not
    /// shown (no `image` capability, or a terminal without graphics).
    pub(super) fn jupyter_show_image(&mut self, mime: &str, b64: &str) -> String {
        match kernel_image::decode(mime, b64) {
            Ok(image) => self.jupyter_open_image(&image),
            Err(e) => refused_line(mime, &e),
        }
    }

    /// Open `image` in a tab from a private scratch directory.
    #[cfg(feature = "image")]
    fn jupyter_open_image(&mut self, image: &Image) -> String {
        let placeholder = || t!("ui.jupyter_image_placeholder", mime = image.mime).to_string();
        if self.jupyter.images.is_none() {
            match vix_jupyter::PrivateDir::create(&std::env::temp_dir()) {
                Ok(dir) => self.jupyter.images = Some(dir),
                Err(e) => {
                    self.messages
                        .error(t!("msg.image_open_failed", error = e).to_string());
                    return placeholder();
                }
            }
        }
        let Some(dir) = &self.jupyter.images else {
            return placeholder();
        };
        let path = dir.path().join(image.file_name());
        if let Err(e) = std::fs::write(&path, &image.bytes) {
            self.messages
                .error(t!("msg.image_open_failed", error = e).to_string());
            return placeholder();
        }
        self.open_image(&path);
        let opened = self.editor.active_tab().is_some_and(|t| {
            t.is_image()
                && t.path
                    .as_ref()
                    .is_some_and(|p| p.ends_with(image.file_name()))
        });
        if opened {
            t!("ui.jupyter_image_opened", mime = image.mime).to_string()
        } else {
            placeholder()
        }
    }

    /// Without the `image` capability an image cannot be shown: the dock
    /// gets its `text/plain` form instead.
    #[cfg(not(feature = "image"))]
    fn jupyter_open_image(&mut self, image: &Image) -> String {
        let _ = &self.jupyter;
        t!("ui.jupyter_image_placeholder", mime = image.mime).to_string()
    }
}

/// The dock line for an image that was refused.
fn refused_line(mime: &str, error: &ImageError) -> String {
    match error {
        ImageError::TooLarge | ImageError::TooManyPixels { .. } => {
            t!("ui.jupyter_image_too_large", mime = mime).to_string()
        }
        _ => t!("ui.jupyter_image_invalid", mime = mime).to_string(),
    }
}

/// Whether `file` is a relative path that stays inside the folder it is
/// joined to: no root, drive prefix or `..`.
fn is_contained(file: &str) -> bool {
    let path = Path::new(file);
    path.components().next().is_some()
        && path
            .components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

/// Write `image` to `path`, creating its folder.
fn write_file(path: &Path, image: &Image) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, &image.bytes)
}

/// The char offset where 0-based `line` starts in `text` (its char length
/// past the last line).
fn char_offset_of_line(text: &str, line: usize) -> usize {
    if line == 0 {
        return 0;
    }
    let mut seen = 0;
    for (i, c) in text.chars().enumerate() {
        if c == '\n' {
            seen += 1;
            if seen == line {
                return i + 1;
            }
        }
    }
    text.chars().count()
}

/// The `#+end_src` line of the block that ran `code`: the one opening at
/// `begin` if its body is still `code`, else the first block whose body is.
fn locate_block(text: &str, begin: usize, code: &str) -> Option<usize> {
    let lines: Vec<&str> = text.split('\n').collect();
    let end_if_same = |b: usize| {
        let (found, end, _) = crate::org::src_block_at(text, b)?;
        (found == b && lines[b + 1..end].join("\n") == code).then_some(end)
    };
    end_if_same(begin).or_else(|| {
        lines
            .iter()
            .enumerate()
            .filter(|(_, l)| {
                l.trim_start()
                    .get(..11)
                    .is_some_and(|p| p.eq_ignore_ascii_case("#+begin_src"))
            })
            .find_map(|(b, _)| end_if_same(b))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contained_paths() {
        assert!(is_contained("plot.png"));
        assert!(is_contained("./img/plot.png"));
        assert!(!is_contained("../plot.png"));
        assert!(!is_contained("img/../../plot.png"));
        assert!(!is_contained("/tmp/plot.png"));
        assert!(!is_contained(""));
    }

    #[test]
    fn line_offsets() {
        assert_eq!(char_offset_of_line("ab\nc\n", 0), 0);
        assert_eq!(char_offset_of_line("ab\nc\n", 1), 3);
        assert_eq!(char_offset_of_line("ab\nc\n", 2), 5);
        assert_eq!(char_offset_of_line("ab\nc\n", 9), 5);
    }

    #[test]
    fn a_moved_block_is_found_by_its_code() {
        let text = "x\n#+begin_src python\n1\n#+end_src\n#+begin_src python\n2\n#+end_src\n";
        assert_eq!(locate_block(text, 1, "1"), Some(3));
        assert_eq!(locate_block(text, 1, "2"), Some(6));
        assert_eq!(locate_block(text, 4, "3"), None);
    }
}
