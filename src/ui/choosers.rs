//! Small centered overlay pickers: the branch/workspace/macro/clipboard/
//! task/script/location/capture/refile list choosers (all built on the
//! shared `draw_list_chooser`), the diff viewer, the Git status panel, the
//! right-click context menu, and the spell-check suggestion popup.

#![warn(clippy::pedantic)]

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph};

use super::{git_change_color, trunc, unix_secs_label};
use crate::app::App;
use crate::theme::{self, icon};

pub(super) fn draw_branch_chooser(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(c) = app.branch_chooser.as_ref() else {
        return;
    };
    let hint = t!("ui.branch_hint");
    app.layout.chooser = draw_list_chooser(
        frame,
        area,
        &t!("ui.branch"),
        &hint,
        &c.branches,
        c.selected,
    );
}

pub(super) fn draw_diff_view(app: &mut App, frame: &mut Frame, area: Rect) {
    use crate::diff_view::Kind;
    let Some(d) = app.diff_view.as_ref() else {
        return;
    };
    let width = (area.width * 8 / 10).clamp(30, area.width);
    let height = (area.height * 8 / 10).clamp(8, area.height);
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
        .title(format!(" {} {} ", icon::INFO, d.title));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);
    let view_h = chunks[0].height as usize;
    let start = d.scroll.min(d.lines.len().saturating_sub(1));
    let lines: Vec<Line> = d
        .lines
        .iter()
        .skip(start)
        .take(view_h)
        .map(|l| {
            let (prefix, style) = match l.kind {
                Kind::Add => ("+ ", Style::default().fg(Color::Green)),
                Kind::Del => ("- ", Style::default().fg(Color::Red)),
                Kind::Context => ("  ", theme::dim()),
                Kind::Sep => ("  ", theme::dim().add_modifier(Modifier::DIM)),
            };
            if l.emphasis.is_empty() {
                return Line::from(Span::styled(format!("{prefix}{}", l.text), style));
            }
            // Word-level diff: emphasize the changed spans (bold + reversed) while
            // the rest of the changed line keeps its add/del color.
            let emph = style.add_modifier(Modifier::BOLD | Modifier::REVERSED);
            let chars: Vec<char> = l.text.chars().collect();
            let mut spans = vec![Span::styled(prefix, style)];
            let mut i = 0;
            for &(s, e) in &l.emphasis {
                if s > i {
                    spans.push(Span::styled(chars[i..s].iter().collect::<String>(), style));
                }
                spans.push(Span::styled(
                    chars[s.min(chars.len())..e.min(chars.len())]
                        .iter()
                        .collect::<String>(),
                    emph,
                ));
                i = e;
            }
            if i < chars.len() {
                spans.push(Span::styled(chars[i..].iter().collect::<String>(), style));
            }
            Line::from(spans)
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), chunks[0]);
    let hint = Line::from(Span::styled(
        t!("ui.diff_view_hint").to_string(),
        theme::dim(),
    ));
    frame.render_widget(Paragraph::new(hint), chunks[1]);
}

pub(super) fn draw_workspace_chooser(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(c) = app.workspace_chooser.as_ref() else {
        return;
    };
    let hint = t!("ui.projects_hint");
    app.layout.chooser =
        draw_list_chooser(frame, area, &t!("ui.projects"), &hint, &c.roots, c.selected);
}

pub(super) fn draw_macro_chooser(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(c) = app.macro_chooser.as_ref() else {
        return;
    };
    let labels: Vec<String> = c
        .macros
        .iter()
        .map(|m| format!("{} ({} keys)", m.name, m.keys.len()))
        .collect();
    let hint = t!("ui.macros_hint");
    app.layout.chooser =
        draw_list_chooser(frame, area, &t!("ui.macros"), &hint, &labels, c.selected);
}

pub(super) fn draw_clipboard_chooser(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(c) = app.clipboard_chooser.as_ref() else {
        return;
    };
    // Show a one-line preview of each entry (newlines/tabs flattened).
    let labels: Vec<String> = c
        .entries
        .iter()
        .map(|e| {
            let flat: String = e.split_whitespace().collect::<Vec<_>>().join(" ");
            flat.chars().take(80).collect()
        })
        .collect();
    let hint = t!("ui.clipboard_hint");
    app.layout.chooser =
        draw_list_chooser(frame, area, &t!("ui.clipboard"), &hint, &labels, c.selected);
}

