//! Small modal dialogs: the generic yes/no confirm box, the workspace-script
//! trust prompt, the find/replace confirmation viewer, the unsaved-changes
//! prompt, the paste-name-conflict prompt, and the interactive query-replace
//! status bar.

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

use super::ClampFit;
use crate::geo::{Geo, chunk};
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};

use crate::app::App;
use crate::theme::{self, icon};

/// Right-align a keyboard-only dialog's text when the layout is mirrored. These
/// dialogs have no column-based clicks, so shifting the drawn text is safe (T564).
fn right_align(app: &App, frame: &mut Frame, inner: Rect) {
    if super::layout_mirrored(app) {
        super::bidi::reflect_rows(
            frame.buffer_mut(),
            inner.x,
            inner.right(),
            inner.y,
            inner.bottom(),
        );
    }
}

/// Draw the yes/no confirmation dialog. Width is measured in display columns
/// (so wide text fits) and a message wider than the screen wraps onto more lines.
pub(super) fn draw_confirm(app: &App, frame: &mut Frame, area: Rect) {
    let Some(c) = app.confirm.as_ref() else {
        return;
    };
    // Display columns, so wide (CJK) text is measured as it is drawn.
    let cols = u16::try_from(Line::from(c.message.as_str()).width()).unwrap_or(u16::MAX);
    let width = cols.plus(6).min(area.width);
    // A message wider than the screen (Cowork's terminal-sharing warning,
    // T697) wraps onto more lines instead of being cut off.
    let text_width = width.minus(2).max(1);
    // Word wrapping can need one line more than the column count says.
    let lines = if cols <= text_width {
        1
    } else {
        cols.div_ceil(text_width).plus(1)
    };
    let y = area.y.plus(area.height.quot(3));
    let rect = Rect {
        x: area.x.plus((area.width.minus(width)).quot(2)),
        y,
        width,
        height: lines.plus(2).min(area.bottom().minus(y)),
    };
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} ", t!("ui.confirm")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    frame.render_widget(
        Paragraph::new(Line::from(c.message.clone())).wrap(Wrap { trim: true }),
        inner,
    );
    right_align(app, frame, inner);
}

/// The "trust this workspace?" prompt (T132): its scripts, and its project
/// settings file (T627).
pub(super) fn draw_script_trust(app: &App, frame: &mut Frame, area: Rect) {
    let Some(p) = app.script_trust.as_ref() else {
        return;
    };
    let width = area.width.times(3).quot(5).clamp(40, 64).min(area.width);
    // More rows when the message names several things to trust.
    let parts = usize::from(p.count > 0)
        .plus(usize::from(p.settings))
        .plus(usize::from(p.env))
        .plus(usize::from(p.language_server.is_some()));
    let height = if parts > 1 { 9u16 } else { 7 }.min(area.height);
    let rect = Rect {
        x: area.x.plus((area.width.minus(width)).quot(2)),
        y: area.y.plus(area.height.quot(3)),
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(
            " {} ",
            if p.count > 0 {
                t!("ui.script_trust_title")
            } else {
                t!("ui.workspace_trust_title")
            }
        ));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);
    // T627: the same prompt covers `.vix/settings.toml`.
    let message = match (p.count > 0, p.settings) {
        (true, true) => format!(
            "{} {}",
            t!("ui.script_trust_message", count = p.count),
            t!("ui.workspace_trust_settings_also")
        ),
        (true, false) => t!("ui.script_trust_message", count = p.count).to_string(),
        (false, true) => t!("ui.workspace_trust_settings_message").to_string(),
        (false, false) => t!("ui.workspace_trust_env_message").to_string(),
    };
    // T667: an environment source (`.envrc`, mise, `.vix/env.toml`) too.
    let message = if p.env && (p.count > 0 || p.settings) {
        format!("{message} {}", t!("ui.workspace_trust_env_also"))
    } else {
        message
    };
    // A built-in language server waiting on the answer.
    let message = match &p.language_server {
        Some(server) if p.count == 0 && !p.settings && !p.env => {
            t!("ui.workspace_trust_lsp_message", server = server).to_string()
        }
        Some(server) => format!(
            "{message} {}",
            t!("ui.workspace_trust_lsp_also", server = server)
        ),
        None => message,
    };
    frame.render_widget(
        Paragraph::new(message).wrap(Wrap { trim: true }),
        chunk(&chunks, 0),
    );
    let hint = Line::from(Span::styled(
        t!("ui.script_trust_hint").to_string(),
        theme::dim(),
    ));
    frame.render_widget(Paragraph::new(hint), chunk(&chunks, 1));
    right_align(app, frame, inner);
}

