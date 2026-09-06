//! Tools → Insert: Markdown/HTML/SQL/LaTeX/Org snippets, inline
//! markers/blocks, dynamic date/time/UUID/ZID insertion; and the small
//! calculator tools (color/unit converters, a calculator, a regex tester)
//! that share the same "open an input overlay, insert its result" shape.
//!
//! Moved out of `app.rs` verbatim (T141, slice 10 -- the first sub-slice
//! of "tools", another large scattered area like "org" was). Clean
//! contiguous block, minus two interlopers that stayed in `app.rs`:
//! `surround`/`toggle_wrap` are general editing operations, not
//! tools-menu snippets, despite sitting in the middle of this range.
//!
//! Also carries [`App::run_tools_action`] (T141, the epic's final piece:
//! the `run_action` match split): every `tools.*` action that is a plain
//! "open this overlay" call with no other logic, gathered into one
//! dispatcher regardless of which sub-slice (`insert_tools`,
//! `picker_panels`, `info_panels`) or `app.rs` itself now owns the method
//! it calls -- `pub(super)` visibility makes the cross-module calls free.
//! `tools.calendar`/`tools.clock` stayed inline in `run_action` (they toggle
//! more than one field, not a single call) and so did the prefix-matched
//! actions (`view.theme:`, `script:`, …).

#![warn(clippy::pedantic)]

use crossterm::event::{KeyCode, KeyEvent};

use super::{App, SQL_CREATE_EXTENSION, SQL_CREATE_TABLE};

impl App {
    /// Dispatch a plain "open this Tools overlay" action. Returns `true` if
    /// `action` was handled. Extracted from [`App::run_action`] to keep that
    /// function within the line limit -- the last piece of T141's `run_action`
    /// split, gathering the `tools.*` arms left un-delegated after every
    /// feature module already had its own dispatcher.
    pub(super) fn run_tools_action(&mut self, action: &str) -> bool {
        match action {
            "tools.nerd_palette" => self.open_nerd_palette(),
            "tools.ascii" => self.open_ascii_panel(),
            "tools.qrcode" => self.open_qrcode(),
            "tools.x11_colors" => self.open_x11_panel(),
            "tools.media_types" => self.open_media_type_panel(),
            "tools.html_chars" => self.open_html_panel(),
            "tools.system_info" => self.open_system_info(),
            "tools.file_info" => self.open_file_info(),
            "tools.text_info" => self.open_text_info(),
            "tools.markdown_preview" => self.open_markdown_preview(),
            "tools.snippets" => self.open_snippets(),
            "tools.contacts" => self.open_contacts(),
            "tools.dashboard" => self.open_dashboard(),
            "tools.color_converter" => self.open_color_converter(),
            "tools.calculator" => self.open_calculator(),
            "tools.regex_tester" => self.open_regex_tester(),
            "tools.pomodoro" => self.open_pomodoro(),
            _ => return false,
        }
        true
    }

    /// Insert generator output (a UUID, ZID, …) at the cursor in the active
    /// editor, reporting it in the status line. No-op when no buffer is editable.
    pub(super) fn insert_content(&mut self, text: &str) {
        let area = self.layout.editor;
        if self.editor.insert_str(text, area) {
            self.status = t!("status.generated", text = text).to_string();
        }
    }

    /// Insert a Markdown snippet for a `tools.insert.markdown.*` action at the
    /// cursor. Returns `true` if `action` was a known Markdown snippet.
    pub(super) fn insert_markdown(&mut self, action: &str) -> bool {
        let snippet = match action {
            "tools.insert.markdown.headline1" => "# Headline 1\n\n",
            "tools.insert.markdown.headline2" => "## Headline 2\n\n",
            "tools.insert.markdown.headline3" => "### Headline 3\n\n",
            "tools.insert.markdown.link" => "[Example](https://www.example.com)",
            "tools.insert.markdown.list" => "- Item\n- Item\n- Item\n\n",
            "tools.insert.markdown.table" => {
                "| x | x | x |\n|---|---|---|\n| x | x | x |\n| x | x | x |\n\n"
            }
            "tools.insert.markdown.todos" => "- [ ] Todo\n- [ ] Todo\n- [ ] Todo\n\n",
            _ => return false,
        };
        self.insert_content(snippet);
        true
    }

