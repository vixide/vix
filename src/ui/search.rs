//! Search-family overlays: the command palette, workspace-wide search/
//! replace, the in-buffer find/replace box, and the generic prompt overlay
//! (including its Org-capture preview).

#![warn(clippy::pedantic)]

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph};

use super::centered;
use crate::app::App;
use crate::search::{Field, Flags as SearchFlags, Scope};
use crate::theme::{self, icon};
use crate::workspace_search::Flags as WorkspaceFlags;

pub(super) fn draw_palette(app: &App, frame: &mut Frame, area: Rect) {
    let Some(p) = app.palette.as_ref() else {
        return;
    };
    let rect = centered(area, 70, 70);
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(
            " {} {} [{}] ",
            icon::SEARCH,
            t!("ui.command_palette"),
            p.mode().label()
        ));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

    let input = Line::from(vec![
        Span::styled("\u{276f} ", theme::title(true)),
        Span::raw(p.input.clone()),
        Span::styled("\u{2588}", theme::dim()),
    ]);
    frame.render_widget(Paragraph::new(input), rows[0]);

    let items: Vec<ListItem> = p
        .entries
        .iter()
        .map(|e| ListItem::new(Line::from(e.label.clone())))
        .collect();
    let list = List::new(items).highlight_style(theme::selected());
    let mut state = ListState::default();
    if !p.entries.is_empty() {
        state.select(Some(p.selected));
    }
    frame.render_stateful_widget(list, rows[1], &mut state);

    let hint = Line::from(Span::styled(t!("ui.palette_prefixes"), theme::dim()));
    frame.render_widget(Paragraph::new(hint), rows[2]);
}

pub(super) fn draw_workspace_search(app: &App, frame: &mut Frame, area: Rect) {
    let Some(ps) = app.workspace_search.as_ref() else {
        return;
    };
    let rect = centered(area, 80, 80);
    frame.render_widget(Clear, rect);
    let replacing = ps.flags.contains(WorkspaceFlags::REPLACING);
    let title = if ps.flags.contains(WorkspaceFlags::STATIC_RESULTS) {
        format!(" {} {} ", icon::SEARCH, t!("ui.goto_definition"))
    } else if replacing {
        format!(" {} {} ", icon::SEARCH, t!("ui.search_replace_workspace"))
    } else {
        format!(" {} {} ", icon::SEARCH, t!("ui.search_workspace"))
    };
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(title);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    // find (+ replace) + include-path + exclude-path + toggles + status.
    let head = if replacing { 6 } else { 5 };
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(head),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

    let mut header = Vec::new();
    let q_focus = !replacing || ps.field == Field::Query;
    header.push(field_line(&t!("ui.field_find"), &ps.query, q_focus));
    if replacing {
        header.push(field_line(
            &t!("ui.field_replace"),
            &ps.replace,
            ps.field == Field::Replace,
        ));
    }
    header.push(field_line(
        &t!("ui.field_include"),
        &ps.include_path,
        ps.field == Field::IncludePath,
    ));
    header.push(field_line(
        &t!("ui.field_exclude"),
        &ps.exclude_path,
        ps.field == Field::ExcludePath,
    ));
    let toggle = |on: bool, label: &str| {
        let style = if on { theme::selected() } else { theme::dim() };
        Span::styled(format!(" {label} "), style)
    };
    header.push(Line::from(vec![
        toggle(
            ps.flags.contains(WorkspaceFlags::CASE_SENSITIVE),
            &t!("ui.toggle_case"),
        ),
        Span::raw(" "),
        toggle(
            ps.flags.contains(WorkspaceFlags::REGEX),
            &t!("ui.toggle_regex"),
        ),
    ]));
    header.push(Line::from(Span::styled(ps.status.clone(), theme::dim())));
    frame.render_widget(Paragraph::new(header), rows[0]);

    let items: Vec<ListItem> = ps
        .hits
        .iter()
        .map(|h| ListItem::new(Line::from(h.display.clone())))
        .collect();
    let list = List::new(items).highlight_style(theme::selected());
    let mut state = ListState::default();
    if !ps.hits.is_empty() {
        state.select(Some(ps.selected));
    }
    frame.render_stateful_widget(list, rows[1], &mut state);

    let hint = if replacing {
        t!("ui.ps_hint_replace")
    } else {
        t!("ui.ps_hint")
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(hint, theme::dim()))),
        rows[2],
    );
}

/// Render a left-to-right row of labeled buttons (each ` label `, one-cell gap),
/// returning each button's clickable rectangle (`Rect::default()` if it did not
/// fit).
fn button_row(frame: &mut Frame, row: Rect, buttons: &[(String, Style)]) -> Vec<Rect> {
    let mut rects = vec![Rect::default(); buttons.len()];
    let mut x = row.x;
    let right = row.x + row.width;
    for (i, (label, style)) in buttons.iter().enumerate() {
        let text = format!(" {label} ");
        let w = u16::try_from(text.chars().count()).unwrap_or(u16::MAX);
        if x >= right {
            break;
        }
        let w = w.min(right - x);
        let r = Rect {
            x,
            y: row.y,
            width: w,
            height: 1,
        };
        frame.render_widget(Paragraph::new(Line::from(Span::styled(text, *style))), r);
        rects[i] = r;
        x = x.saturating_add(w + 1);
    }
    rects
}