/// Draw the workspace-replace confirmation overlay listing the pending changes.
pub(super) fn draw_replace_confirm(app: &App, frame: &mut Frame, area: Rect) {
    let Some(rc) = app.replace_confirm.as_ref() else {
        return;
    };
    let width = area.width.times(7).quot(10).clamp_fit(30, area.width);
    let height = area.height.times(6).quot(10).clamp_fit(8, area.height);
    let rect = Rect {
        x: area.x.plus(area.width.minus(width).quot(2)),
        y: area.y.plus(area.height.minus(height).quot(3)),
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let title = format!(
        " {} {} ",
        icon::SEARCH,
        t!(
            "ui.replace_confirm_title",
            replaced = rc.replaced,
            files = rc.plan.len()
        )
    );
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(title);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);
    let view_h = usize::from(chunk(&chunks, 0).height);
    let start = rc.scroll.min(rc.lines.len().minus(1));
    let lines: Vec<Line> = rc
        .lines
        .iter()
        .skip(start)
        .take(view_h)
        .map(|l| Line::from(Span::raw(l.clone())))
        .collect();
    frame.render_widget(Paragraph::new(lines), chunk(&chunks, 0));
    let hint = Line::from(Span::styled(
        t!("ui.replace_confirm_hint").to_string(),
        theme::dim(),
    ));
    frame.render_widget(Paragraph::new(hint), chunk(&chunks, 1));
}

/// The T211 apply-diff-back-to-sources confirm step, mirroring
/// `draw_replace_confirm`'s layout exactly (they share `ReplaceConfirm`'s
/// shape) but over `app.wgrep_confirm` and with edit-specific wording.
pub(super) fn draw_wgrep_confirm(app: &App, frame: &mut Frame, area: Rect) {
    let Some(wc) = app.wgrep_confirm.as_ref() else {
        return;
    };
    let title = format!(
        " {} {} ",
        icon::SEARCH,
        t!(
            "ui.wgrep_confirm_title",
            edited = wc.replaced,
            files = wc.plan.len()
        )
    );
    let hint = t!("ui.wgrep_confirm_hint").to_string();
    draw_preview_confirm(frame, area, &title, &wc.lines, wc.scroll, &hint);
}

/// The T636 directory-listing confirm step: one row per rename, create and
/// delete, in the same layout as the other preview-and-confirm steps.
pub(super) fn draw_dir_edit_confirm(app: &App, frame: &mut Frame, area: Rect) {
    let Some(c) = app.dir_edit_confirm.as_ref() else {
        return;
    };
    let title = format!(
        " {} ",
        t!(
            "ui.dir_edit_confirm_title",
            count = c.count,
            path = c.dir_label
        )
    );
    let hint = t!("ui.dir_edit_confirm_hint").to_string();
    draw_preview_confirm(frame, area, &title, &c.lines, c.scroll, &hint);
}

/// A centered preview-and-confirm box: `title`, the scrollable preview
/// `lines` from `scroll`, and a dim `hint` row.
fn draw_preview_confirm(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    lines: &[String],
    scroll: usize,
    hint: &str,
) {
    let width = area.width.times(7).quot(10).clamp_fit(30, area.width);
    let height = area.height.times(6).quot(10).clamp_fit(8, area.height);
    let rect = Rect {
        x: area.x.plus(area.width.minus(width).quot(2)),
        y: area.y.plus(area.height.minus(height).quot(3)),
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(title.to_string());
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);
    let view_h = usize::from(chunk(&chunks, 0).height);
    let start = scroll.min(lines.len().minus(1));
    let rows: Vec<Line> = lines
        .iter()
        .skip(start)
        .take(view_h)
        .map(|l| Line::from(Span::raw(l.clone())))
        .collect();
    frame.render_widget(Paragraph::new(rows), chunk(&chunks, 0));
    let hint = Line::from(Span::styled(hint.to_string(), theme::dim()));
    frame.render_widget(Paragraph::new(hint), chunk(&chunks, 1));
}