    /// Insert an HTML snippet for a `tools.insert.html.*` action at the cursor.
    /// Returns `true` if `action` was a known HTML snippet.
    pub(super) fn insert_html(&mut self, action: &str) -> bool {
        let snippet = match action {
            "tools.insert.html.headline1" => "<h1>Headline</h1>\n\n",
            "tools.insert.html.headline2" => "<h2>Headline</h2>\n\n",
            "tools.insert.html.headline3" => "<h3>Headline</h3>\n\n",
            "tools.insert.html.link" => "<a href=\"https://www.example.com\">Example</a>",
            "tools.insert.html.list" => {
                "<ul>\n  <li>Item</li>\n  <li>Item</li>\n  <li>Item</li>\n</ul>\n\n"
            }
            "tools.insert.html.table" => concat!(
                "<table>\n",
                "  <thead>\n",
                "    <tr><th>x</th><th>x</th><th>x</th></tr>\n",
                "  </thead>\n",
                "  <tbody>\n",
                "    <tr><td>x</td><td>x</td><td>x</td></tr>\n",
                "    <tr><td>x</td><td>x</td><td>x</td></tr>\n",
                "  </tbody>\n",
                "  <tfoot>\n",
                "    <tr><th>x</th><th>x</th><th>x</th></tr>\n",
                "  </tfoot>\n",
                "</table>\n\n",
            ),
            _ => return false,
        };
        self.insert_content(snippet);
        true
    }

    /// Insert a SQL (`PostgreSQL`) snippet for a `tools.insert.sql.*` action at the
    /// cursor. Returns `true` if `action` was a known SQL snippet.
    pub(super) fn insert_sql(&mut self, action: &str) -> bool {
        let snippet = match action {
            "tools.insert.sql.alter_role" => {
                "-- Enable a user to create a database.\nALTER ROLE alice WITH CREATEDB;\n"
            }
            "tools.insert.sql.create_extension" => SQL_CREATE_EXTENSION,
            "tools.insert.sql.create_function" => concat!(
                "CREATE FUNCTION updated_at()\n",
                "RETURNS TRIGGER AS $$\n",
                "BEGIN\n",
                "    NEW.updated_at = NOW();\n",
                "    RETURN NEW;\n",
                "END;\n",
                "$$ LANGUAGE plpgsql;\n",
            ),
            "tools.insert.sql.create_user" => {
                "CREATE USER alice\nWITH LOGIN\nENCRYPTED PASSWORD 'secret';\n"
            }
            "tools.insert.sql.grant_create" => concat!(
                "-- Enable a user to create new tables, views, etc. inside the public schema.\n",
                "GRANT CREATE ON SCHEMA public TO alice;\n",
            ),
            "tools.insert.sql.grant_usage" => concat!(
                "-- Enable a user to see and use objects in the public schema.\n",
                "GRANT USAGE ON SCHEMA public TO alice;\n",
            ),
            "tools.insert.sql.create_table" => SQL_CREATE_TABLE,
            _ => return false,
        };
        self.insert_content(snippet);
        true
    }

