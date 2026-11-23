//! Small centered overlay pickers: the branch/workspace/macro/clipboard/
//! task/script/location/capture/refile list choosers (all built on the
//! shared `draw_list_chooser`), the diff viewer, the Git status panel, the
//! right-click context menu, and the spell-check suggestion popup.

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
use crate::app::chooser::ChooserSlot;
use crate::geo::{Geo, chunk};
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph};

use super::{git_change_color, trunc, unix_secs_label};
use crate::app::App;
use crate::theme::{self, icon};

/// Draw the branch chooser, or the gutter-diff base chooser when the chooser
/// was opened for that purpose (its title and hint differ).
pub(super) fn draw_branch_chooser(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(c) = app.chooser.branch() else {
        return;
    };
    let (title, hint) = if c.gutter {
        (t!("ui.gutter_base"), t!("ui.gutter_base_hint"))
    } else {
        (t!("ui.branch"), t!("ui.branch_hint"))
    };
    app.layout.chooser = draw_list_chooser(frame, area, &title, &hint, &c.branches, c.selected);
}

/// **Git → Stash List…** (T634): one row per stash, `stash@{N}  message`.
pub(super) fn draw_stash_chooser(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(c) = app.chooser.stash() else {
        return;
    };
    let labels: Vec<String> = c
        .entries
        .iter()
        .map(|e| format!("{}  {}", e.reference(), e.message))
        .collect();
    let hint = t!("ui.git_stash_hint");
    app.layout.chooser = draw_list_chooser_sized(
        frame,
        area,
        (&t!("ui.git_stash_list"), &hint),
        &labels,
        c.selected,
        72,
    );
}

/// **Git → Worktree** (T634): one row per worktree — path, branch (or a
/// detached/bare tag), and its current/locked/missing marks.
pub(super) fn draw_worktree_chooser(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(c) = app.chooser.worktree() else {
        return;
    };
    let current = c.current(&app.root);
    let labels: Vec<String> = c
        .worktrees
        .iter()
        .enumerate()
        .map(|(i, w)| {
            let what = match &w.checkout {
                crate::git::Checkout::Branch(b) => b.clone(),
                crate::git::Checkout::Detached => {
                    let sha = w.head.as_deref().unwrap_or("");
                    format!(
                        "{} {}",
                        t!("ui.git_worktree_detached"),
                        sha.get(..7).unwrap_or(sha)
                    )
                }
                crate::git::Checkout::Bare => t!("ui.git_worktree_bare").to_string(),
            };
            let mut marks = Vec::new();
            if current == Some(i) {
                marks.push(t!("ui.git_worktree_current"));
            }
            if w.locked {
                marks.push(t!("ui.git_worktree_locked"));
            }
            if w.prunable {
                marks.push(t!("ui.git_worktree_missing"));
            }
            let marks = if marks.is_empty() {
                String::new()
            } else {
                format!("  ({})", marks.join(", "))
            };
            format!("{}  [{what}]{marks}", w.path.display())
        })
        .collect();
    let (title, hint) = if c.remove {
        (
            t!("ui.git_worktree_remove"),
            t!("ui.git_worktree_remove_hint"),
        )
    } else {
        (t!("ui.git_worktrees"), t!("ui.git_worktree_hint"))
    };
    app.layout.chooser =
        draw_list_chooser_sized(frame, area, (&title, &hint), &labels, c.selected, 80);
}

/// **Git → Rebase → Interactive Rebase…** (T633): the todo list, oldest
/// commit first, one `action  abbrev  subject` row per commit (a typed
/// reword message shown after an arrow), and the key hint.
pub(super) fn draw_git_rebase(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(n) = app.git_rebase.as_ref().map(|t| t.commits.len()) else {
        return;
    };
    let width = 90u16.min(area.width);
    let max_rows = area.height.minus(4).max(1);
    let rows = u16::try_from(n).unwrap_or(u16::MAX).min(max_rows);
    let height = rows.plus(4).min(area.height);
    let rect = Rect {
        x: area.x.plus(area.width.minus(width).quot(2)),
        y: area.y.plus(area.height.minus(height).quot(3)),
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let base = app
        .git_rebase
        .as_ref()
        .map_or_default(|t| t.base_label.clone());
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(
            " {} {} ",
            icon::BRANCH,
            t!("ui.git_rebase", base = base)
        ));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);
    let view_h = usize::from(chunk(&chunks, 0).height);
    let Some(t) = app.git_rebase.as_mut() else {
        return;
    };
    t.ensure_visible(view_h);
    let lines: Vec<Line> = t
        .commits
        .iter()
        .enumerate()
        .skip(t.scroll)
        .take(view_h)
        .map(|(idx, c)| {
            let mut text = format!("  {:<7} {:<8} {}", c.action.word(), c.abbrev(), c.subject);
            if let Some(m) = &c.message {
                text.push_str("  → ");
                text.push_str(m);
            }
            if idx == t.selected {
                Line::from(Span::styled(text, theme::selected()))
            } else if c.action == crate::git::RebaseAction::Drop {
                Line::from(Span::styled(text, theme::dim()))
            } else {
                Line::from(text)
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), chunk(&chunks, 0));
    let hint = Line::from(Span::styled(
        t!("ui.git_rebase_hint").to_string(),
        theme::dim(),
    ));
    frame.render_widget(Paragraph::new(hint), chunk(&chunks, 1));
    app.layout.chooser = chunk(&chunks, 0);
}

