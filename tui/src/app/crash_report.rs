//! Telling the user about a saved crash report, and **Help → Report a
//! Crash…** (`spec/crash-reports/index.md`). The report itself is written by
//! the panic hook in [`crate::crash`]; nothing here sends anything.

#![cfg_attr(
    not(test),
    deny(
        clippy::arithmetic_side_effects,
        clippy::integer_division,
        clippy::modulo_arithmetic,
        clippy::as_conversions,
        clippy::indexing_slicing,
        clippy::string_slice
    )
)]

use std::path::Path;

use vix_input::{KeyCode, KeyEvent};

use super::{App, Confirm, ConfirmKind};
use crate::messages::Level;

impl App {
    /// On start, say once that the last run saved a crash report.
    pub(super) fn notice_crash_report(&mut self) {
        let Some(dir) = crate::crash::crashes_dir() else {
            return;
        };
        if let Some(path) = crate::crash::take_unseen(&dir) {
            let msg = t!("status.crash_report_waiting").to_string();
            self.messages
                .push(Level::Warn, format!("{msg} ({})", path.display()));
            self.status = msg;
        }
    }

    /// **Help → Report a Crash…**: show the newest report read-only, then ask
    /// whether to open it as a prefilled GitHub issue.
    pub(super) fn report_crash(&mut self) {
        let newest = crate::crash::crashes_dir()
            .and_then(|dir| crate::crash::reports(&dir).into_iter().next());
        let Some(path) = newest else {
            self.status = t!("status.no_crash_reports").to_string();
            return;
        };
        self.show_crash_report(&path);
    }

    /// Open `path` (a crash report) read-only and ask about the issue.
    pub(crate) fn show_crash_report(&mut self, path: &Path) {
        let Ok(text) = std::fs::read_to_string(path) else {
            self.status = t!("status.no_crash_reports").to_string();
            return;
        };
        let title = path.file_name().map_or_else(
            || "crash report".to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        self.open_readonly_text_tab(&title, &text);
        self.confirm = Some(Confirm {
            message: t!("confirm.report_crash").to_string(),
            paths: vec![path.to_path_buf()],
            kind: ConfirmKind::ReportCrash,
        });
    }

    /// The crash-report confirmation: `y` opens the prefilled issue (or, when
    /// no browser opens, puts its link in the message drawer); `n`/Esc closes.
    /// Returns whether the key was this confirmation's.
    pub(super) fn crash_confirm_key(&mut self, key: KeyEvent) -> bool {
        if self.confirm.as_ref().map(|c| c.kind) != Some(ConfirmKind::ReportCrash) {
            return false;
        }
        match key.code {
            KeyCode::Char('y' | 'Y') => {
                let path = self.confirm.take().and_then(|c| c.paths.into_iter().next());
                let report = path.and_then(|p| std::fs::read_to_string(p).ok());
                if let Some(report) = report {
                    let url = crate::crash::issue_url(env!("CARGO_PKG_REPOSITORY"), &report);
                    if super::doc_links::open_external(&url) {
                        self.status = t!("status.crash_issue_opened").to_string();
                    } else {
                        self.messages.push(Level::Info, url);
                        self.status = t!("status.crash_issue_failed").to_string();
                    }
                }
            }
            KeyCode::Char('n' | 'N') | KeyCode::Esc => self.confirm = None,
            _ => {}
        }
        true
    }
}