    /// Insert an Org/LaTeX markup snippet for a `tools.insert.latex.*` action at
    /// the cursor. Returns `true` if `action` was a known snippet.
    pub(super) fn insert_latex(&mut self, action: &str) -> bool {
        let snippet = match action {
            "tools.insert.latex.headline" => "* Headline\n",
            "tools.insert.latex.subheadline" => "** Subheadline\n",
            "tools.insert.latex.link" => "[[https://org.mode][Org]]",
            "tools.insert.latex.bold" => "*hello*",
            "tools.insert.latex.italic" => "/hello/",
            "tools.insert.latex.underline" => "_hello_",
            "tools.insert.latex.table" => {
                "| x | x | x |\n|---|---|---|\n| x | x | x |\n| x | x | x |\n"
            }
            "tools.insert.latex.deadline" => "DEADLINE: <YYYY-MM-DD Day>\n",
            "tools.insert.latex.scheduled" => "SCHEDULED: <YYYY-MM-DD Day>\n",
            "tools.insert.latex.time_range" => {
                "<2004-08-23 Mon 10:00-11:00>--<2004-08-26 Thu 10:00-11:00>"
            }
            "tools.insert.latex.timestamp" => "<2006-11-02 Thu 10:00-12:00>",
            "tools.insert.latex.timestamp_repeater" => "<2006-11-02 Thu 10:00-12:00 +1w>",
            "tools.insert.latex.quote" => concat!(
                "#+BEGIN_QUOTE\n",
                "Everything should be made\n",
                "as simple as possible,\n",
                "but not any simpler.\n",
                "---Albert Einstein\n",
                "#+END_QUOTE\n",
            ),
            "tools.insert.latex.verse" => concat!(
                "#+BEGIN_VERSE\n",
                "I write, erase, rewrite\n",
                "Erase again, and then\n",
                "A poppy blooms.\n",
                "---Katsushika Hokusai\n",
                "#+END_VERSE\n",
            ),
            "tools.insert.latex.center" => concat!(
                "#+BEGIN_CENTER\n",
                "Nature is an infinite sphere\n",
                "of which the center is everywhere\n",
                "and the circumference nowhere.\n",
                "--- Blaise Pascal\n",
                "#+END_CENTER\n",
            ),
            "tools.insert.latex.drawer" => {
                concat!(":DRAWERNAME:\n", "This is inside the drawer.\n", ":END:\n",)
            }
            _ => return false,
        };
        self.insert_content(snippet);
        true
    }

    /// Insert a ditaa ASCII-art shape for a `tools.draw.*` action at the cursor.
    /// Returns `true` if `action` was a known shape. See <https://ditaa.sourceforge.net/>.
    #[allow(clippy::too_many_lines)]
    pub(super) fn draw_insert(&mut self, action: &str) -> bool {
        let art = match action {
            "tools.draw.rectangle" => "+-------+\n|       |\n+-------+\n",
            "tools.draw.rounded" => "/-------\\\n|       |\n\\-------/\n",
            "tools.draw.document" => "+-------+\n|{d}    |\n+-------+\n",
            "tools.draw.storage" => "+-------+\n|{s}    |\n+-------+\n",
            "tools.draw.line_h" => "--------",
            "tools.draw.line_v" => "|\n|\n|\n",
            "tools.draw.dashed_h" => "========",
            "tools.draw.dashed_v" => ":\n:\n:\n",
            "tools.draw.arrow_right" => "------->",
            "tools.draw.arrow_left" => "<-------",
            "tools.draw.arrow_up" => "^\n|\n|\n",
            "tools.draw.arrow_down" => "|\n|\nv\n",
            "tools.draw.point" => "*",
            "tools.draw.flow" => concat!(
                "+--------+      +--------+\n",
                "| Box A  +----->| Box B  |\n",
                "+--------+      +--------+\n",
            ),
            "tools.draw.color_box" => concat!(
                "/--------\\\n",
                "|cBLU    |\n",
                "| Box    |\n",
                "\\--------/\n",
            ),
            _ => return false,
        };
        self.insert_content(art);
        true
    }