/// Draw the read-only diff viewer overlay (about 80% of the screen).
pub(super) fn draw_diff_view(app: &mut App, frame: &mut Frame, area: Rect) {
    use crate::diff_view::Kind;
    let Some(d) = app.diff_view.as_ref() else {
        return;
    };
    let width = area.width.times(8).quot(10).clamp_fit(30, area.width);
    let height = area.height.times(8).quot(10).clamp_fit(8, area.height);
    let rect = Rect {
        x: area.x.plus(area.width.minus(width).quot(2)),
        y: area.y.plus(area.height.minus(height).quot(2)),
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
    let view_h = usize::from(chunk(&chunks, 0).height);
    let start = d.scroll.min(d.lines.len().minus(1));
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
                    spans.push(Span::styled(
                        chars
                            .get(i..s)
                            .unwrap_or_default()
                            .iter()
                            .collect::<String>(),
                        style,
                    ));
                }
                spans.push(Span::styled(
                    chars
                        .get(s.min(chars.len())..e.min(chars.len()))
                        .unwrap_or_default()
                        .iter()
                        .collect::<String>(),
                    emph,
                ));
                i = e;
            }
            if i < chars.len() {
                spans.push(Span::styled(
                    chars
                        .get(i..)
                        .unwrap_or_default()
                        .iter()
                        .collect::<String>(),
                    style,
                ));
            }
            Line::from(spans)
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), chunk(&chunks, 0));
    let hint = Line::from(Span::styled(
        t!("ui.diff_view_hint").to_string(),
        theme::dim(),
    ));
    frame.render_widget(Paragraph::new(hint), chunk(&chunks, 1));
}

/// Draw the project (workspace root) chooser.
pub(super) fn draw_workspace_chooser(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(c) = app.chooser.workspace() else {
        return;
    };
    let home = std::env::var("HOME").ok().filter(|h| !h.is_empty());
    let labels: Vec<String> = c
        .roots
        .iter()
        .map(|root| project_label(root, home.as_deref()))
        .collect();
    let hint = t!("ui.projects_hint");
    app.layout.chooser = draw_list_chooser_sized(
        frame,
        area,
        (&t!("ui.projects"), &hint),
        &labels,
        c.selected,
        72,
    );
}

/// A Switch Project row: the folder's own name first (what tells projects
/// apart, and what a narrow box would otherwise cut off), then where it is,
/// with `home` shown as `~`.
fn project_label(root: &str, home: Option<&str>) -> String {
    let trimmed = root.trim_end_matches(['/', '\\']);
    let name = trimmed.rsplit(['/', '\\']).next().unwrap_or(trimmed);
    let place = match home {
        Some(h) if root == h || root.starts_with(&format!("{h}/")) => {
            format!("~{}", root.get(h.len()..).unwrap_or_default())
        }
        _ => root.to_string(),
    };
    if name.is_empty() || name == place {
        place
    } else {
        format!("{name}   {place}")
    }
}

