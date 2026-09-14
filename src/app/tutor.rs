//! The interactive tutorial: working-copy creation, chapter navigation, and
//! the live progress indicator (`crates/vix-tutor/spec/index.md`). `vix
//! --tutor` and **Help → Tutorial** both call [`App::open_tutor`].

#![warn(clippy::pedantic)]

use std::path::PathBuf;

use super::App;

/// The active tutorial session: where its working copies live, and which
/// chapter is current. Not persisted across restarts (`spec/index.md`, §
/// Session state) — a fresh `App` always starts a fresh session.
pub(super) struct TutorSession {
    /// The `vix-tutor-<pid>` working directory holding every chapter's
    /// working copy, canonicalized (matching how `vix_editor::Editor::open`
    /// stores every `Tab::path`, so the two can be compared directly).
    dir: PathBuf,
    /// 0-based index into `vix_tutor::CHAPTERS`.
    active: usize,
}

impl App {
    /// Open (or resume) the interactive tutorial: on first call this
    /// session, copies every chapter's bundled body into a fresh working
    /// directory, then opens the active chapter as a normal, freely-editable
    /// tab. A later call (e.g. re-selecting **Help → Tutorial**) just
    /// re-opens whichever chapter was last active.
    pub fn open_tutor(&mut self) {
        if self.tutor.is_none() {
            let dir = std::env::temp_dir().join(format!("vix-tutor-{}", std::process::id()));
            if let Err(e) = std::fs::create_dir_all(&dir) {
                self.messages
                    .error(t!("msg.open_failed", error = e).to_string());
                return;
            }
            let dir = dir.canonicalize().unwrap_or(dir);
            for chapter in vix_tutor::CHAPTERS {
                let path = dir.join(chapter.filename());
                if let Err(e) = std::fs::write(&path, chapter.body()) {
                    self.messages
                        .error(t!("msg.open_failed", error = e).to_string());
                    return;
                }
            }
            self.tutor = Some(TutorSession { dir, active: 0 });
        }
        let active = self.tutor.as_ref().map_or(0, |s| s.active);
        self.open_tutor_chapter(active);
    }

    /// Open `ordinal`'s working copy as the active tab and refresh the
    /// status-bar progress indicator. No-op if the tutor has no session yet
    /// or `ordinal` is out of range (both defensive: every caller already
    /// guards these).
    fn open_tutor_chapter(&mut self, ordinal: usize) {
        let (Some(session), Some(chapter)) = (&self.tutor, vix_tutor::by_ordinal(ordinal)) else {
            return;
        };
        let path = session.dir.join(chapter.filename());
        self.open_path(&path, false);
        self.update_tutor_status();
    }

    /// **Help → Tutorial**'s companion navigation: move to the next chapter,
    /// clamped at the last one (no wraparound). No-op if the tutorial isn't
    /// open.
    pub(super) fn tutor_next_chapter(&mut self) {
        let Some(session) = &mut self.tutor else {
            return;
        };
        session.active = (session.active + 1).min(vix_tutor::CHAPTERS.len() - 1);
        let ordinal = session.active;
        self.open_tutor_chapter(ordinal);
    }

    /// Move to the previous chapter, clamped at chapter 1. No-op if the
    /// tutorial isn't open.
    pub(super) fn tutor_prev_chapter(&mut self) {
        let Some(session) = &mut self.tutor else {
            return;
        };
        session.active = session.active.saturating_sub(1);
        let ordinal = session.active;
        self.open_tutor_chapter(ordinal);
    }

    /// Overwrite the active chapter's working copy with its pristine bundled
    /// body, discarding whatever the learner did to it — assumes the active
    /// tab *is* the tutorial chapter's own tab, which holds as long as this
    /// is invoked from inside it (its only sensible use). No-op if the
    /// tutorial isn't open.
    pub(super) fn tutor_restart_chapter(&mut self) {
        let Some(session) = &self.tutor else {
            return;
        };
        let Some(chapter) = vix_tutor::by_ordinal(session.active) else {
            return;
        };
        let path = session.dir.join(chapter.filename());
        let body = chapter.body();
        if let Err(e) = std::fs::write(&path, body) {
            self.messages
                .error(t!("msg.open_failed", error = e).to_string());
            return;
        }
        if let Some(tab) = self.editor.active_tab_mut() {
            tab.editor.set_content(body);
            tab.editor.set_cursor(0);
            tab.dirty = false;
        }
        self.status = t!("status.tutor_chapter_restarted").to_string();
        self.update_tutor_status();
    }

    /// Re-derive the active chapter's progress from the active tab's current
    /// text and cursor, and show it in the status bar — called after every
    /// key (`App::on_key`) so it stays live as the learner types, and
    /// again by every navigation/restart action above. A cheap no-op
    /// whenever the active tab isn't the tutor's own (the learner switched
    /// to some other file): the indicator is simply left as whatever it
    /// last said, not cleared or recomputed against unrelated text.
    pub(super) fn update_tutor_status(&mut self) {
        let (Some(session), Some(tab)) = (&self.tutor, self.editor.active_tab()) else {
            return;
        };
        let Some(chapter) = vix_tutor::by_ordinal(session.active) else {
            return;
        };
        if tab.path.as_deref() != Some(session.dir.join(chapter.filename()).as_path()) {
            return;
        }
        let progress = vix_tutor::progress(chapter.id, &tab.text(), tab.editor.get_cursor());
        self.status = t!(
            "status.tutor_progress",
            chapter = t!(chapter.title),
            done = progress.done,
            total = progress.total
        )
        .to_string();
    }
}