    /// Insert an Org-mode snippet for a `tools.insert.org.*` action at the cursor.
    /// Returns `true` if `action` was a known Org snippet.
    pub(super) fn insert_org(&mut self, action: &str) -> bool {
        let snippet = match action {
            "tools.insert.org.title" => "#+title: Hello World\n",
            "tools.insert.org.author" => "#+author: Alice Adams\n",
            "tools.insert.org.headline" => "* Headline\n",
            "tools.insert.org.subheadline" => "** Subheadline\n",
            "tools.insert.org.link" => "[[https://org.mode][Org]]",
            "tools.insert.org.image" => "[[https://example.com]]",
            "tools.insert.org.list" => "- Alfa\n- Bravo\n- Charlie\n",
            "tools.insert.org.ordered_list" => "1. Alfa\n2. Bravo\n3. Charlie\n",
            "tools.insert.org.check_list" => concat!(
                "- [ ] Alfa work ready to do\n",
                "- [-] Bravo work in progress\n",
                "- [x] Charlie work complete\n",
            ),
            "tools.insert.org.table" => {
                "| x | x | x |\n|---|---|---|\n| x | x | x |\n| x | x | x |\n"
            }
            "tools.insert.org.todo" => "**** TODO A todo item.\n",
            "tools.insert.org.done" => "**** DONE A todo item that has been done.\n",
            "tools.insert.org.deadline" => "DEADLINE: <YYYY-MM-DD Day>\n",
            "tools.insert.org.scheduled" => "SCHEDULED: <YYYY-MM-DD Day>\n",
            "tools.insert.org.time_range" => {
                "<2004-08-23 Mon 10:00-11:00>--<2004-08-26 Thu 10:00-11:00>"
            }
            "tools.insert.org.timestamp" => "<2006-11-02 Thu 10:00-12:00>",
            "tools.insert.org.timestamp_repeater" => "<2006-11-02 Thu 10:00-12:00 +1w>",
            "tools.insert.org.drawer" => ":DRAWERNAME:\nThis is inside the drawer.\n:END:\n",
            "tools.insert.org.properties" => concat!(
                ":PROPERTIES:\n",
                ":Title:     Goldberg Variations\n",
                ":Composer:  J.S. Bach\n",
                ":Publisher: Deutsche Grammophon\n",
                ":NDisks:    1\n",
                ":END:\n",
            ),
            _ => return false,
        };
        self.insert_content(snippet);
        true
    }

    /// Toggle an Org inline emphasis marker for a `tools.insert.marker.*` action.
    /// Returns `true` if `action` was a known marker.
    pub(super) fn insert_marker(&mut self, action: &str) -> bool {
        let ch = match action {
            "tools.insert.marker.tag" => ":",
            "tools.insert.marker.bold" => "*",
            "tools.insert.marker.italic" => "/",
            "tools.insert.marker.underline" => "_",
            "tools.insert.marker.strikethrough" => "+",
            "tools.insert.marker.code" => "~",
            "tools.insert.marker.verbatim" => "=",
            _ => return false,
        };
        self.toggle_wrap(ch, ch);
        true
    }

    /// Toggle an Org `#+BEGIN_…`/`#+END_…` block for a `tools.insert.block.*`
    /// action. Returns `true` if `action` was a known block.
    pub(super) fn insert_block(&mut self, action: &str) -> bool {
        let name = match action {
            "tools.insert.block.comment" => "COMMENT",
            "tools.insert.block.center" => "CENTER",
            "tools.insert.block.quote" => "QUOTE",
            "tools.insert.block.verse" => "VERSE",
            _ => return false,
        };
        self.toggle_wrap(&format!("#+BEGIN_{name}\n"), &format!("\n#+END_{name}"));
        true
    }

    /// Insert dynamically-generated content (Lorem ipsum, date/time presets) for
    /// a `tools.insert.*` action. Returns `true` if `action` was handled.
    pub(super) fn insert_dynamic(&mut self, action: &str) -> bool {
        let text = match action {
            "tools.insert.lorem.words" => crate::lorem::words(8),
            "tools.insert.lorem.sentence" => crate::lorem::sentence(),
            "tools.insert.lorem.paragraph" => format!("{}\n\n", crate::lorem::paragraph()),
            "tools.insert.datetime.iso8601" => crate::clock::iso8601(&crate::clock::now_local()),
            "tools.insert.datetime.rfc3339" => crate::clock::rfc3339(&crate::clock::now_local()),
            "tools.insert.datetime.epoch" => {
                crate::clock::epoch_seconds(&crate::clock::now_local())
            }
            _ => return false,
        };
        self.insert_content(&text);
        true
    }

