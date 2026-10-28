//! The Git graph overlay (T681): lanes, hash, ref decorations, subject and
//! author per commit, with a filter line above and a hint (or the pending
//! confirmation / branch-name prompt) below.

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use vix_git_graph::{Action, Commit, Mode, Panel, RefKind};

use super::{draw_scrollbar, trunc};
use crate::app::App;
use crate::theme::{self, icon};

/// The decoration text for a commit: `main, v1.0` with a `*` before the
/// branch `HEAD` is on, or empty when nothing points at it.
fn decoration(c: &Commit) -> String {
    let names: Vec<String> = c
        .refs
        .iter()
        .map(|r| {
            let mark = match (r.is_head, r.kind) {
                (true, _) => "*",
                (_, RefKind::Tag) => "#",
                _ => "",
            };
            format!("{mark}{}", r.name)
        })
        .collect();
    if names.is_empty() {
        String::new()
    } else {
        format!("({}) ", names.join(", "))
    }
}

/// The bottom line: the key hint, or the question/input of a pending action.
fn footer(p: &Panel) -> String {
    match &p.mode {
        Mode::Browse => t!("ui.git_graph_hint").to_string(),
        Mode::Confirm { action, sha } => {
            let abbrev = p
                .commits
                .iter()
                .find(|c| &c.sha == sha)
                .map_or(sha.as_str(), |c| c.abbrev.as_str());
            match action {
                Action::Checkout => t!("ui.git_graph_confirm_checkout", sha = abbrev).to_string(),
                Action::CherryPick => {
                    t!("ui.git_graph_confirm_cherry_pick", sha = abbrev).to_string()
                }
            }
        }
        Mode::NewBranch { name, .. } => t!("ui.git_graph_new_branch", name = name).to_string(),
    }
}

pub(super) fn draw_git_graph(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.git_graph.is_none() {
        return;
    }
    let width = 120u16.min(area.width);
    let height = area.height.saturating_sub(2).max(6).min(area.height);
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} {} ", icon::BRANCH, t!("ui.git_graph")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

    let view_h = chunks[1].height as usize;
    if let Some(p) = app.git_graph.as_mut() {
        p.ensure_visible(view_h);
    }
    let Some(p) = app.git_graph.as_ref() else {
        return;
    };

    // Filter line: search text and the active branch/tag filter.
    let scope = p.reference.as_deref().map_or_else(
        || t!("ui.git_graph_all_refs").to_string(),
        std::string::ToString::to_string,
    );
    let query = if p.query.is_empty() {
        t!("ui.git_graph_search").to_string()
    } else {
        format!("/ {}", p.query)
    };
    let more = if p.exhausted { "" } else { " …" };
    let header = Line::from(vec![
        Span::raw(query),
        Span::styled(format!("   [{scope}]{more}"), theme::dim()),
    ]);
    frame.render_widget(Paragraph::new(header), chunks[0]);

    let graph_w = p.graph_width();
    let avail = chunks[1].width as usize;
    let mut lines: Vec<Line> = Vec::with_capacity(view_h);
    for (row, &idx) in p.visible().iter().enumerate().skip(p.scroll).take(view_h) {
        let (c, r) = (&p.commits[idx], &p.rows[idx]);
        let graph = r.glyphs(graph_w);
        let deco = decoration(c);
        let tail = format!("  {} {}", trunc(&c.author, 16), c.date);
        let used = graph.chars().count() + c.abbrev.chars().count() + 2 + deco.chars().count();
        let subject_w = avail.saturating_sub(used + tail.chars().count()).max(8);
        let subject = trunc(&c.subject, subject_w);
        let pad = " ".repeat(subject_w.saturating_sub(subject.chars().count()));
        let selected = row == p.selected;
        let style = |s: Style| if selected { theme::selected() } else { s };
        lines.push(Line::from(vec![
            Span::styled(graph, style(theme::dim())),
            Span::styled(format!(" {} ", c.abbrev), style(Style::default())),
            Span::styled(deco, style(Style::default().add_modifier(Modifier::BOLD))),
            Span::styled(format!("{subject}{pad}"), style(Style::default())),
            Span::styled(tail, style(theme::dim())),
        ]));
    }
    let total = p.len();
    let show_bar = total > view_h && chunks[1].width > 1;
    let row_area = if show_bar {
        Rect {
            width: chunks[1].width - 1,
            ..chunks[1]
        }
    } else {
        chunks[1]
    };
    frame.render_widget(Paragraph::new(lines), row_area);
    if show_bar {
        let sb_area = Rect {
            x: chunks[1].x + chunks[1].width - 1,
            ..chunks[1]
        };
        draw_scrollbar(frame, sb_area, p.selected, total.saturating_sub(1));
    }
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(footer(p), theme::dim()))),
        chunks[2],
    );
    app.layout.git_graph = Rect {
        height: u16::try_from(view_h).unwrap_or(u16::MAX),
        ..row_area
    };
}