pub(super) fn draw_task_chooser(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(c) = app.task_chooser.as_ref() else {
        return;
    };
    // Show "name — command" so the action is clear before running it.
    let labels: Vec<String> = c
        .tasks
        .iter()
        .map(|t| format!("{} — {}", t.name, t.command))
        .collect();
    let hint = t!("ui.tasks_hint");
    app.layout.chooser =
        draw_list_chooser(frame, area, &t!("ui.tasks"), &hint, &labels, c.selected);
}

pub(super) fn draw_script_chooser(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(c) = app.script_chooser.as_ref() else {
        return;
    };
    // Script-authored label first (shown verbatim), the owning script's file
    // stem second — two scripts can register the same label.
    let labels: Vec<String> = c
        .commands
        .iter()
        .map(|(stem, cmd)| format!("{} — {stem}", cmd.label))
        .collect();
    let hint = t!("ui.scripts_hint");
    app.layout.chooser =
        draw_list_chooser(frame, area, &t!("ui.scripts"), &hint, &labels, c.selected);
}

pub(super) fn draw_git_panel(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.git_panel.is_none() {
        return;
    }
    let selected = app.git_panel.as_ref().unwrap().selected;
    let rows = app.git_status.len().max(1);
    let width = 64u16.min(area.width);
    let max_rows = area.height.saturating_sub(4).max(1);
    let visible = u16::try_from(rows).unwrap_or(u16::MAX).min(max_rows);
    let height = (visible + 4).min(area.height);
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 3,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let title = match app.git_branch.as_deref() {
        Some(b) => format!(" {} {} — {} ", icon::BRANCH, t!("ui.git_changes"), b),
        None => format!(" {} {} ", icon::BRANCH, t!("ui.git_changes")),
    };
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

    if app.git_status.is_empty() {
        let clean = Line::from(Span::styled(t!("ui.git_clean").to_string(), theme::dim()));
        frame.render_widget(Paragraph::new(clean), chunks[0]);
    } else {
        let items: Vec<ListItem> = app
            .git_status
            .iter()
            .map(|s| {
                // "[x] M  path" — [x] when staged; the letter is colored by change.
                let staged = if s.is_staged() { "[\u{2713}]" } else { "[ ]" };
                let change = s.primary();
                let letter = change.map_or(' ', crate::git::Change::letter);
                let color = change.map_or(Color::Gray, git_change_color);
                ListItem::new(Line::from(vec![
                    Span::raw(format!("  {staged} ")),
                    Span::styled(format!("{letter} "), Style::default().fg(color)),
                    Span::raw(s.path.clone()),
                ]))
            })
            .collect();
        let list = List::new(items).highlight_style(theme::selected());
        let mut state = ListState::default();
        state.select(Some(selected.min(app.git_status.len() - 1)));
        frame.render_stateful_widget(list, chunks[0], &mut state);
    }

    let hint = Line::from(Span::styled(
        t!("ui.git_changes_hint").to_string(),
        theme::dim(),
    ));
    frame.render_widget(Paragraph::new(hint), chunks[1]);

    app.layout.git_panel = Rect {
        x: chunks[0].x,
        y: chunks[0].y,
        width: chunks[0].width,
        height: u16::try_from(app.git_status.len())
            .unwrap_or(u16::MAX)
            .min(chunks[0].height),
    };
}

