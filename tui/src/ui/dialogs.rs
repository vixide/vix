//! Small modal dialogs: the generic yes/no confirm box, the workspace-script
//! trust prompt, the find/replace confirmation viewer, the unsaved-changes
//! prompt, the paste-name-conflict prompt, and the interactive query-replace
//! status bar.

use super::ClampFit;
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

pub(super) fn draw_confirm(app: &App, frame: &mut Frame, area: Rect) {
    let Some(c) = app.confirm.as_ref() else {
        return;
    };
    // Display columns, so wide (CJK) text is measured as it is drawn.
    let cols = u16::try_from(Line::from(c.message.as_str()).width()).unwrap_or(u16::MAX);
    let width = cols.saturating_add(6).min(area.width);
    // A message wider than the screen (Cowork's terminal-sharing warning,
    // T697) wraps onto more lines instead of being cut off.
    let text_width = width.saturating_sub(2).max(1);
    // Word wrapping can need one line more than the column count says.
    let lines = if cols <= text_width {
        1
    } else {
        cols.div_ceil(text_width) + 1
    };
    let y = area.y + area.height / 3;
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y,
        width,
        height: lines.saturating_add(2).min(area.bottom().saturating_sub(y)),
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
    let width = (area.width * 3 / 5).clamp(40, 64).min(area.width);
    // Two more rows when the message also mentions the settings file.
    let height = if p.count > 0 && p.settings { 8u16 } else { 6 }.min(area.height);
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + area.height / 3,
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
        (false, _) => t!("ui.workspace_trust_settings_message").to_string(),
    };
    frame.render_widget(Paragraph::new(message).wrap(Wrap { trim: true }), chunks[0]);
    let hint = Line::from(Span::styled(
        t!("ui.script_trust_hint").to_string(),
        theme::dim(),
    ));
    frame.render_widget(Paragraph::new(hint), chunks[1]);
    right_align(app, frame, inner);
}

pub(super) fn draw_replace_confirm(app: &App, frame: &mut Frame, area: Rect) {
    let Some(rc) = app.replace_confirm.as_ref() else {
        return;
    };
    let width = (area.width * 7 / 10).clamp_fit(30, area.width);
    let height = (area.height * 6 / 10).clamp_fit(8, area.height);
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 3,
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
    let view_h = chunks[0].height as usize;
    let start = rc.scroll.min(rc.lines.len().saturating_sub(1));
    let lines: Vec<Line> = rc
        .lines
        .iter()
        .skip(start)
        .take(view_h)
        .map(|l| Line::from(Span::raw(l.clone())))
        .collect();
    frame.render_widget(Paragraph::new(lines), chunks[0]);
    let hint = Line::from(Span::styled(
        t!("ui.replace_confirm_hint").to_string(),
        theme::dim(),
    ));
    frame.render_widget(Paragraph::new(hint), chunks[1]);
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
    let width = (area.width * 7 / 10).clamp_fit(30, area.width);
    let height = (area.height * 6 / 10).clamp_fit(8, area.height);
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 3,
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
    let view_h = chunks[0].height as usize;
    let start = scroll.min(lines.len().saturating_sub(1));
    let rows: Vec<Line> = lines
        .iter()
        .skip(start)
        .take(view_h)
        .map(|l| Line::from(Span::raw(l.clone())))
        .collect();
    frame.render_widget(Paragraph::new(rows), chunks[0]);
    let hint = Line::from(Span::styled(hint.to_string(), theme::dim()));
    frame.render_widget(Paragraph::new(hint), chunks[1]);
}

pub(super) fn draw_unsaved(app: &App, frame: &mut Frame, area: Rect) {
    let Some(u) = app.unsaved.as_ref() else {
        return;
    };
    let message = t!("ui.unsaved_prompt", name = u.name).to_string();
    let choices = t!("ui.unsaved_choices");
    let width = (u16::try_from(message.chars().count().max(choices.chars().count()))
        .unwrap_or(u16::MAX)
        + 6)
    .min(area.width);
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height / 3,
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

pub(super) fn draw_paste_conflict(app: &App, frame: &mut Frame, area: Rect) {
    let Some(op) = app.paste.as_ref() else { return };
    let Some(src) = op.conflict.as_ref() else {
        return;
    };
    let name = src
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let lines = vec![
        Line::from(t!("ui.paste_exists", name = name).to_string()),
        Line::from(Span::styled(t!("ui.paste_choices"), theme::dim())),
    ];
    let width = 56u16.min(area.width);
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + area.height / 3,
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

pub(super) fn draw_query_replace(app: &App, frame: &mut Frame, area: Rect) {
    let Some(qr) = app.query_replace.as_ref() else {
        return;
    };
    let bar = Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(2),
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
        y: area.y + area.height.saturating_sub(2),
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