/// Draw the "unsaved changes" prompt shown before closing a modified tab.
pub(super) fn draw_unsaved(app: &App, frame: &mut Frame, area: Rect) {
    let Some(u) = app.unsaved.as_ref() else {
        return;
    };
    let message = t!("ui.unsaved_prompt", name = u.name).to_string();
    let choices = t!("ui.unsaved_choices");
    let width = (u16::try_from(message.chars().count().max(choices.chars().count()))
        .unwrap_or(u16::MAX))
    .plus(6)
    .min(area.width);
    let rect = Rect {
        x: area.x.plus(area.width.minus(width).quot(2)),
        y: area.y.plus(area.height.quot(3)),
        width,
        height: 4,
    };
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} ", t!("ui.unsaved_title")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    let lines = vec![
        Line::from(message),
        Line::from(Span::styled(choices.to_string(), theme::dim())),
    ];
    frame.render_widget(Paragraph::new(lines), inner);
    right_align(app, frame, inner);
}

/// Draw the prompt shown when pasting a file whose name already exists in the
/// target folder, offering the resolution choices.
pub(super) fn draw_paste_conflict(app: &App, frame: &mut Frame, area: Rect) {
    let Some(op) = app.paste.as_ref() else { return };
    let Some(src) = op.conflict.as_ref() else {
        return;
    };
    let name = src
        .file_name()
        .map_or_default(|s| s.to_string_lossy().into_owned());
    let lines = vec![
        Line::from(t!("ui.paste_exists", name = name).to_string()),
        Line::from(Span::styled(t!("ui.paste_choices"), theme::dim())),
    ];
    let width = 56u16.min(area.width);
    let rect = Rect {
        x: area.x.plus((area.width.minus(width)).quot(2)),
        y: area.y.plus(area.height.quot(3)),
        width,
        height: 4,
    };
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} ", t!("ui.paste_conflict")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    frame.render_widget(Paragraph::new(lines), inner);
    right_align(app, frame, inner);
}

/// Draw the one-line query-replace (y/n/!/q) bar along the bottom of `area`.
pub(super) fn draw_query_replace(app: &App, frame: &mut Frame, area: Rect) {
    let Some(qr) = app.query_replace.as_ref() else {
        return;
    };
    let bar = Rect {
        x: area.x,
        y: area.y.plus(area.height.minus(2)),
        width: area.width,
        height: 1,
    };
    frame.render_widget(Clear, bar);
    let line = Line::from(vec![
        Span::styled(
            format!(" {} {} ", icon::SEARCH, t!("ui.qr_label")),
            Style::default(),
        ),
        Span::styled(format!(" «{}» ", qr.label), Style::default()),
        Span::raw("  "),
        Span::styled("y", theme::title(true)),
        Span::raw(format!(" {}  ", t!("ui.qr_replace"))),
        Span::styled("n", theme::title(true)),
        Span::raw(format!(" {}  ", t!("ui.qr_skip"))),
        Span::styled("!", theme::title(true)),
        Span::raw(format!(" {}  ", t!("ui.qr_rest"))),
        Span::styled("q", theme::title(true)),
        Span::raw(format!(" {}   ", t!("ui.qr_quit"))),
        Span::styled(t!("ui.qr_replaced", count = qr.replaced), theme::dim()),
    ]);
    frame.render_widget(Paragraph::new(line).style(theme::base()), bar);
}