pub(super) fn draw_context_menu(app: &mut App, frame: &mut Frame, area: Rect) {
    use crate::app::CONTEXT_ITEMS;
    let Some(cm) = app.context_menu.as_ref() else {
        return;
    };
    let labels: Vec<String> = CONTEXT_ITEMS
        .iter()
        .map(|&(label, action)| {
            if action == "menu.separator" {
                String::new()
            } else {
                t!(label).to_string()
            }
        })
        .collect();
    let width = (u16::try_from(labels.iter().map(|l| l.chars().count()).max().unwrap_or(8))
        .unwrap_or(u16::MAX)
        + 4)
    .min(area.width);
    let height = (u16::try_from(CONTEXT_ITEMS.len()).unwrap_or(u16::MAX) + 2).min(area.height);
    // Clamp so the menu stays on screen near the click.
    let x = cm.x.min(area.right().saturating_sub(width));
    let y = cm.y.min(area.bottom().saturating_sub(height));
    let rect = Rect {
        x,
        y,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let lines: Vec<Line> = CONTEXT_ITEMS
        .iter()
        .enumerate()
        .map(|(i, (_, action))| {
            if *action == "menu.separator" {
                Line::from(Span::styled("─".repeat(inner.width as usize), theme::dim()))
            } else {
                let text = format!(" {} ", labels[i]);
                if i == cm.selected {
                    Line::from(Span::styled(text, theme::selected()))
                } else {
                    Line::from(Span::raw(text))
                }
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
    app.layout.context_menu = rect;
}

pub(super) fn draw_spell_suggest(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(p) = app.spell_suggest.as_ref() else {
        return;
    };
    let title = t!("ui.spell_suggest", word = p.word).to_string();
    let hint = t!("ui.spell_suggest_hint");
    let rows = p.suggestions.len().max(1);
    let widest = p
        .suggestions
        .iter()
        .map(|s| s.chars().count())
        .max()
        .unwrap_or(0)
        .max(title.chars().count())
        .max(hint.chars().count());
    let width = (u16::try_from(widest).unwrap_or(u16::MAX) + 6).min(area.width);
    // Borders (2) + suggestion rows + hint (1).
    let height = (u16::try_from(rows).unwrap_or(u16::MAX) + 3).min(area.height);
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
        .title(format!(" {title} "));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    if p.suggestions.is_empty() {
        let none = Line::from(Span::styled(
            t!("ui.spell_no_suggestions").to_string(),
            theme::dim(),
        ));
        frame.render_widget(Paragraph::new(none), chunks[0]);
    } else {
        let items: Vec<ListItem> = p
            .suggestions
            .iter()
            .map(|s| ListItem::new(Line::from(format!("  {s}"))))
            .collect();
        let list = List::new(items).highlight_style(theme::selected());
        let mut state = ListState::default();
        state.select(Some(p.selected));
        frame.render_stateful_widget(list, chunks[0], &mut state);
    }

    let hint = Line::from(Span::styled(hint.to_string(), theme::dim()));
    frame.render_widget(Paragraph::new(hint), chunks[1]);

    app.layout.spell_suggest = Rect {
        x: chunks[0].x,
        y: chunks[0].y,
        width: chunks[0].width,
        height: u16::try_from(p.suggestions.len())
            .unwrap_or(u16::MAX)
            .min(chunks[0].height),
    };
}

/// Render a centered list-chooser overlay (theme/locale/keymap): a titled box
/// with one row per `labels` entry, the `selected` row highlighted, and a hint
/// line. Returns the list's rectangle so the caller can record it for mouse
/// hit-testing.
fn draw_list_chooser(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    hint: &str,
    labels: &[String],
    selected: usize,
) -> Rect {
    let width = 34u16.min(area.width);
    let height = (u16::try_from(labels.len()).unwrap_or(u16::MAX) + 4).min(area.height);
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
        .title(format!(" {} {} ", icon::PALETTE, title));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let items: Vec<ListItem> = labels
        .iter()
        .map(|label| ListItem::new(Line::from(format!("  {label}"))))
        .collect();
    let list = List::new(items).highlight_style(theme::selected());
    let mut state = ListState::default();
    state.select(Some(selected));
    frame.render_stateful_widget(list, rows[0], &mut state);

    let hint = Line::from(Span::styled(hint.to_string(), theme::dim()));
    frame.render_widget(Paragraph::new(hint), rows[1]);
    rows[0]
}

/// The recent-files chooser: a multi-column table — basename, path, size,
/// created at, modified at — with the highlighted row opened by Enter or a
/// click. Records the data-row rectangle for mouse hit-testing.
pub(super) fn draw_recent_chooser(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(rc) = app.recent_chooser.as_ref() else {
        return;
    };
    let width = 100u16.min(area.width);
    let rows_h = u16::try_from(rc.entries.len()).unwrap_or(u16::MAX);
    let height = (rows_h + 5).min(area.height);
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
        .title(format!(" {} {} ", icon::PALETTE, t!("ui.recent")));
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

    // Fixed name (22), size (9), and date (16 + 16) columns; path gets the rest.
    let name_w = 22usize;
    let path_w = (chunks[0].width as usize)
        .saturating_sub(name_w + 9 + 16 + 16 + 6)
        .max(8);
    let row_text = |name: &str, path: &str, size: &str, created: &str, modified: &str| {
        format!(
            " {:<name_w$} {:<path_w$} {:>9} {:<16} {:<16}",
            trunc(name, name_w),
            trunc(path, path_w),
            size,
            created,
            modified,
        )
    };
    let header = row_text(
        &t!("ui.file_browser_sort_name"),
        &t!("ui.recent_col_path"),
        &t!("ui.file_browser_sort_size"),
        &t!("ui.file_browser_sort_created"),
        &t!("ui.file_browser_sort_modified"),
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(header, theme::title(true)))),
        chunks[0],
    );

    let view_h = chunks[1].height as usize;
    let mut lines: Vec<Line> = Vec::with_capacity(view_h);
    for (row, e) in rc.entries.iter().enumerate().take(view_h) {
        let name = e
            .path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let dir = e
            .path
            .parent()
            .map(|d| d.display().to_string())
            .unwrap_or_default();
        let text = row_text(
            &name,
            &dir,
            &crate::file_browser_panel::size_label(e.size),
            &unix_secs_label(e.created),
            &unix_secs_label(e.modified),
        );
        let line = if row == rc.selected {
            Line::from(Span::styled(text, theme::selected()))
        } else {
            Line::from(Span::raw(text))
        };
        lines.push(line);
    }
    frame.render_widget(Paragraph::new(lines), chunks[1]);

    let hint = Line::from(Span::styled(t!("ui.recent_hint").to_string(), theme::dim()));
    frame.render_widget(Paragraph::new(hint), chunks[2]);

    // Mouse hit-testing maps clicks to data rows only (the header is not a row).
    app.layout.chooser = chunks[1];
}

pub(super) fn draw_location_chooser(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(lc) = app.location_chooser.as_ref() else {
        return;
    };
    let selected = lc.selected;
    // Show the file name and line first (survives truncation), then its directory.
    let labels: Vec<String> = lc
        .entries
        .iter()
        .map(|loc| {
            let name = loc
                .path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            match loc.path.parent() {
                Some(dir) if !dir.as_os_str().is_empty() => {
                    format!("{name}:{}  —  {}", loc.line, dir.display())
                }
                _ => format!("{name}:{}", loc.line),
            }
        })
        .collect();
    let hint = t!("ui.locations_hint");
    app.layout.chooser =
        draw_list_chooser(frame, area, &t!("ui.locations"), &hint, &labels, selected);
}

/// The Org-capture template chooser overlay (Org → Capture → Choose
/// Template…): every configured `org_capture_templates` entry as
/// `<description> (<key>)`.
pub(super) fn draw_capture_chooser(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(cc) = app.capture_chooser.as_ref() else {
        return;
    };
    let selected = cc.selected;
    let labels: Vec<String> = app
        .settings
        .org_capture_templates
        .iter()
        .map(|t| format!("{} ({})", t.description, t.key))
        .collect();
    let hint = t!("ui.capture_chooser_hint");
    app.layout.chooser = draw_list_chooser(
        frame,
        area,
        &t!("ui.capture_chooser"),
        &hint,
        &labels,
        selected,
    );
}

/// The Org refile-target chooser (Org → Edit Structure → Refile Subtree…):
/// every candidate headline, indented by level.
pub(super) fn draw_refile_chooser(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(c) = app.refile_chooser.as_ref() else {
        return;
    };
    let selected = c.selected;
    let labels: Vec<String> = c.targets.iter().map(|(_, l)| l.clone()).collect();
    let hint = t!("ui.refile_chooser_hint");
    app.layout.chooser = draw_list_chooser(
        frame,
        area,
        &t!("ui.refile_chooser"),
        &hint,
        &labels,
        selected,
    );
}