#[allow(clippy::too_many_lines)]
pub(super) fn draw_search(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(s) = app.search.as_ref() else { return };
    let replacing = s.flags.contains(SearchFlags::REPLACING);
    let height = if replacing { 6 } else { 4 };
    let width = area.width * 7 / 10;
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + 1,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let title = if s.flags.contains(SearchFlags::INTERACTIVE) {
        format!(" {} {} ", icon::SEARCH, t!("ui.query_replace"))
    } else if replacing {
        format!(" {} {} ", icon::SEARCH, t!("ui.find_replace"))
    } else {
        format!(" {} {} ", icon::SEARCH, t!("ui.find"))
    };
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(title);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    app.layout.search = inner;
    // Forget any stale button rects (they are re-recorded below when shown).
    app.layout.search_case = Rect::default();
    app.layout.search_word = Rect::default();
    app.layout.search_regex = Rect::default();
    app.layout.search_replace_toggle = Rect::default();
    app.layout.search_scope = Rect::default();
    app.layout.search_once = Rect::default();
    app.layout.search_ask = Rect::default();
    app.layout.search_all = Rect::default();

    // Rows: Find field, toggle buttons, [Replace field, replace buttons,]? status.
    let constraints: Vec<Constraint> = if replacing {
        vec![Constraint::Length(1); 5]
    } else {
        vec![Constraint::Length(1); 3]
    };
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    let q_focus = !replacing || s.field == Field::Query;
    frame.render_widget(
        Paragraph::new(field_line(&t!("ui.field_find"), &s.query, q_focus)),
        rows[0],
    );

    let options = vec![
        (
            t!("ui.toggle_case").to_string(),
            s.flags.contains(SearchFlags::CASE_SENSITIVE),
        ),
        (
            t!("ui.toggle_smartcase").to_string(),
            s.flags.contains(SearchFlags::SMART_CASE),
        ),
        (
            t!("ui.toggle_word").to_string(),
            s.flags.contains(SearchFlags::WHOLE_WORD),
        ),
        (
            t!("ui.toggle_regex").to_string(),
            s.flags.contains(SearchFlags::REGEX),
        ),
        // The two options that used to be separate menu items.
        (t!("ui.toggle_replace").to_string(), replacing),
        (
            t!(s.scope.label_key()).to_string(),
            s.scope != Scope::Buffer,
        ),
    ];
    // Everything the rows below need, taken while the box is still borrowed.
    let (replace_text, on_replace_field, status_text, interactive) = (
        s.replace.clone(),
        s.field == Field::Replace,
        s.status.clone(),
        s.flags.contains(SearchFlags::INTERACTIVE),
    );
    draw_search_options(app, frame, rows[1], &options);

    if replacing {
        frame.render_widget(
            Paragraph::new(field_line(
                &t!("ui.field_replace"),
                &replace_text,
                on_replace_field,
            )),
            rows[2],
        );
        // Once / Ask / All replace buttons (reverse-video, like pressable buttons).
        let actions = vec![
            (t!("ui.btn_once").to_string(), theme::selected()),
            (t!("ui.btn_ask").to_string(), theme::selected()),
            (t!("ui.btn_all").to_string(), theme::selected()),
        ];
        let arects = button_row(frame, rows[3], &actions);
        app.layout.search_once = arects[0];
        app.layout.search_ask = arects[1];
        app.layout.search_all = arects[2];
    }

    let status = if !status_text.is_empty() {
        status_text
    } else if interactive {
        t!("ui.search_hint_interactive").to_string()
    } else if replacing {
        t!("ui.search_hint_replace").to_string()
    } else {
        format!(
            "{}   {}",
            t!("ui.search_hint"),
            t!("ui.search_hint_options")
        )
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(status, theme::dim()))),
        rows[rows.len() - 1],
    );
}

/// Draw the find box's option row: the match toggles (Case / Smart case / Word /
/// Regex) plus the two options that replaced separate menu items — **Replace**
/// mode and the **In:** scope — and record each button's rectangle so a click can
/// hit it. Split out of [`draw_search`] to keep that within the line limit.
///
/// `options` arrives as `(label, is_on)` pairs, already read from the box, so
/// this can take `&mut App` to record the rectangles without holding a borrow of
/// the box itself.
fn draw_search_options(app: &mut App, frame: &mut Frame, row: Rect, options: &[(String, bool)]) {
    let style = |on: bool| if on { theme::selected() } else { theme::dim() };
    let buttons: Vec<(String, Style)> = options
        .iter()
        .map(|(label, on)| (label.clone(), style(*on)))
        .collect();
    let rects = button_row(frame, row, &buttons);
    app.layout.search_case = rects[0];
    app.layout.search_smartcase = rects[1];
    app.layout.search_word = rects[2];
    app.layout.search_regex = rects[3];
    app.layout.search_replace_toggle = rects[4];
    app.layout.search_scope = rects[5];
}

