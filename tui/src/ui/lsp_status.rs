//! View → LSP Status… (T732): every configured language server as a block —
//! a header with its name and state, then its command, root, extensions,
//! open files, in-flight progress, capabilities and last error. Read live
//! from [`crate::lsp::Lsp::statuses`] each frame.

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use crate::app::App;
use crate::lsp::{ServerError, ServerState, ServerStatus};
use crate::theme::{self, icon};

/// The translated label for a server's lifecycle state.
fn state_label(state: ServerState) -> String {
    match state {
        ServerState::Starting => t!("ui.lsp_state_starting"),
        ServerState::Running => t!("ui.lsp_state_running"),
        ServerState::Failed => t!("ui.lsp_state_failed"),
        // `NotStarted`, and any state added later.
        _ => t!("ui.lsp_state_not_started"),
    }
    .to_string()
}

/// The translated text for a server's last error.
fn error_text(error: Option<&ServerError>) -> String {
    match error {
        None => t!("ui.lsp_status_none").to_string(),
        Some(ServerError::SpawnFailed) => t!("ui.lsp_error_spawn_failed").to_string(),
        Some(ServerError::Crashed) => t!("ui.lsp_error_crashed").to_string(),
        Some(ServerError::Request(message)) => message.clone(),
        // Any error kind added later.
        Some(_) => t!("ui.lsp_state_failed").to_string(),
    }
}

/// Greedy word-wrap of `items` joined by `", "` into lines of at most
/// `width` characters (an item longer than `width` gets a line of its own).
fn wrap_list(items: &[String], width: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    for (i, item) in items.iter().enumerate() {
        let piece = if i + 1 < items.len() {
            format!("{item},")
        } else {
            item.clone()
        };
        let need = cur.chars().count() + usize::from(!cur.is_empty()) + piece.chars().count();
        if !cur.is_empty() && need > width {
            lines.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(&piece);
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

/// One server's block of lines (header first, then labelled rows).
fn server_lines(s: &ServerStatus, selected: bool, width: usize) -> Vec<Line<'static>> {
    let header = format!(" {}  ·  {}", s.language_id, state_label(s.state));
    let style = if selected {
        theme::selected()
    } else {
        theme::title(true)
    };
    let mut out = vec![Line::from(Span::styled(header, style))];
    let none = t!("ui.lsp_status_none").to_string();
    let mut rows: Vec<(String, Vec<String>)> = vec![
        (
            t!("ui.lsp_status_command").to_string(),
            vec![s.command.clone()],
        ),
        (
            t!("ui.lsp_status_root").to_string(),
            vec![
                s.root
                    .as_ref()
                    .map_or_else(|| none.clone(), |r| r.display().to_string()),
            ],
        ),
        (
            t!("ui.lsp_status_extensions").to_string(),
            vec![s.extensions.join(", ")],
        ),
        (
            t!("ui.lsp_status_open_files").to_string(),
            vec![s.open_documents.to_string()],
        ),
    ];
    rows.push((
        t!("ui.lsp_status_progress").to_string(),
        if s.progress.is_empty() {
            vec![none.clone()]
        } else {
            s.progress.clone()
        },
    ));
    let label_w = [
        t!("ui.lsp_status_capabilities").chars().count(),
        t!("ui.lsp_status_last_error").chars().count(),
    ]
    .into_iter()
    .chain(rows.iter().map(|(l, _)| l.chars().count()))
    .max()
    .unwrap_or(0);
    let value_w = width.saturating_sub(label_w + 5).max(10);
    let caps = if s.capabilities.is_empty() {
        vec![none]
    } else {
        wrap_list(&s.capabilities, value_w)
    };
    rows.push((t!("ui.lsp_status_capabilities").to_string(), caps));
    rows.push((
        t!("ui.lsp_status_last_error").to_string(),
        vec![error_text(s.last_error.as_ref())],
    ));
    for (label, values) in rows {
        for (i, value) in values.into_iter().enumerate() {
            let shown = if i == 0 { label.as_str() } else { "" };
            let pad = label_w.saturating_sub(shown.chars().count());
            out.push(Line::from(vec![
                Span::styled(format!("   {shown}{}  ", " ".repeat(pad)), theme::dim()),
                Span::raw(value),
            ]));
        }
    }
    out
}

/// Draw the LSP Status overlay, centered over `area`.
pub(super) fn draw_lsp_status(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(selected) = app.lsp_status.as_ref().map(|p| p.selected) else {
        return;
    };
    let statuses = app.lsp.statuses();
    let selected = selected.min(statuses.len().saturating_sub(1));
    let width = 86u16.min(area.width);
    let inner_w = usize::from(width.saturating_sub(2));

    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut starts = Vec::with_capacity(statuses.len());
    if !app.lsp.enabled() {
        lines.push(Line::from(Span::styled(
            format!(" {}", t!("ui.lsp_status_off")),
            theme::dim(),
        )));
    } else if statuses.is_empty() {
        lines.push(Line::from(Span::styled(
            format!(" {}", t!("ui.lsp_status_no_servers")),
            theme::dim(),
        )));
    }
    for (i, s) in statuses.iter().enumerate() {
        if !lines.is_empty() {
            lines.push(Line::default());
        }
        starts.push(lines.len());
        lines.extend(server_lines(s, i == selected, inner_w));
    }

    let wanted = u16::try_from(lines.len())
        .unwrap_or(u16::MAX)
        .saturating_add(3);
    let height = wanted
        .min(area.height.saturating_sub(2).max(4))
        .min(area.height);
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 4,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} {} ", icon::INFO, t!("ui.lsp_status_title")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);
    let view_h = usize::from(chunks[0].height).max(1);

    // Keep the highlighted server's whole block in view when it fits, and
    // its header in view always.
    let start = starts.get(selected).copied().unwrap_or(0);
    let end = starts.get(selected + 1).map_or(lines.len(), |&n| n - 1);
    let Some(panel) = app.lsp_status.as_mut() else {
        return;
    };
    panel.selected = selected;
    if end > panel.scroll + view_h {
        panel.scroll = end.saturating_sub(view_h).min(start);
    }
    if start < panel.scroll {
        panel.scroll = start;
    }
    panel.scroll = panel.scroll.min(lines.len().saturating_sub(view_h));
    let scroll = panel.scroll;
    panel.starts = starts;

    let visible: Vec<Line> = lines.into_iter().skip(scroll).take(view_h).collect();
    frame.render_widget(Paragraph::new(visible), chunks[0]);
    let hint = Line::from(Span::styled(
        t!("ui.lsp_status_hint").to_string(),
        theme::dim(),
    ));
    frame.render_widget(Paragraph::new(hint), chunks[1]);
    app.layout.lsp_status = chunks[0];
}

#[cfg(test)]
mod tests {
    use super::wrap_list;

    #[test]
    fn wrap_list_breaks_between_items() {
        let items: Vec<String> = ["hover", "definition", "rename"]
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(wrap_list(&items, 18), ["hover, definition,", "rename"]);
        assert_eq!(wrap_list(&items, 80), ["hover, definition, rename"]);
        assert_eq!(wrap_list(&[], 10), Vec::<String>::new());
    }
}