/// The T201 structural-replace session bar, mirroring `draw_query_replace`
/// exactly (same `y`/`n`/`!`/`q` legend) but over `app.structural_replace`.
pub(super) fn draw_structural_replace(app: &App, frame: &mut Frame, area: Rect) {
    let Some(sr) = app.structural_replace.as_ref() else {
        return;
    };
    let bar = Rect {
        x: area.x,
        y: area.y.plus(area.height.minus(2)),
        width: area.width,
        height: 1,
    };
    frame.render_widget(Clear, bar);
    let line = Line::from(vec![
        Span::styled(
            format!(" {} {} ", icon::SEARCH, t!("ui.sr_label")),
            Style::default(),
        ),
        Span::styled(format!(" «{}» ", sr.label), Style::default()),
        Span::raw("  "),
        Span::styled("y", theme::title(true)),
        Span::raw(format!(" {}  ", t!("ui.qr_replace"))),
        Span::styled("n", theme::title(true)),
        Span::raw(format!(" {}  ", t!("ui.qr_skip"))),
        Span::styled("!", theme::title(true)),
        Span::raw(format!(" {}  ", t!("ui.qr_rest"))),
        Span::styled("q", theme::title(true)),
        Span::raw(format!(" {}   ", t!("ui.qr_quit"))),
        Span::styled(t!("ui.qr_replaced", count = sr.replaced), theme::dim()),
    ]);
    frame.render_widget(Paragraph::new(line).style(theme::base()), bar);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{
        Confirm, ConfirmKind, PasteOp, QueryReplace, ReplaceConfirm, ScriptTrustPrompt,
        StructuralReplace, UnsavedMode, UnsavedPrompt,
    };
    use crate::settings::Settings;
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};
    use std::collections::VecDeque;
    use std::path::PathBuf;

    fn app() -> App {
        App::new(std::env::temp_dir(), Settings::default())
    }

    /// Draw one overlay into a fresh `w` x `h` buffer.
    fn render(w: u16, h: u16, f: impl FnOnce(&mut Frame, Rect)) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                f(frame, area);
            })
            .unwrap();
        terminal.backend().buffer().clone()
    }

    fn rows(buf: &Buffer) -> Vec<String> {
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect()
    }

    fn text(buf: &Buffer) -> String {
        rows(buf).join("\n")
    }

    /// The drawn text with borders removed and whitespace collapsed, so a
    /// wrapped sentence can be searched for as one string.
    fn flat(buf: &Buffer) -> String {
        text(buf)
            .replace(['│', '╭', '╮', '╰', '╯', '─'], " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn blank(buf: &Buffer) -> bool {
        text(buf).chars().all(|c| c == ' ' || c == '\n')
    }

    #[test]
    fn confirm_draws_title_and_message_and_nothing_without_state() {
        let mut a = app();
        assert!(blank(&render(60, 12, |f, r| draw_confirm(&a, f, r))));
        a.confirm = Some(Confirm {
            message: "Really delete notes.txt?".to_string(),
            paths: Vec::new(),
            kind: ConfirmKind::Delete,
        });
        let buf = render(60, 12, |f, r| draw_confirm(&a, f, r));
        let t = text(&buf);
        assert!(t.contains(&t!("ui.confirm").to_string()));
        assert!(t.contains("Really delete notes.txt?"));
        // Centered box with rounded corners, one third down the screen.
        assert!(rows(&buf).get(4).is_some_and(|r| r.contains('╭')));
    }

    #[test]
    fn confirm_wraps_a_message_wider_than_the_screen() {
        let mut a = app();
        a.confirm = Some(Confirm {
            message: "alpha beta gamma delta epsilon zeta eta theta".to_string(),
            paths: Vec::new(),
            kind: ConfirmKind::Delete,
        });
        let buf = render(24, 14, |f, r| draw_confirm(&a, f, r));
        let r = rows(&buf);
        let first = r.iter().position(|l| l.contains("alpha")).unwrap();
        let last = r.iter().position(|l| l.contains("theta")).unwrap();
        assert!(last > first, "the message continues on later rows");
    }

    #[test]
    fn mirrored_layout_right_aligns_dialog_text() {
        let mut a = app();
        a.confirm = Some(Confirm {
            message: "Sure?".to_string(),
            paths: Vec::new(),
            kind: ConfirmKind::Delete,
        });
        let plain = rows(&render(60, 12, |f, r| draw_confirm(&a, f, r)));
        a.settings.mirror_layout = "on".to_string();
        let mirrored = rows(&render(60, 12, |f, r| draw_confirm(&a, f, r)));
        let col = |rs: &[String]| {
            rs.iter()
                .find_map(|l| l.find("Sure?").or_else(|| l.find("?eruS")))
                .unwrap()
        };
        assert!(col(&mirrored) >= col(&plain));
        assert_ne!(plain, mirrored, "the text moved to the right edge");
    }

    fn trust(count: usize, settings: bool, env: bool, lsp: Option<&str>) -> App {
        let mut a = app();
        let mut p = ScriptTrustPrompt {
            count,
            settings,
            env,
            language_server: lsp.map(str::to_string),
        };
        p.count = count;
        a.script_trust = Some(p);
        a
    }

    #[test]
    fn script_trust_titles_and_messages_follow_what_is_waiting() {
        let a = app();
        assert!(blank(&render(100, 20, |f, r| draw_script_trust(&a, f, r))));

        let scripts = trust(2, false, false, None);
        let t = flat(&render(100, 20, |f, r| draw_script_trust(&scripts, f, r)));
        assert!(t.contains(&t!("ui.script_trust_title").to_string()));
        let hint = t!("ui.script_trust_hint").to_string();
        assert!(t.contains(hint.chars().take(12).collect::<String>().as_str()));

        let both = trust(2, true, false, None);
        let t = flat(&render(100, 20, |f, r| draw_script_trust(&both, f, r)));
        assert!(t.contains(&t!("ui.workspace_trust_settings_also").to_string()));

        let settings = trust(0, true, false, None);
        let t = flat(&render(100, 20, |f, r| draw_script_trust(&settings, f, r)));
        assert!(t.contains(&t!("ui.workspace_trust_title").to_string()));

        let env = trust(0, false, true, None);
        let t = flat(&render(100, 20, |f, r| draw_script_trust(&env, f, r)));
        assert!(t.contains(&t!("ui.workspace_trust_title").to_string()));

        let env_also = trust(1, false, true, None);
        let t = flat(&render(120, 20, |f, r| draw_script_trust(&env_also, f, r)));
        assert!(t.contains(&t!("ui.workspace_trust_env_also").to_string()));

        let lsp = trust(0, false, false, Some("rust-analyzer"));
        let t = flat(&render(100, 20, |f, r| draw_script_trust(&lsp, f, r)));
        assert!(t.contains("rust-analyzer"));

        let lsp_also = trust(1, false, false, Some("gopls"));
        let t = flat(&render(100, 20, |f, r| draw_script_trust(&lsp_also, f, r)));
        assert!(t.contains("gopls"));
    }

    #[test]
    fn script_trust_box_is_taller_when_several_things_wait() {
        let one = trust(1, false, false, None);
        let many = trust(1, true, true, None);
        let height = |a: &App| {
            let buf = render(100, 20, |f, r| draw_script_trust(a, f, r));
            let r = rows(&buf);
            let top = r.iter().position(|l| l.contains('╭')).unwrap();
            let bottom = r.iter().position(|l| l.contains('╰')).unwrap();
            bottom - top + 1
        };
        assert_eq!(height(&one), 7);
        assert_eq!(height(&many), 9);
    }

    fn confirm_lines(n: usize) -> ReplaceConfirm {
        ReplaceConfirm {
            plan: vec![(PathBuf::from("a.txt"), "x".to_string())],
            replaced: 3,
            lines: (0..n).map(|i| format!("file{i}.txt (1)")).collect(),
            scroll: 0,
        }
    }

    #[test]
    fn replace_confirm_lists_files_and_scrolls() {
        let mut a = app();
        assert!(blank(&render(80, 24, |f, r| draw_replace_confirm(
            &a, f, r
        ))));
        a.replace_confirm = Some(confirm_lines(40));
        let t = text(&render(80, 24, |f, r| draw_replace_confirm(&a, f, r)));
        assert!(t.contains("file0.txt (1)"));
        assert!(!t.contains("file39.txt"));
        assert!(t.contains(&t!("ui.replace_confirm_hint").to_string()));
        a.replace_confirm.as_mut().unwrap().scroll = 10;
        let t = text(&render(80, 24, |f, r| draw_replace_confirm(&a, f, r)));
        assert!(!t.contains("file0.txt (1)"));
        assert!(t.contains("file10.txt (1)"));
        // Scrolling past the end clamps to the last row.
        a.replace_confirm.as_mut().unwrap().scroll = 1000;
        let t = text(&render(80, 24, |f, r| draw_replace_confirm(&a, f, r)));
        assert!(t.contains("file39.txt (1)"));
    }

    #[test]
    fn wgrep_confirm_uses_its_own_title_and_hint() {
        let mut a = app();
        assert!(blank(&render(80, 24, |f, r| draw_wgrep_confirm(&a, f, r))));
        a.wgrep_confirm = Some(confirm_lines(3));
        let t = text(&render(80, 24, |f, r| draw_wgrep_confirm(&a, f, r)));
        assert!(t.contains("file2.txt (1)"));
        assert!(t.contains(&t!("ui.wgrep_confirm_hint").to_string()));
    }

    #[test]
    fn dir_edit_confirm_draws_nothing_without_a_pending_listing() {
        let a = app();
        assert!(blank(&render(80, 24, |f, r| draw_dir_edit_confirm(
            &a, f, r
        ))));
    }

    #[test]
    fn unsaved_prompt_names_the_buffer_and_offers_choices() {
        let mut a = app();
        assert!(blank(&render(80, 24, |f, r| draw_unsaved(&a, f, r))));
        a.unsaved = Some(UnsavedPrompt {
            mode: UnsavedMode::CloseTab,
            name: "draft.md".to_string(),
        });
        let t = text(&render(80, 24, |f, r| draw_unsaved(&a, f, r)));
        assert!(t.contains("draft.md"));
        assert!(t.contains(&t!("ui.unsaved_title").to_string()));
        assert!(t.contains(&t!("ui.unsaved_choices").to_string()));
    }

    #[test]
    fn paste_conflict_names_the_clashing_file() {
        let mut a = app();
        assert!(blank(&render(80, 24, |f, r| draw_paste_conflict(&a, f, r))));
        let mut op = PasteOp {
            target: PathBuf::from("/dst"),
            cut: false,
            queue: VecDeque::new(),
            overwrite_all: false,
            skip_all: false,
            conflict: None,
        };
        a.paste = Some(op);
        // A paste with no pending conflict draws nothing.
        assert!(blank(&render(80, 24, |f, r| draw_paste_conflict(&a, f, r))));
        op = a.paste.take().unwrap();
        op.conflict = Some(PathBuf::from("/src/report.txt"));
        a.paste = Some(op);
        let t = text(&render(80, 24, |f, r| draw_paste_conflict(&a, f, r)));
        assert!(t.contains("report.txt"));
        assert!(t.contains(&t!("ui.paste_conflict").to_string()));
    }

    #[test]
    fn query_replace_bar_shows_label_and_replaced_count() {
        let mut a = app();
        assert!(blank(&render(100, 10, |f, r| draw_query_replace(&a, f, r))));
        a.query_replace = Some(QueryReplace {
            re: regex::Regex::new("foo").unwrap(),
            template: "bar".to_string(),
            regex: false,
            current: (0, 3),
            replaced: 4,
            label: "foo".to_string(),
        });
        let buf = render(100, 10, |f, r| draw_query_replace(&a, f, r));
        let line = rows(&buf).get(8).cloned().unwrap();
        assert!(line.contains("«foo»"));
        assert!(line.contains(&t!("ui.qr_replaced", count = 4).to_string()));
        assert!(line.contains(&t!("ui.qr_quit").to_string()));
    }

    #[test]
    fn structural_replace_bar_shows_its_own_label() {
        let mut a = app();
        assert!(blank(&render(100, 10, |f, r| draw_structural_replace(
            &a, f, r
        ))));
        a.structural_replace = Some(StructuralReplace {
            pattern: vix_structural_replace::Pattern::compile("foo($X)").unwrap(),
            template: "bar($X)".to_string(),
            current: (0, 3),
            bounds: None,
            replaced: 2,
            label: "foo($X)".to_string(),
        });
        let buf = render(100, 10, |f, r| draw_structural_replace(&a, f, r));
        let line = rows(&buf).get(8).cloned().unwrap();
        assert!(line.contains("«foo($X)»"));
        assert!(line.contains(&t!("ui.sr_label").to_string()));
        assert!(line.contains(&t!("ui.qr_replaced", count = 2).to_string()));
    }
}