fn field_line(label: &str, value: &str, focused: bool) -> Line<'static> {
    let marker = if focused { "\u{276f}" } else { " " };
    let lstyle = if focused {
        theme::title(true)
    } else {
        theme::dim()
    };
    Line::from(vec![
        Span::styled(format!("{marker} {label} "), lstyle),
        Span::raw(value.to_string()),
    ])
}

/// Render an Org-capture template preview (see `App::capture_preview`) into
/// the top `preview_rows` of `inner`, plus a one-row divider, scrolled to keep
/// the field being filled in (marked `‹Label›`) visible. Returns the
/// remaining rect for the input area below.
fn draw_prompt_preview(frame: &mut Frame, inner: Rect, text: &str, preview_rows: u16) -> Rect {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(preview_rows),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(inner);
    let line_count = text.lines().count();
    let current_line = text
        .lines()
        .position(|l| l.contains('\u{2039}'))
        .unwrap_or(0);
    let scroll_y = u16::try_from(current_line)
        .unwrap_or(0)
        .saturating_sub(preview_rows / 2)
        .min(
            u16::try_from(line_count)
                .unwrap_or(0)
                .saturating_sub(preview_rows),
        );
    frame.render_widget(
        Paragraph::new(text)
            .style(theme::dim())
            .scroll((scroll_y, 0)),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "\u{2500}".repeat(rows[1].width as usize),
            theme::dim(),
        ))),
        rows[1],
    );
    rows[2]
}

pub(super) fn draw_prompt(app: &App, frame: &mut Frame, area: Rect) {
    let Some(p) = app.prompt.as_ref() else { return };
    // The workspace→dock search prompt shows case/regex toggles on a second line.
    let toggles = matches!(p.kind, crate::app::PromptKind::SearchToDock);
    // The git-commit prompt accepts a multi-line message (Alt+Enter = newline).
    let multiline = matches!(
        p.kind,
        crate::app::PromptKind::GitCommit
            | crate::app::PromptKind::OrgCaptureField
            | crate::app::PromptKind::OrgCaptureReview
    );
    let width = area.width * 6 / 10;
    let body_rows: u16 = if multiline {
        u16::try_from(p.input.split('\n').count())
            .unwrap_or(1)
            .clamp(1, 12)
            + 1
    } else if toggles {
        2
    } else {
        1
    };
    // Org-capture field prompts carry a live preview of the template in
    // progress (`App::capture_preview`), rendered above the input, capped so
    // the dialog can't outgrow the screen and scrolled to keep the field
    // being filled in (marked `‹Label›`) in view.
    let preview_line_count = p.preview.as_deref().map_or(0, |t| t.lines().count());
    let preview_rows = u16::try_from(preview_line_count).unwrap_or(0).min(14);
    let preview_extra = if preview_rows > 0 {
        preview_rows + 1
    } else {
        0
    };
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + area.height / 3,
        width,
        height: (body_rows + preview_extra + 2).min(area.height),
    };
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} ", p.title));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let inner = if preview_rows > 0 {
        draw_prompt_preview(
            frame,
            inner,
            p.preview.as_deref().unwrap_or_default(),
            preview_rows,
        )
    } else {
        inner
    };

    if multiline {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(inner);
        let parts: Vec<&str> = p.input.split('\n').collect();
        let last = parts.len().saturating_sub(1);
        let mut lines: Vec<Line> = Vec::with_capacity(parts.len());
        for (i, part) in parts.iter().enumerate() {
            let prefix = if i == 0 { "\u{276f} " } else { "  " };
            let mut spans = vec![
                Span::styled(prefix, theme::title(true)),
                Span::raw((*part).to_string()),
            ];
            if i == last {
                spans.push(Span::styled("\u{2588}", theme::dim()));
            }
            lines.push(Line::from(spans));
        }
        frame.render_widget(Paragraph::new(lines), rows[0]);
        let hint = Line::from(Span::styled(t!("ui.commit_hint").to_string(), theme::dim()));
        frame.render_widget(Paragraph::new(hint), rows[1]);
        return;
    }

    let input = Line::from(vec![
        Span::styled("\u{276f} ", theme::title(true)),
        Span::raw(p.input.clone()),
        Span::styled("\u{2588}", theme::dim()),
    ]);
    if toggles {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(1)])
            .split(inner);
        frame.render_widget(Paragraph::new(input), rows[0]);
        let on = |b: bool| if b { "on" } else { "off" };
        let hint = format!(
            "Alt C case: {}   Alt R regex: {}",
            on(p.case_sensitive),
            on(p.regex)
        );
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(hint, theme::dim()))),
            rows[1],
        );
    } else {
        frame.render_widget(Paragraph::new(input), inner);
    }
}