    /// Open the Color Converter dialog, seeding it from the selection when that
    /// text parses as a HEX/RGB/HSL color.
    pub(super) fn open_color_converter(&mut self) {
        let mut conv = crate::color_converter_tool::Converter::new();
        if let Some(sel) = self
            .editor
            .active_tab_mut()
            .and_then(|t| t.editor.get_selection_text())
        {
            let s = sel.trim();
            let color = crate::color_converter_tool::Color::from_hex(s)
                .or_else(|| crate::color_converter_tool::Color::from_rgb(s))
                .or_else(|| crate::color_converter_tool::Color::from_hsl(s));
            if let Some(c) = color {
                conv.set_color(c);
            }
        }
        self.color_converter = Some(conv);
    }

    /// Handle a key for the Color Converter dialog: type into the focused field
    /// (live-updating the others), Tab/arrows to switch fields, Enter to insert
    /// the focused value into the editor, Esc to close.
    pub(super) fn color_converter_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.color_converter = None,
            KeyCode::Tab | KeyCode::Down => {
                if let Some(c) = self.color_converter.as_mut() {
                    c.focus_next();
                }
            }
            KeyCode::BackTab | KeyCode::Up => {
                if let Some(c) = self.color_converter.as_mut() {
                    c.focus_prev();
                }
            }
            KeyCode::Backspace => {
                if let Some(c) = self.color_converter.as_mut() {
                    c.backspace();
                }
            }
            KeyCode::Enter => self.insert_color_value(),
            KeyCode::Char(ch) if !Self::ctrl(&key) && !Self::alt(&key) => {
                if let Some(c) = self.color_converter.as_mut() {
                    c.push(ch);
                }
            }
            _ => {}
        }
    }

    /// Insert the focused field's value into the editor and close the dialog.
    fn insert_color_value(&mut self) {
        let Some(value) = self
            .color_converter
            .as_ref()
            .map(|c| c.current().to_string())
        else {
            return;
        };
        if value.is_empty() {
            self.color_converter = None;
            return;
        }
        let area = self.layout.editor;
        self.editor.insert_str(&value, area);
        self.status = t!("status.generated", text = value).to_string();
        self.color_converter = None;
    }

    /// Open the Unit Converter dialog (seeded with `1 m → km`).
    pub(super) fn open_unit_converter(&mut self) {
        self.unit_converter = Some(crate::unit_converter_tool::Converter::new());
    }

    /// Handle a key for the Unit Converter dialog: type a number into the value
    /// field, Tab/Up/Down to switch field, Left/Right to cycle the focused unit
    /// selector, Enter to insert the result, Esc to close.
    pub(super) fn unit_converter_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.unit_converter = None,
            KeyCode::Tab | KeyCode::Down => {
                if let Some(c) = self.unit_converter.as_mut() {
                    c.focus_next();
                }
            }
            KeyCode::BackTab | KeyCode::Up => {
                if let Some(c) = self.unit_converter.as_mut() {
                    c.focus_prev();
                }
            }
            KeyCode::Left => {
                if let Some(c) = self.unit_converter.as_mut() {
                    c.cycle(-1);
                }
            }
            KeyCode::Right => {
                if let Some(c) = self.unit_converter.as_mut() {
                    c.cycle(1);
                }
            }
            KeyCode::Backspace => {
                if let Some(c) = self.unit_converter.as_mut() {
                    c.backspace();
                }
            }
            KeyCode::Enter => self.insert_unit_value(),
            KeyCode::Char(ch) if !Self::ctrl(&key) && !Self::alt(&key) => {
                if let Some(c) = self.unit_converter.as_mut() {
                    c.push(ch);
                }
            }
            _ => {}
        }
    }

    /// Insert the converted `<value> <unit>` into the editor and close the dialog.
    fn insert_unit_value(&mut self) {
        let Some(text) = self
            .unit_converter
            .as_ref()
            .map(crate::unit_converter_tool::Converter::insert_text)
        else {
            return;
        };
        if !text.is_empty() {
            let area = self.layout.editor;
            self.editor.insert_str(&text, area);
            self.status = t!("status.generated", text = text).to_string();
        }
        self.unit_converter = None;
    }

    /// Open the Calculator dialog, seeding the formula from the selection.
    pub(super) fn open_calculator(&mut self) {
        let mut calc = crate::calculator_tool::Calculator::new();
        if let Some(sel) = self
            .editor
            .active_tab_mut()
            .and_then(|t| t.editor.get_selection_text())
        {
            calc.input = sel.trim().to_string();
        }
        self.calculator = Some(calc);
    }

    /// Handle a key for the Calculator dialog: type the formula, Enter runs it
    /// (or, with the Insert button focused, inserts the result), Tab cycles the
    /// Input/Run/Insert controls, Esc closes.
    pub(super) fn calculator_key(&mut self, key: KeyEvent) {
        use crate::calculator_tool::Focus;
        match key.code {
            KeyCode::Esc => self.calculator = None,
            KeyCode::Tab => {
                if let Some(c) = self.calculator.as_mut() {
                    c.focus_next();
                }
            }
            KeyCode::BackTab => {
                if let Some(c) = self.calculator.as_mut() {
                    c.focus_prev();
                }
            }
            KeyCode::Enter => {
                let focus = self.calculator.as_ref().map(|c| c.focus);
                match focus {
                    Some(Focus::Insert) => self.insert_calculator_result(),
                    Some(_) => {
                        if let Some(c) = self.calculator.as_mut() {
                            c.run();
                        }
                    }
                    None => {}
                }
            }
            KeyCode::Backspace => {
                if let Some(c) = self.calculator.as_mut() {
                    c.backspace();
                }
            }
            KeyCode::Char(ch) if !Self::ctrl(&key) && !Self::alt(&key) => {
                if let Some(c) = self.calculator.as_mut()
                    && c.focus == Focus::Input
                {
                    c.push(ch);
                }
            }
            _ => {}
        }
    }

    /// Insert the Calculator's current result into the editor and close it.
    pub(super) fn insert_calculator_result(&mut self) {
        let result = self
            .calculator
            .as_ref()
            .and_then(|c| c.result().map(str::to_string));
        if let Some(value) = result {
            let area = self.layout.editor;
            self.editor.insert_str(&value, area);
            self.status = t!("status.generated", text = value).to_string();
            self.calculator = None;
        }
    }

    /// Open the regex tester, seeding the subject from the selection.
    pub(super) fn open_regex_tester(&mut self) {
        let subject = self
            .editor
            .active_tab_mut()
            .and_then(|t| t.editor.get_selection_text())
            .unwrap_or_default();
        self.regex_tester = Some(crate::regex_tool::Tester::new(subject));
    }

    /// Handle a key for the regex tester: type into the focused field, Tab
    /// switches pattern/subject, Esc closes.
    pub(super) fn regex_tester_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.regex_tester = None,
            KeyCode::Tab => {
                if let Some(t) = self.regex_tester.as_mut() {
                    t.toggle_field();
                }
            }
            KeyCode::Backspace => {
                if let Some(t) = self.regex_tester.as_mut() {
                    t.backspace();
                }
            }
            KeyCode::Char(c) if !Self::ctrl(&key) && !Self::alt(&key) => {
                if let Some(t) = self.regex_tester.as_mut() {
                    t.push(c);
                }
            }
            _ => {}
        }
    }
}
