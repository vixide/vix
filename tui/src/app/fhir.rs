//! FHIR client (T684): the request prompt and the Bundle viewer overlay. The
//! networking half lives in `fhir_http.rs` (feature `http`); everything here is
//! terminal state only and compiles without it.

use vix_input::{KeyCode, KeyEvent, MouseEvent};

use super::picker_panels::{list_panel_click_index, list_panel_nav_key};
use super::{App, Focus, Prompt, PromptKind};

impl App {
    /// Dispatch the FHIR actions. Returns `true` if handled. The `tools.http_`
    /// prefix puts both under the `http` capability gate in `run_action`.
    pub(super) fn run_fhir_action(&mut self, action: &str) -> bool {
        match action {
            "tools.http_fhir_request" => {
                self.prompt = Some(Prompt::new(
                    PromptKind::FhirRequest,
                    t!("prompt.fhir_request").to_string(),
                ));
            }
            "tools.http_fhir_bundle" => self.fhir_open_bundle(),
            _ => return false,
        }
        true
    }

    /// Open the Bundle viewer on the active buffer if it holds a Bundle,
    /// otherwise on the last FHIR response.
    pub(super) fn fhir_open_bundle(&mut self) {
        let text = self.editor.active_tab().map(crate::editor::Tab::text);
        if let Some(text) = text
            && let Ok(b) = crate::fhir::parse_bundle(&text)
        {
            // Remembered (in memory only) so the viewer can be reopened after
            // an entry has taken over the active tab.
            self.fhir_last = Some(text);
            self.fhir_bundle = Some(crate::fhir::BundlePanel::open(b));
            return;
        }
        match self
            .fhir_last
            .as_deref()
            .and_then(|text| crate::fhir::parse_bundle(text).ok())
        {
            Some(b) => self.fhir_bundle = Some(crate::fhir::BundlePanel::open(b)),
            None => self.status = t!("status.fhir_no_bundle").to_string(),
        }
    }

    /// Keys for the Bundle viewer: arrows/page move, Enter opens the entry,
    /// `n` follows the `next` link, Esc closes.
    pub(super) fn fhir_bundle_key(&mut self, key: KeyEvent) {
        let page = (self.layout.fhir_bundle.height as usize).max(1);
        if list_panel_nav_key(self.fhir_bundle.as_mut(), key.code, page) {
            return;
        }
        match key.code {
            KeyCode::Enter => self.fhir_open_selected_entry(),
            KeyCode::Char('n') => self.fhir_follow_next(),
            KeyCode::Esc => self.fhir_bundle = None,
            _ => {}
        }
    }

    /// A click selects a row; a click on the highlighted row opens it.
    pub(super) fn fhir_bundle_mouse(&mut self, mouse: MouseEvent) {
        let Some(idx) =
            list_panel_click_index(self.fhir_bundle.as_ref(), self.layout.fhir_bundle, mouse)
        else {
            return;
        };
        let already = self.fhir_bundle.as_ref().is_some_and(|p| p.selected == idx);
        if let Some(p) = self.fhir_bundle.as_mut()
            && p.select_index(idx)
            && already
        {
            self.fhir_open_selected_entry();
        }
    }

    /// Open the highlighted entry's resource, pretty-printed, in a JSON tab
    /// (the viewer closes; Tools -> FHIR Bundle reopens it on the same Bundle).
    fn fhir_open_selected_entry(&mut self) {
        let Some(text) = self
            .fhir_bundle
            .as_ref()
            .and_then(crate::fhir::BundlePanel::selected_entry)
            .and_then(crate::fhir::Entry::resource_pretty)
        else {
            return;
        };
        self.fhir_bundle = None;
        self.editor
            .new_tab_with_content_as(&text, Some("resource.json"));
        self.focus = Focus::Editor;
    }
}