/// Draw the recorded-macro chooser; each row shows the macro name and its key count.
pub(super) fn draw_macro_chooser(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(c) = app.chooser.macros() else {
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

/// File → Reopen / Save with Encoding (T661): one row per encoding label.
pub(super) fn draw_encoding_chooser(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(c) = app.chooser.encoding() else {
        return;
    };
    let labels: Vec<String> = crate::encoding::CHOICES
        .iter()
        .map(ToString::to_string)
        .collect();
    let title = match c.action {
        crate::app::EncodingAction::Reopen => t!("ui.encoding_reopen"),
        crate::app::EncodingAction::Save => t!("ui.encoding_save"),
    };
    let hint = t!("ui.encoding_hint");
    app.layout.chooser = draw_list_chooser(frame, area, &title, &hint, &labels, c.selected);
}

/// Tools → HTTP Environment…: "none" plus one row per environment name.
pub(super) fn draw_http_env_chooser(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(c) = app.chooser.http_env() else {
        return;
    };
    let mut labels = vec![t!("ui.http_env_none").to_string()];
    labels.extend(c.names.iter().cloned());
    let hint = t!("ui.encoding_hint");
    app.layout.chooser =
        draw_list_chooser(frame, area, &t!("ui.http_env"), &hint, &labels, c.selected);
}

/// Draw the clipboard-history chooser; each entry is flattened to one line and
/// truncated to 80 characters.
pub(super) fn draw_clipboard_chooser(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(c) = app.chooser.clipboard() else {
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

/// Draw the task chooser; each row shows "name - command" so the action is
/// clear before it runs.
pub(super) fn draw_task_chooser(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(c) = app.chooser.task() else {
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

/// Draw the script-command chooser; each row shows the script-supplied label
/// plus the owning script's file stem (two scripts may register the same label).
#[cfg(feature = "script")]
pub(super) fn draw_script_chooser(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(c) = app.chooser.script() else {
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

/// Vix → Plugins… (T637): one row per installed plugin — name, translated
/// state, short commit.
#[cfg(feature = "script")]
pub(super) fn draw_plugin_chooser(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(c) = app.chooser.plugin() else {
        return;
    };
    let labels: Vec<String> = c
        .rows
        .iter()
        .map(|r| {
            format!(
                "{} — {} — {}",
                r.name,
                t!(r.state.i18n_key()),
                r.commit.get(..12).unwrap_or(&r.commit)
            )
        })
        .collect();
    let hint = t!("ui.plugins_hint");
    app.layout.chooser =
        draw_list_chooser(frame, area, &t!("ui.plugins"), &hint, &labels, c.selected);
}

/// Draw the Git status panel: one row per changed file, highlighting the
/// selected one, sized to fit (at most the screen height minus chrome).
pub(super) fn draw_git_panel(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(selected) = app.git_panel.as_ref().map(|p| p.selected) else {
        return;
    };
    let rows = app.git_status.len().max(1);
    let width = 64u16.min(area.width);
    let (hint, hint_rows) = super::hint_paragraph(&t!("ui.git_changes_hint"), width.minus(2));
    let max_rows = area.height.minus(hint_rows.plus(3)).max(1);
    let visible = u16::try_from(rows).unwrap_or(u16::MAX).min(max_rows);
    let height = visible.plus(3).plus(hint_rows).min(area.height);
    let rect = Rect {
        x: area.x.plus(area.width.minus(width).quot(2)),
        y: area.y.plus(area.height.minus(height).quot(3)),
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
        .constraints([Constraint::Min(1), Constraint::Length(hint_rows)])
        .split(inner);

    if app.git_status.is_empty() {
        let clean = Line::from(Span::styled(t!("ui.git_clean").to_string(), theme::dim()));
        frame.render_widget(Paragraph::new(clean), chunk(&chunks, 0));
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
        state.select(Some(selected.min(app.git_status.len().minus(1))));
        frame.render_stateful_widget(list, chunk(&chunks, 0), &mut state);
    }

    frame.render_widget(hint, chunk(&chunks, 1));

    app.layout.git_panel = Rect {
        x: chunk(&chunks, 0).x,
        y: chunk(&chunks, 0).y,
        width: chunk(&chunks, 0).width,
        height: u16::try_from(app.git_status.len())
            .unwrap_or(u16::MAX)
            .min(chunk(&chunks, 0).height),
    };
}

/// The T207 interactive commit-list panel (**Git → Log → Browse Log…** /
/// **File History**): abbrev hash, date, author, subject, one commit per
/// row. Wider than `draw_list_chooser`'s fixed 34 columns can fit, so this
/// mirrors `draw_outline`'s own manual scroll/`ensure_visible` layout
/// instead.
pub(super) fn draw_git_log(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some((n, scope)) = app
        .git_log
        .as_ref()
        .map(|p| (p.entries.len(), p.scope.clone()))
    else {
        return;
    };
    let width = 100u16.min(area.width);
    let max_rows = area.height.minus(4).max(1);
    let rows = u16::try_from(n).unwrap_or(u16::MAX).min(max_rows);
    let height = rows.plus(4).min(area.height);
    let rect = Rect {
        x: area.x.plus(area.width.minus(width).quot(2)),
        y: area.y.plus(area.height.minus(height).quot(3)),
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let title = match &scope {
        crate::git::LogScope::File(p) => {
            let name = std::path::Path::new(p)
                .file_name()
                .map_or_else(|| p.clone(), |n| n.to_string_lossy().into_owned());
            format!(" {} {} — {name} ", icon::BRANCH, t!("ui.git_file_history"))
        }
        crate::git::LogScope::Pick(rev) => format!(
            " {} {} ",
            icon::BRANCH,
            t!("ui.git_cherry_pick_from", rev = rev)
        ),
        _ => format!(" {} {} ", icon::BRANCH, t!("ui.git_log")),
    };
    let hint_text = if matches!(scope, crate::git::LogScope::Pick(_)) {
        t!("ui.git_cherry_pick_hint")
    } else {
        t!("ui.git_log_hint")
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

    let view_h = usize::from(chunk(&chunks, 0).height);
    if let Some(p) = app.git_log.as_mut() {
        p.ensure_visible(view_h);
    }
    let Some(p) = app.git_log.as_ref() else {
        return;
    };
    let mut lines: Vec<Line> = Vec::with_capacity(view_h);
    for idx in p.scroll..p.scroll.plus(view_h).min(p.entries.len()) {
        let Some(e) = p.entries.get(idx) else {
            break;
        };
        let text = format!(
            "  {:<8} {:<10} {:<20} {}",
            e.abbrev,
            e.date,
            trunc(&e.author, 20),
            e.subject
        );
        if idx == p.selected {
            lines.push(Line::from(Span::styled(text, theme::selected())));
        } else {
            lines.push(Line::from(text));
        }
    }
    frame.render_widget(Paragraph::new(lines), chunk(&chunks, 0));

    let hint = Line::from(Span::styled(hint_text.to_string(), theme::dim()));
    frame.render_widget(Paragraph::new(hint), chunk(&chunks, 1));

    app.layout.git_log = chunk(&chunks, 0);
}

/// Draw the right-click context menu from `CONTEXT_ITEMS`; `menu.separator`
/// entries become blank rows.
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
        .unwrap_or(u16::MAX))
    .plus(4)
    .min(area.width);
    let height = (u16::try_from(CONTEXT_ITEMS.len()).unwrap_or(u16::MAX))
        .plus(2)
        .min(area.height);
    // Clamp so the menu stays on screen near the click.
    let x = cm.x.min(area.right().minus(width));
    let y = cm.y.min(area.bottom().minus(height));
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
                Line::from(Span::styled(
                    "─".repeat(usize::from(inner.width)),
                    theme::dim(),
                ))
            } else {
                let text = format!(" {} ", labels.get(i).map_or("", String::as_str));
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

/// Draw the spelling-suggestion popup for the word under the cursor, sized to
/// its widest suggestion or title.
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
    let width = u16::try_from(widest)
        .unwrap_or(u16::MAX)
        .plus(6)
        .min(area.width);
    // Borders (2) + suggestion rows + hint (1).
    let height = u16::try_from(rows)
        .unwrap_or(u16::MAX)
        .plus(3)
        .min(area.height);
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
        frame.render_widget(Paragraph::new(none), chunk(&chunks, 0));
    } else {
        let items: Vec<ListItem> = p
            .suggestions
            .iter()
            .map(|s| ListItem::new(Line::from(format!("  {s}"))))
            .collect();
        let list = List::new(items).highlight_style(theme::selected());
        let mut state = ListState::default();
        state.select(Some(p.selected));
        frame.render_stateful_widget(list, chunk(&chunks, 0), &mut state);
    }

    let hint = Line::from(Span::styled(hint.to_string(), theme::dim()));
    frame.render_widget(Paragraph::new(hint), chunk(&chunks, 1));

    app.layout.spell_suggest = Rect {
        x: chunk(&chunks, 0).x,
        y: chunk(&chunks, 0).y,
        width: chunk(&chunks, 0).width,
        height: u16::try_from(p.suggestions.len())
            .unwrap_or(u16::MAX)
            .min(chunk(&chunks, 0).height),
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
    draw_list_chooser_sized(frame, area, (title, hint), labels, selected, 34)
}

/// [`draw_list_chooser`] at `width` columns, for rows that carry more than a
/// name (the T634 stash and worktree choosers).
fn draw_list_chooser_sized(
    frame: &mut Frame,
    area: Rect,
    (title, hint): (&str, &str),
    labels: &[String],
    selected: usize,
    width: u16,
) -> Rect {
    let width = width.min(area.width);
    let (hint, hint_rows) = super::hint_paragraph(hint, width.minus(2));
    let height = (u16::try_from(labels.len()).unwrap_or(u16::MAX))
        .plus(3)
        .plus(hint_rows)
        .min(area.height);
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
        .title(format!(" {} {} ", icon::PALETTE, title));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(hint_rows)])
        .split(inner);

    let items: Vec<ListItem> = labels
        .iter()
        .map(|label| ListItem::new(Line::from(format!("  {label}"))))
        .collect();
    let list = List::new(items).highlight_style(theme::selected());
    let mut state = ListState::default();
    state.select(Some(selected));
    frame.render_stateful_widget(list, chunk(&rows, 0), &mut state);

    frame.render_widget(hint, chunk(&rows, 1));
    chunk(&rows, 0)
}

/// The recent-files chooser: a multi-column table — basename, path, size,
/// created at, modified at — with the highlighted row opened by Enter or a
/// click. Records the data-row rectangle for mouse hit-testing.
pub(super) fn draw_recent_chooser(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(rc) = app.chooser.recent() else {
        return;
    };
    let width = 100u16.min(area.width);
    let rows_h = u16::try_from(rc.entries.len()).unwrap_or(u16::MAX);
    let height = rows_h.plus(5).min(area.height);
    let rect = Rect {
        x: area.x.plus(area.width.minus(width).quot(2)),
        y: area.y.plus(area.height.minus(height).quot(4)),
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
    let path_w = (usize::from(chunk(&chunks, 0).width))
        .minus((name_w + 9).plus(16).plus(16).plus(6))
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
        chunk(&chunks, 0),
    );

    let view_h = usize::from(chunk(&chunks, 1).height);
    let mut lines: Vec<Line> = Vec::with_capacity(view_h);
    for (row, e) in rc.entries.iter().enumerate().take(view_h) {
        let name = e
            .path
            .file_name()
            .map_or_default(|s| s.to_string_lossy().into_owned());
        let dir = e.path.parent().map_or_default(|d| d.display().to_string());
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
    frame.render_widget(Paragraph::new(lines), chunk(&chunks, 1));

    let hint = Line::from(Span::styled(t!("ui.recent_hint").to_string(), theme::dim()));
    frame.render_widget(Paragraph::new(hint), chunk(&chunks, 2));

    // Mouse hit-testing maps clicks to data rows only (the header is not a row).
    app.layout.chooser = chunk(&chunks, 1);
}

/// Draw the location chooser (references, definitions, ...): the file name and
/// line first so they survive truncation, then the directory.
pub(super) fn draw_location_chooser(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(lc) = app.chooser.location() else {
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
                .map_or_default(|s| s.to_string_lossy().into_owned());
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
    let Some(cc) = app.chooser.capture() else {
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
    let Some(c) = app.chooser.refile() else {
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

#[cfg(test)]
mod project_label_tests {
    use super::project_label;

    #[test]
    fn the_folder_name_comes_first_and_home_is_a_tilde() {
        assert_eq!(
            project_label("/home/me/git/vix", Some("/home/me")),
            "vix   ~/git/vix"
        );
        assert_eq!(project_label("/srv/app/", None), "app   /srv/app/");
        assert_eq!(
            project_label("/home/mel/x", Some("/home/me")),
            "x   /home/mel/x"
        );
        assert_eq!(project_label("/", None), "/");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{
        BranchChooser, ContextMenu, EncodingAction, EncodingChooser, GitPanel, MacroChooser,
        RecentChooser, RecentEntry, SpellSuggest, StashChooser, TaskChooser, WorkspaceChooser,
        WorktreeChooser,
    };
    use crate::settings::Settings;
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

    fn app() -> App {
        App::new(std::env::temp_dir(), Settings::default())
    }

    fn render(app: &mut App, w: u16, h: u16, f: fn(&mut App, &mut Frame, Rect)) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                f(app, frame, area);
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

    fn blank(buf: &Buffer) -> bool {
        text(buf).chars().all(|c| c == ' ' || c == '\n')
    }

    /// Every chooser draws nothing while it is not the open one.
    #[test]
    fn closed_choosers_draw_nothing() {
        let draws: [fn(&mut App, &mut Frame, Rect); 14] = [
            draw_branch_chooser,
            draw_stash_chooser,
            draw_worktree_chooser,
            draw_git_rebase,
            draw_diff_view,
            draw_workspace_chooser,
            draw_macro_chooser,
            draw_encoding_chooser,
            draw_http_env_chooser,
            draw_clipboard_chooser,
            draw_task_chooser,
            draw_git_panel,
            draw_git_log,
            draw_context_menu,
        ];
        for (i, f) in draws.iter().enumerate() {
            let mut a = app();
            assert!(
                blank(&render(&mut a, 100, 30, *f)),
                "chooser {i} drew while closed"
            );
        }
        let more: [fn(&mut App, &mut Frame, Rect); 6] = [
            draw_spell_suggest,
            draw_recent_chooser,
            draw_location_chooser,
            draw_capture_chooser,
            draw_refile_chooser,
            draw_script_chooser,
        ];
        for (i, f) in more.iter().enumerate() {
            let mut a = app();
            assert!(
                blank(&render(&mut a, 100, 30, *f)),
                "chooser {i} drew while closed"
            );
        }
        let mut a = app();
        assert!(blank(&render(&mut a, 100, 30, draw_plugin_chooser)));
    }

    #[test]
    fn the_branch_chooser_titles_follow_its_purpose() {
        use crate::app::chooser::ChooserSlot;
        let mut a = app();
        a.chooser.open_branch(BranchChooser {
            branches: vec!["main".to_string(), "feature/x".to_string()],
            selected: 1,
            merge: false,
            gutter: false,
        });
        let buf = render(&mut a, 100, 30, draw_branch_chooser);
        let t = text(&buf);
        assert!(t.contains(&t!("ui.branch").to_string()));
        assert!(t.contains("main") && t.contains("feature/x"));
        assert!(a.layout.chooser.height >= 2);
        a.chooser.open_branch(BranchChooser {
            branches: vec!["main".to_string()],
            selected: 0,
            merge: false,
            gutter: true,
        });
        let t = text(&render(&mut a, 100, 30, draw_branch_chooser));
        assert!(t.contains(&t!("ui.gutter_base").to_string()));
    }

    #[test]
    fn the_stash_and_worktree_choosers_describe_each_row() {
        use crate::app::chooser::ChooserSlot;
        let mut a = app();
        a.chooser.open_stash(StashChooser {
            entries: vec![crate::git::StashEntry {
                index: 0,
                message: "WIP on main".to_string(),
            }],
            selected: 0,
        });
        let t = text(&render(&mut a, 120, 30, draw_stash_chooser));
        assert!(t.contains("stash@{0}  WIP on main"));
        assert!(t.contains(&t!("ui.git_stash_list").to_string()));

        let wt = |path: &str, checkout, locked, prunable| crate::git::Worktree {
            path: std::path::PathBuf::from(path),
            head: Some("0123456789abcdef".to_string()),
            checkout,
            locked,
            prunable,
        };
        a.chooser.open_worktree(WorktreeChooser {
            worktrees: vec![
                wt(
                    "/w/main",
                    crate::git::Checkout::Branch("main".to_string()),
                    true,
                    false,
                ),
                wt("/w/detached", crate::git::Checkout::Detached, false, true),
                wt("/w/bare", crate::git::Checkout::Bare, false, false),
            ],
            selected: 0,
            remove: false,
        });
        let t = text(&render(&mut a, 120, 30, draw_worktree_chooser));
        assert!(t.contains("[main]"));
        assert!(t.contains(&format!("{} 0123456", t!("ui.git_worktree_detached"))));
        assert!(t.contains(&format!("[{}]", t!("ui.git_worktree_bare"))));
        assert!(t.contains(&t!("ui.git_worktree_locked").to_string()));
        assert!(t.contains(&t!("ui.git_worktree_missing").to_string()));
        assert!(t.contains(&t!("ui.git_worktrees").to_string()));
        // The remove flavor has its own title.
        a.chooser.worktree_mut().unwrap().remove = true;
        let t = text(&render(&mut a, 120, 30, draw_worktree_chooser));
        assert!(t.contains(&t!("ui.git_worktree_remove").to_string()));
    }

    #[test]
    fn the_rebase_todo_shows_actions_and_typed_reword_messages() {
        use crate::git::{RebaseAction, RebaseTodo, TodoCommit};
        let mut a = app();
        let mut todo = RebaseTodo::new(
            "base".to_string(),
            "main~3".to_string(),
            vec![
                TodoCommit::pick("1111111aaaa", "first"),
                TodoCommit::pick("2222222bbbb", "second"),
                TodoCommit::pick("3333333cccc", "third"),
            ],
        );
        todo.selected = 0;
        todo.commits[1].action = RebaseAction::Drop;
        todo.commits[2].message = Some("new words".to_string());
        a.git_rebase = Some(todo);
        let buf = render(&mut a, 120, 20, draw_git_rebase);
        let t = text(&buf);
        assert!(t.contains(&t!("ui.git_rebase", base = "main~3").to_string()));
        assert!(t.contains("1111111"));
        assert!(t.contains("second"));
        assert!(t.contains("→ new words"));
        assert!(t.contains(&t!("ui.git_rebase_hint").to_string()[..10]));
        assert!(a.layout.chooser.height >= 3, "one row per commit");
    }

    #[test]
    fn the_diff_view_prefixes_lines_and_emphasizes_changed_words() {
        use crate::diff_view::{Kind, Line as DLine};
        let mut a = app();
        a.diff_view = Some(crate::app::DiffViewState {
            title: "a.txt vs b.txt".to_string(),
            lines: vec![
                DLine {
                    kind: Kind::Context,
                    text: "same".to_string(),
                    emphasis: Vec::new(),
                },
                DLine {
                    kind: Kind::Del,
                    text: "let old = 1;".to_string(),
                    emphasis: vec![(4, 7)],
                },
                DLine {
                    kind: Kind::Add,
                    text: "let new = 1;".to_string(),
                    emphasis: vec![(4, 7)],
                },
                DLine {
                    kind: Kind::Sep,
                    text: "...".to_string(),
                    emphasis: Vec::new(),
                },
            ],
            scroll: 0,
        });
        let buf = render(&mut a, 100, 24, draw_diff_view);
        let r = rows(&buf);
        assert!(text(&buf).contains("a.txt vs b.txt"));
        assert!(r.iter().any(|l| l.contains("- let old = 1;")));
        assert!(r.iter().any(|l| l.contains("+ let new = 1;")));
        // The differing word is reversed + bold while the rest keeps its color.
        let (y, line) = r
            .iter()
            .enumerate()
            .find(|(_, l)| l.contains("+ let new"))
            .unwrap();
        let x = line.find("new").map(|b| line[..b].chars().count()).unwrap();
        let y = u16::try_from(y).unwrap();
        let word = &buf[(u16::try_from(x).unwrap(), y)];
        assert!(word.modifier.contains(Modifier::REVERSED));
        assert!(word.modifier.contains(Modifier::BOLD));
        assert_eq!(word.fg, Color::Green);
        let rest = &buf[(u16::try_from(x.saturating_sub(2)).unwrap(), y)];
        assert!(!rest.modifier.contains(Modifier::REVERSED));
        assert!(text(&buf).contains(&t!("ui.diff_view_hint").to_string()[..10]));
    }

    #[test]
    fn project_labels_lead_with_the_folder_name_and_abbreviate_home() {
        assert_eq!(
            project_label("/home/me/code/vix", Some("/home/me")),
            "vix   ~/code/vix"
        );
        assert_eq!(project_label("/home/me", Some("/home/me")), "me   ~");
        assert_eq!(project_label("/srv/app/", None), "app   /srv/app/");
        assert_eq!(project_label("/", None), "/");
        assert_eq!(
            project_label("/homework/x", Some("/home")),
            "x   /homework/x"
        );
    }

    #[test]
    fn simple_choosers_list_their_rows_under_their_titles() {
        use crate::app::chooser::ChooserSlot;
        let mut a = app();
        a.chooser.open_workspace(WorkspaceChooser {
            roots: vec!["/tmp/alpha".to_string(), "/tmp/beta".to_string()],
            selected: 1,
        });
        let t = text(&render(&mut a, 100, 30, draw_workspace_chooser));
        assert!(t.contains(&t!("ui.projects").to_string()));
        assert!(t.contains("alpha") && t.contains("beta"));

        a.chooser.open_macros(MacroChooser {
            macros: vec![crate::macros::Macro {
                name: "fmt".to_string(),
                keys: vec!["a".to_string(), "b".to_string(), "c".to_string()],
            }],
            selected: 0,
        });
        let t = text(&render(&mut a, 100, 30, draw_macro_chooser));
        assert!(t.contains("fmt (3 keys)"));

        a.chooser.open_encoding(EncodingChooser {
            action: EncodingAction::Reopen,
            selected: 0,
        });
        let t = text(&render(&mut a, 100, 30, draw_encoding_chooser));
        assert!(t.contains(&t!("ui.encoding_reopen").to_string()));
        assert!(t.contains(crate::encoding::CHOICES[0]));
        a.chooser.open_encoding(EncodingChooser {
            action: EncodingAction::Save,
            selected: 0,
        });
        let t = text(&render(&mut a, 100, 30, draw_encoding_chooser));
        assert!(t.contains(&t!("ui.encoding_save").to_string()));

        a.chooser.open_http_env(crate::app::HttpEnvChooser {
            names: vec!["staging".to_string()],
            selected: 0,
        });
        let t = text(&render(&mut a, 100, 30, draw_http_env_chooser));
        assert!(t.contains(&t!("ui.http_env_none").to_string()));
        assert!(t.contains("staging"));

        a.chooser.open_clipboard(crate::app::ClipboardChooser {
            entries: vec!["first\n\tline   two".to_string(), "x".repeat(200)],
            selected: 0,
        });
        let buf = render(&mut a, 120, 30, draw_clipboard_chooser);
        let t = text(&buf);
        assert!(t.contains("first line two"), "whitespace is flattened");
        assert!(
            !t.contains(&"x".repeat(81)),
            "long entries are cut at 80 characters"
        );
    }

    #[test]
    fn the_task_chooser_shows_name_and_command() {
        use crate::app::chooser::ChooserSlot;
        let mut a = app();
        a.chooser.open_task(TaskChooser {
            tasks: vec![crate::tasks::task::NamedTask {
                name: "build".to_string(),
                command: "cargo build".to_string(),
                source: crate::tasks::task::TaskSource::UserConfigured,
                matcher: None,
            }],
            selected: 0,
        });
        let t = text(&render(&mut a, 100, 30, draw_task_chooser));
        assert!(t.contains("build \u{2014} cargo build"));
        assert!(t.contains(&t!("ui.tasks").to_string()));
    }

    fn status(
        path: &str,
        staged: Option<crate::git::Change>,
        unstaged: Option<crate::git::Change>,
    ) -> crate::git::FileStatus {
        crate::git::FileStatus {
            path: path.to_string(),
            staged,
            unstaged,
        }
    }

    #[test]
    fn the_git_panel_marks_staged_files_and_reports_a_clean_tree() {
        use crate::git::Change;
        let mut a = app();
        a.git_panel = Some(GitPanel { selected: 0 });
        a.git_branch = Some("main".to_string());
        let t = text(&render(&mut a, 100, 30, draw_git_panel));
        assert!(t.contains(&t!("ui.git_clean").to_string()));
        assert!(t.contains("main"));
        a.git_status = vec![
            status("src/a.rs", Some(Change::Added), None),
            status("src/b.rs", None, Some(Change::Modified)),
            status("notes.txt", None, Some(Change::Untracked)),
        ];
        a.git_branch = None;
        let buf = render(&mut a, 100, 30, draw_git_panel);
        let r = rows(&buf);
        assert!(
            r.iter()
                .any(|l| l.contains("[\u{2713}]") && l.contains("src/a.rs"))
        );
        assert!(
            r.iter()
                .any(|l| l.contains("[ ]") && l.contains("src/b.rs"))
        );
        assert_eq!(a.layout.git_panel.height, 3);
    }

    #[test]
    fn the_git_log_titles_by_scope_and_marks_the_selection() {
        use crate::git::{LogEntry, LogPanel, LogScope};
        let entry = |n: usize| LogEntry {
            sha: format!("{n:040}"),
            abbrev: format!("abc{n:04}"),
            author: "A very long author name indeed".to_string(),
            date: "2026-11-23".to_string(),
            subject: format!("subject {n}"),
        };
        let mut a = app();
        a.git_log = Some(LogPanel::new(vec![entry(1), entry(2)], LogScope::Repo));
        let t = text(&render(&mut a, 120, 30, draw_git_log));
        assert!(t.contains(&t!("ui.git_log").to_string()));
        assert!(t.contains("abc0001") && t.contains("subject 2"));
        assert!(t.contains("2026-11-23"));
        assert!(
            !t.contains("A very long author name indeed"),
            "authors are cut to 20 columns"
        );
        a.git_log = Some(LogPanel::new(
            vec![entry(1)],
            LogScope::File("src/deep/mod.rs".to_string()),
        ));
        let t = text(&render(&mut a, 120, 30, draw_git_log));
        assert!(t.contains(&t!("ui.git_file_history").to_string()));
        assert!(t.contains("mod.rs"));
        a.git_log = Some(LogPanel::new(
            vec![entry(1)],
            LogScope::Pick("topic".to_string()),
        ));
        let t = text(&render(&mut a, 120, 30, draw_git_log));
        assert!(t.contains(&t!("ui.git_cherry_pick_from", rev = "topic").to_string()));
        assert!(t.contains(&t!("ui.git_cherry_pick_hint").to_string()[..10]));
    }

    #[test]
    fn the_context_menu_stays_on_screen_and_separators_are_rules() {
        use crate::app::CONTEXT_ITEMS;
        let mut a = app();
        a.context_menu = Some(ContextMenu {
            selected: 0,
            x: 95,
            y: 28,
        });
        let buf = render(&mut a, 100, 30, draw_context_menu);
        let r = a.layout.context_menu;
        assert!(
            r.right() <= 100 && r.bottom() <= 30,
            "clamped inside the screen: {r:?}"
        );
        assert_eq!(usize::from(r.height), CONTEXT_ITEMS.len() + 2);
        let t = text(&buf);
        let first = CONTEXT_ITEMS
            .iter()
            .find(|(_, act)| *act != "menu.separator")
            .unwrap();
        assert!(t.contains(&t!(first.0).to_string()));
        if CONTEXT_ITEMS
            .iter()
            .any(|(_, act)| *act == "menu.separator")
        {
            assert!(t.contains("\u{2500}\u{2500}\u{2500}"));
        }
    }

    #[test]
    fn the_spell_popup_lists_suggestions_or_says_there_are_none() {
        let mut a = app();
        a.spell_suggest = Some(SpellSuggest {
            word: "teh".to_string(),
            span: (0, 3),
            suggestions: vec!["the".to_string(), "tech".to_string()],
            selected: 1,
        });
        let t = text(&render(&mut a, 80, 24, draw_spell_suggest));
        assert!(t.contains(&t!("ui.spell_suggest", word = "teh").to_string()));
        assert!(t.contains("the") && t.contains("tech"));
        assert_eq!(a.layout.spell_suggest.height, 2);
        a.spell_suggest = Some(SpellSuggest {
            word: "zzz".to_string(),
            span: (0, 3),
            suggestions: Vec::new(),
            selected: 0,
        });
        let t = text(&render(&mut a, 80, 24, draw_spell_suggest));
        assert!(t.contains(&t!("ui.spell_no_suggestions").to_string()));
        assert_eq!(a.layout.spell_suggest.height, 0);
    }

    #[test]
    fn the_recent_files_table_shows_sizes_and_dates() {
        use crate::app::chooser::ChooserSlot;
        let mut a = app();
        a.chooser.open_recent(RecentChooser {
            entries: vec![
                RecentEntry {
                    path: std::path::PathBuf::from("/tmp/one.txt"),
                    size: 2048,
                    created: Some(1_700_000_000),
                    modified: Some(1_700_000_100),
                },
                RecentEntry {
                    path: std::path::PathBuf::from("/tmp/two.txt"),
                    size: 5,
                    created: None,
                    modified: None,
                },
            ],
            selected: 0,
        });
        let t = text(&render(&mut a, 140, 30, draw_recent_chooser));
        assert!(t.contains("one.txt") && t.contains("two.txt"));
        assert!(t.contains("/tmp"), "the directory is its own column");
        assert!(t.contains(&crate::file_browser_panel::size_label(2048)));
        assert!(
            a.layout.chooser.height >= 2,
            "both data rows are hit-testable"
        );
    }
}
