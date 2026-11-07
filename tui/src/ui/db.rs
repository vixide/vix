//! The DB workbench overlay (`spec/db`): connection list, add/edit form,
//! password/save/ask/params prompts, and the three-pane workbench itself
//! (schema tree, SQL editor with autocomplete, results grid).

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};

use super::trunc;
use crate::app::App;
use crate::theme::{self, icon};

/// Draw the DB overlay (`spec/db`): the connections list, the add/edit form,
/// the password prompt, or the tree/editor/results workbench.
pub(super) fn draw_db(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(b) = app.db.as_ref() else {
        return;
    };
    frame.render_widget(Clear, area);
    let name = b
        .conn
        .as_ref()
        .map(|c| format!(" — {}", c.name))
        .unwrap_or_default();
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} {}{} ", icon::DATABASE, t!("ui.db"), name));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);
    let view = b.view;
    let compare = b.compare().map(|c| c.screen);
    if compare.is_some() {
        draw_db_compare(app, frame, chunks[0]);
    } else {
        draw_db_view(app, frame, view, chunks[0]);
    }
    let Some(b) = app.db.as_ref() else { return };
    if let Some(msg) = &b.message {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(msg.clone(), theme::dim()))),
            chunks[1],
        );
    }
    let hint = match compare {
        Some(screen) => compare_hint(screen),
        None => view_hint(b, view),
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(hint, theme::dim()))),
        chunks[2],
    );
}

/// Draw the body of the current workbench `view`.
fn draw_db_view(app: &mut App, frame: &mut Frame, view: crate::db::View, area: Rect) {
    let (filter, tables) = app.db.as_ref().map_or((false, false), |b| {
        (b.row_filter_open(), b.table_export.is_some())
    });
    match view {
        crate::db::View::Connections => draw_db_connections(app, frame, area),
        crate::db::View::Form => draw_db_form(app, frame, area),
        crate::db::View::Password => draw_db_password(app, frame, area),
        crate::db::View::Workbench if filter => draw_db_row_filter(app, frame, area),
        crate::db::View::Workbench => draw_db_workbench(app, frame, area),
        crate::db::View::History | crate::db::View::Saved => {
            draw_db_query_list(app, frame, area);
        }
        crate::db::View::SaveName => draw_db_save_name(app, frame, area),
        crate::db::View::Confirm => draw_db_confirm(app, frame, area),
        crate::db::View::Cell => draw_db_cell(app, frame, area),
        crate::db::View::Export if tables => draw_db_table_export(app, frame, area),
        crate::db::View::Export => draw_db_export(app, frame, area),
        crate::db::View::Log => draw_db_log(app, frame, area),
        crate::db::View::Erd => draw_db_erd(app, frame, area),
        crate::db::View::Ask => draw_db_ask(app, frame, area),
        crate::db::View::Params => draw_db_params(app, frame, area),
        crate::db::View::Import => draw_db_import(app, frame, area),
        crate::db::View::CellEdit => draw_db_cell_edit(app, frame, area),
    }
}

/// The hint line for the current workbench `view`.
fn view_hint(b: &crate::db::Browser, view: crate::db::View) -> String {
    let tables = b.table_export.is_some();
    let sql_file = b.import_is_sql_file();
    let hint = match view {
        crate::db::View::Connections => t!("ui.db_connections_hint"),
        crate::db::View::Form => t!("ui.db_form_hint"),
        crate::db::View::Password => t!("ui.db_password_hint"),
        crate::db::View::Workbench if b.row_filter_open() => t!("ui.db_filter_hint"),
        crate::db::View::Workbench => t!("ui.db_hint"),
        crate::db::View::History | crate::db::View::Saved => t!("ui.db_list_hint"),
        crate::db::View::SaveName => t!("ui.db_save_hint"),
        crate::db::View::Confirm => t!("ui.db_confirm_hint"),
        crate::db::View::Cell => t!("ui.db_cell_hint"),
        crate::db::View::Export if tables => t!("ui.db_table_export_hint"),
        crate::db::View::Export => t!("ui.db_export_hint"),
        crate::db::View::Log => t!("ui.db_log_hint"),
        crate::db::View::Erd => t!("ui.db_erd_hint"),
        crate::db::View::Ask => t!("ui.db_ask_hint"),
        crate::db::View::Params => t!("ui.db_params_hint"),
        crate::db::View::Import if sql_file => t!("ui.db_run_file_hint"),
        crate::db::View::Import => t!("ui.db_import_hint"),
        crate::db::View::CellEdit => t!("ui.db_cell_edit_hint"),
    };
    hint.to_string()
}

/// The saved-connections list of the DB overlay.
fn draw_db_connections(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(b) = app.db.as_ref() else {
        return;
    };
    let mut lines: Vec<Line> = vec![Line::from(Span::styled(
        format!(" {}", t!("ui.db_connections")),
        theme::title(true),
    ))];
    if b.connections.is_empty() {
        lines.push(Line::from(Span::styled(
            format!(" {}", t!("ui.db_connections_empty")),
            theme::dim(),
        )));
    }
    for (i, conn) in b.connections.iter().enumerate() {
        let env = b.environment_of(&conn.name);
        let badge = crate::db::environment::badge(env);
        let text = format!(
            " {} {badge} {:10} {}",
            conn.name,
            conn.kind.label(),
            conn.target()
        );
        let line = if i == b.sel {
            Line::from(Span::styled(text, theme::selected()))
        } else {
            Line::from(vec![
                Span::raw(format!(" {} ", conn.name)),
                Span::styled(badge, crate::db::environment::style(env)),
                Span::styled(
                    format!(" {:10} {}", conn.kind.label(), conn.target()),
                    theme::dim(),
                ),
            ])
        };
        lines.push(line);
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// The add/edit connection form of the DB overlay.
fn draw_db_form(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(b) = app.db.as_ref() else {
        return;
    };
    let labels = [
        t!("ui.db_field_name"),
        t!("ui.db_field_kind"),
        t!("ui.db_field_file"),
        t!("ui.db_field_host"),
        t!("ui.db_field_port"),
        t!("ui.db_field_user"),
        t!("ui.db_field_database"),
        t!("ui.db_field_password_command"),
        t!("ui.db_field_ssh_host"),
        t!("ui.db_field_ssh_user"),
        t!("ui.db_field_ssh_port"),
        t!("ui.db_field_ssh_identity"),
        t!("ui.db_field_sslmode"),
        t!("ui.db_field_access"),
        t!("ui.db_field_store_keyring"),
        t!("ui.db_field_environment"),
    ];
    let on_off = |on: bool| {
        if on {
            t!("ui.db_toggle_on")
        } else {
            t!("ui.db_toggle_off")
        }
    };
    let mut lines: Vec<Line> = vec![Line::from(Span::styled(
        format!(" {}", t!("ui.db_form")),
        theme::title(true),
    ))];
    for (i, label) in labels.iter().enumerate() {
        let value = if i == crate::db::FORM_KIND {
            b.form.kind.label().to_string()
        } else if i == crate::db::FORM_WRITABLE {
            if b.form.writable {
                t!("ui.db_access_write").to_string()
            } else {
                t!("ui.db_access_read").to_string()
            }
        } else if i == crate::db::FORM_STORE {
            on_off(b.form.store_keyring).to_string()
        } else if i == crate::db::FORM_ENVIRONMENT {
            crate::db::environment::badge(b.form_environment)
        } else {
            b.form.fields[i].clone()
        };
        let value_style = if i == crate::db::FORM_ENVIRONMENT {
            crate::db::environment::style(b.form_environment)
        } else {
            Style::default()
        };
        let text = format!(" {label:<14} {value}");
        let line = if i == b.form.sel {
            Line::from(Span::styled(text, theme::selected()))
        } else {
            Line::from(vec![
                Span::styled(format!(" {label:<14} "), theme::dim()),
                Span::styled(value, value_style),
            ])
        };
        lines.push(line);
    }
    if b.form_password_held() {
        lines.push(Line::from(Span::styled(
            format!(" {}", t!("ui.db_form_password_held")),
            theme::dim(),
        )));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// The connect-time password prompt of the DB overlay (input is masked; the
/// password lives only in memory).
fn draw_db_password(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(b) = app.db.as_ref() else {
        return;
    };
    let masked = "\u{2022}".repeat(b.password.chars().count());
    let lines = vec![
        Line::from(Span::styled(
            format!(" {}", t!("ui.db_password")),
            theme::title(true),
        )),
        Line::from(format!(" {masked}")),
    ];
    frame.render_widget(Paragraph::new(lines), area);
    let col = u16::try_from(masked.chars().count() + 1).unwrap_or(u16::MAX);
    frame.set_cursor_position((area.x + col.min(area.width.saturating_sub(1)), area.y + 1));
}

/// The query-history / saved-queries list of the DB overlay.
fn draw_db_query_list(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(b) = app.db.as_ref() else {
        return;
    };
    let history = b.view == crate::db::View::History;
    let title = if history {
        t!("ui.db_history")
    } else {
        t!("ui.db_saved_title")
    };
    let mut lines: Vec<Line> = vec![Line::from(Span::styled(
        format!(" {title}"),
        theme::title(true),
    ))];
    let entries: Vec<(String, String)> = if history {
        b.history
            .entries
            .iter()
            .map(|e| (String::new(), e.clone()))
            .collect()
    } else {
        b.saved
            .queries
            .iter()
            .map(|q| (q.name.clone(), q.sql.clone()))
            .collect()
    };
    if entries.is_empty() {
        let empty = if history {
            t!("ui.db_history_empty")
        } else {
            t!("ui.db_saved_empty")
        };
        lines.push(Line::from(Span::styled(format!(" {empty}"), theme::dim())));
    }
    let width = usize::from(area.width);
    let height = usize::from(area.height).saturating_sub(1);
    let skip = b.list_sel.saturating_sub(height.saturating_sub(1));
    for (i, (name, sql)) in entries.iter().enumerate().skip(skip).take(height) {
        let one_line = sql.split_whitespace().collect::<Vec<_>>().join(" ");
        let text = if name.is_empty() {
            format!(" {}", trunc(&one_line, width.saturating_sub(2)))
        } else {
            format!(
                " {name}  {}",
                trunc(&one_line, width.saturating_sub(name.chars().count() + 4))
            )
        };
        if i == b.list_sel {
            lines.push(Line::from(Span::styled(text, theme::selected())));
        } else {
            lines.push(Line::from(Span::raw(text)));
        }
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// The save-query naming prompt of the DB overlay.
fn draw_db_save_name(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(b) = app.db.as_ref() else {
        return;
    };
    let lines = vec![
        Line::from(Span::styled(
            format!(" {}", t!("ui.db_save_name")),
            theme::title(true),
        )),
        Line::from(format!(" {}", b.save_name)),
    ];
    frame.render_widget(Paragraph::new(lines), area);
    let col = u16::try_from(b.save_name.chars().count() + 1).unwrap_or(u16::MAX);
    frame.set_cursor_position((area.x + col.min(area.width.saturating_sub(1)), area.y + 1));
}

/// The inline cell editor prompt (stages a new value for the selected cell),
/// with the column's allowed values listed under it when it has some.
fn draw_db_cell_edit(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(b) = app.db.as_ref() else {
        return;
    };
    let mut lines = vec![
        Line::from(Span::styled(
            format!(" {}", t!("ui.db_cell_edit_title")),
            theme::title(true),
        )),
        Line::from(format!(" {}", b.edit_input)),
    ];
    let choices = b.edit_choices();
    if !choices.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!(" {}", t!("ui.db_cell_edit_choices")),
            theme::dim(),
        )));
        for (i, choice) in choices.iter().enumerate() {
            let style = if b.edit_choice == Some(i) {
                theme::selected()
            } else {
                theme::base()
            };
            lines.push(Line::from(Span::styled(format!("   {choice} "), style)));
        }
    }
    frame.render_widget(Paragraph::new(lines), area);
    let col = u16::try_from(b.edit_input.chars().count() + 1).unwrap_or(u16::MAX);
    frame.set_cursor_position((area.x + col.min(area.width.saturating_sub(1)), area.y + 1));
}

/// The server-side row search form: column, operator and value rows, and the
/// `WHERE` condition they build (or why they do not yet).
fn draw_db_row_filter(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(b) = app.db.as_ref() else {
        return;
    };
    let form = &b.row_filter;
    let column = b.grid.headers.get(form.column).cloned().unwrap_or_default();
    let fields = [
        (
            t!("ui.db_filter_column"),
            format!("\u{2039} {column} \u{203a}"),
        ),
        (
            t!("ui.db_filter_operator"),
            format!("\u{2039} {} \u{203a}", form.op().label()),
        ),
        (t!("ui.db_filter_value"), form.value.clone()),
    ];
    let width = fields
        .iter()
        .map(|(label, _)| label.chars().count())
        .max()
        .unwrap_or(0);
    let mut lines = vec![Line::from(Span::styled(
        format!(" {}", t!("ui.db_filter_title")),
        theme::title(true),
    ))];
    for (i, (label, value)) in fields.iter().enumerate() {
        let style = if i == form.field {
            theme::selected()
        } else {
            theme::base()
        };
        lines.push(Line::from(vec![
            Span::styled(format!(" {label:<width$}  "), theme::dim()),
            Span::styled(format!("{value} "), style),
        ]));
    }
    lines.push(Line::from(""));
    let preview = match b.row_filter_clause() {
        Ok(clause) => Span::styled(format!(" WHERE {clause}"), theme::base()),
        Err(e) => Span::styled(format!(" {e}"), theme::dim()),
    };
    lines.push(Line::from(preview));
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
    if form.field == 2 {
        let col = u16::try_from(width + 3 + form.value.chars().count()).unwrap_or(u16::MAX);
        frame.set_cursor_position((area.x + col.min(area.width.saturating_sub(1)), area.y + 3));
    }
}

/// The file-path prompt: CSV/TSV import, or a `.sql` file to run (T786).
fn draw_db_import(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(b) = app.db.as_ref() else {
        return;
    };
    let title = if b.import_is_sql_file() {
        t!("ui.db_run_file_title")
    } else {
        t!("ui.db_import_title")
    };
    let lines = vec![
        Line::from(Span::styled(format!(" {title}"), theme::title(true))),
        Line::from(format!(" {}", b.import_path)),
    ];
    frame.render_widget(Paragraph::new(lines), area);
    let col = u16::try_from(b.import_path.chars().count() + 1).unwrap_or(u16::MAX);
    frame.set_cursor_position((area.x + col.min(area.width.saturating_sub(1)), area.y + 1));
}

/// The bind-parameter prompt: one value at a time, showing progress.
fn draw_db_params(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(b) = app.db.as_ref() else {
        return;
    };
    let Some(p) = b.params.as_ref() else {
        return;
    };
    let name = p.current().unwrap_or("");
    let title = t!(
        "ui.db_params_title",
        name = name,
        done = p.values.len() + 1,
        total = p.names.len()
    );
    let lines = vec![
        Line::from(Span::styled(format!(" {title}"), theme::title(true))),
        Line::from(format!(" {}", p.input)),
    ];
    frame.render_widget(Paragraph::new(lines), area);
    let col = u16::try_from(p.input.chars().count() + 1).unwrap_or(u16::MAX);
    frame.set_cursor_position((area.x + col.min(area.width.saturating_sub(1)), area.y + 1));
}

/// The natural-language "Ask AI" prompt of the DB overlay.
fn draw_db_ask(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(b) = app.db.as_ref() else {
        return;
    };
    let lines = vec![
        Line::from(Span::styled(
            format!(" {}", t!("ui.db_ask_title")),
            theme::title(true),
        )),
        Line::from(format!(" {}", b.ask_input)),
    ];
    frame.render_widget(Paragraph::new(lines), area);
    let col = u16::try_from(b.ask_input.chars().count() + 1).unwrap_or(u16::MAX);
    frame.set_cursor_position((area.x + col.min(area.width.saturating_sub(1)), area.y + 1));
}

/// The write/DDL confirmation of the DB overlay.
fn draw_db_confirm(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(b) = app.db.as_ref() else {
        return;
    };
    // Every pending statement in full (T781): on production in particular the
    // user approves exactly the SQL that will run.
    let env = b.active_environment();
    let mut lines = vec![Line::from(vec![
        Span::styled(format!(" {} ", t!("ui.db_confirm")), theme::title(true)),
        Span::styled(
            crate::db::environment::badge(env),
            crate::db::environment::style(env),
        ),
    ])];
    for stmt in b.pending_statements() {
        for text in stmt.lines() {
            lines.push(Line::from(Span::raw(format!(" {text}"))));
        }
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

/// The full-content cell viewer of the DB overlay.
fn draw_db_cell(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(b) = app.db.as_ref() else {
        return;
    };
    let content = if b.cell_pretty {
        crate::db::pretty_json(&b.cell_text).unwrap_or_else(|| b.cell_text.clone())
    } else {
        b.cell_text.clone()
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} ", t!("ui.db_cell")));
    let body = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(content)
            .wrap(Wrap { trim: false })
            .scroll((u16::try_from(b.view_scroll).unwrap_or(u16::MAX), 0)),
        body,
    );
}

/// The session query log: one line per execution, latency color-coded and the
/// statement origin tagged.
fn draw_db_log(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(b) = app.db.as_ref() else {
        return;
    };
    let mut lines: Vec<Line> = vec![Line::from(Span::styled(
        format!(" {}", t!("ui.db_log_title")),
        theme::title(true),
    ))];
    if b.log.entries.is_empty() {
        lines.push(Line::from(Span::styled(
            format!(" {}", t!("ui.db_log_empty")),
            theme::dim(),
        )));
    }
    let height = usize::from(area.height).saturating_sub(1);
    let skip = b.list_sel.saturating_sub(height.saturating_sub(1));
    for (i, e) in b.log.entries.iter().enumerate().skip(skip).take(height) {
        // Green fast, yellow moderate, red slow — errors always red.
        let timing = if !e.ok || e.ms >= 500 {
            Color::Red
        } else if e.ms >= 50 {
            Color::Yellow
        } else {
            Color::Green
        };
        let sql: String = e.sql.replace('\n', " ").chars().take(70).collect();
        let meta = format!(" {:>6}ms {:>5}r {:<4} ", e.ms, e.rows, e.origin.label());
        let line = if i == b.list_sel {
            Line::from(Span::styled(format!("{meta}{sql}"), theme::selected()))
        } else {
            Line::from(vec![
                Span::styled(meta, Style::default().fg(timing)),
                Span::styled(sql, theme::dim()),
            ])
        };
        lines.push(line);
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// The generated ER-diagram viewer (scrollable Mermaid text).
fn draw_db_erd(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(b) = app.db.as_ref() else {
        return;
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} ", t!("ui.db_erd_title")));
    let body = block.inner(area);
    frame.render_widget(block, area);
    let lines: Vec<Line> = b
        .cell_text
        .lines()
        .skip(b.view_scroll)
        .take(usize::from(body.height))
        .map(|l| Line::from(Span::raw(l.to_string())))
        .collect();
    frame.render_widget(Paragraph::new(lines), body);
}

/// The results-export dialog of the DB overlay.
fn draw_db_export(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(b) = app.db.as_ref() else {
        return;
    };
    let formats: Vec<Span> = crate::db::export::FORMATS
        .iter()
        .enumerate()
        .flat_map(|(i, f)| {
            let style = if i == b.export_format {
                theme::selected()
            } else {
                theme::dim()
            };
            [
                Span::styled(format!(" {} ", f.label()), style),
                Span::raw(" "),
            ]
        })
        .collect();
    let dest = if b.export_clipboard {
        t!("ui.db_export_clipboard").to_string()
    } else {
        format!("{}: {}", t!("ui.db_export_file"), b.export_path)
    };
    let lines = vec![
        Line::from(Span::styled(
            format!(" {}", t!("ui.db_export")),
            theme::title(true),
        )),
        Line::from(formats),
        Line::from(format!(" {dest}")),
    ];
    frame.render_widget(Paragraph::new(lines), area);
}

/// The table-export dialog (T784): the chosen tables, the format, and the
/// destination directory (or file, for SQL).
fn draw_db_table_export(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(b) = app.db.as_ref() else {
        return;
    };
    let Some(dialog) = b.table_export.as_ref() else {
        return;
    };
    let formats: Vec<Span> = crate::db::table_export::TABLE_FORMATS
        .iter()
        .enumerate()
        .flat_map(|(i, f)| {
            let style = if i == dialog.format {
                theme::selected()
            } else {
                theme::dim()
            };
            [
                Span::styled(format!(" {} ", f.label()), style),
                Span::raw(" "),
            ]
        })
        .collect();
    let names: Vec<&str> = dialog.tables.iter().map(|(_, t)| t.as_str()).collect();
    let dest = if dialog.single_file() {
        t!("ui.db_export_file")
    } else {
        t!("ui.db_table_export_dir")
    };
    let lines = vec![
        Line::from(Span::styled(
            format!(" {}", t!("ui.db_table_export")),
            theme::title(true),
        )),
        Line::from(Span::styled(
            format!(
                " {}",
                t!("ui.db_table_export_tables", names = names.join(", "))
            ),
            theme::dim(),
        )),
        Line::from(formats),
        Line::from(format!(" {dest}: {}", dialog.path)),
    ];
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

/// The three-pane DB workbench: schema tree, SQL editor, results grid.
fn draw_db_workbench(app: &mut App, frame: &mut Frame, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(32), Constraint::Min(20)])
        .split(area);
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(55), Constraint::Min(3)])
        .split(cols[1]);
    let tree_body = Block::default().borders(Borders::ALL).inner(cols[0]);
    let editor_body = Block::default().borders(Borders::ALL).inner(right[0]);
    let results_area = Block::default().borders(Borders::ALL).inner(right[1]);
    // The results grid keeps its first row for the headers.
    let results_body = Rect {
        y: results_area.y.saturating_add(1),
        height: results_area.height.saturating_sub(1),
        ..results_area
    };
    if let Some(b) = app.db.as_mut() {
        b.tree.ensure_visible(usize::from(tree_body.height));
        b.query.ensure_visible(usize::from(editor_body.height));
        b.grid.ensure_visible(usize::from(results_body.height));
    }
    app.layout.db_tree = tree_body;
    app.layout.db_editor = editor_body;
    app.layout.db_results = results_body;
    draw_db_tree(app, frame, cols[0]);
    draw_db_editor(app, frame, right[0]);
    draw_db_results(app, frame, right[1]);
}

/// The workbench's schema tree pane.
fn draw_db_tree(app: &App, frame: &mut Frame, area: Rect) {
    let Some(b) = app.db.as_ref() else {
        return;
    };
    let focused = b.focus == crate::db::Pane::Tree;
    let search = if b.tree.filter.is_empty() && !b.tree.filtering {
        String::new()
    } else {
        format!("/{} ", b.tree.filter)
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(focused))
        .title(format!(" {} {search}", t!("ui.db_tree")));
    let body = block.inner(area);
    frame.render_widget(block, area);
    let rows = b.tree.rows();
    let height = usize::from(body.height);
    let mut lines: Vec<Line> = Vec::with_capacity(height);
    for (i, row) in rows.iter().enumerate().skip(b.tree.scroll).take(height) {
        let arrow = if row.expandable {
            if row.expanded {
                "\u{25be} "
            } else {
                "\u{25b8} "
            }
        } else {
            "  "
        };
        let text = if let crate::db::catalog::RowRef::Folder(_, folder) = row.reference {
            t!(folder.label_key()).to_string()
        } else {
            row.text.clone()
        };
        // A table marked for export (`m`, T784) shows a `*` in the arrow slot.
        let arrow = match row.reference {
            crate::db::catalog::RowRef::Object(si, crate::db::catalog::Folder::Tables, _)
                if b.tree.schemas.get(si).is_some_and(|s| {
                    b.export_marks.contains(&(s.name.clone(), row.text.clone()))
                }) =>
            {
                "* "
            }
            _ => arrow,
        };
        let text = format!("{}{arrow}{text}", " ".repeat(row.depth * 2));
        let line = if i == b.tree.sel && focused {
            Line::from(Span::styled(text, theme::selected()))
        } else if row.depth < 2 {
            Line::from(Span::styled(text, theme::dim()))
        } else {
            Line::from(Span::raw(text))
        };
        lines.push(line);
    }
    if rows.is_empty() {
        lines.push(Line::from(Span::styled(
            t!("ui.db_tree_empty").to_string(),
            theme::dim(),
        )));
    }
    frame.render_widget(Paragraph::new(lines), body);
}

/// The workbench's SQL editor pane, with live highlighting and the
/// autocomplete popup.
fn draw_db_editor(app: &App, frame: &mut Frame, area: Rect) {
    let Some(b) = app.db.as_ref() else {
        return;
    };
    let focused = b.focus == crate::db::Pane::Editor;
    let badge = if b.ai_busy() {
        t!("ui.db_ai_thinking")
    } else if b.write_enabled() {
        t!("ui.db_access_write")
    } else {
        t!("ui.db_access_read")
    };
    let tx = match b.tx_state() {
        crate::db::TxState::Open => " · TX",
        crate::db::TxState::Aborted => " · TX!",
        crate::db::TxState::None => "",
    };
    let env = b.active_environment();
    let env_badge = Span::styled(
        crate::db::environment::badge(env),
        crate::db::environment::style(env),
    );
    let title = if let Some(secs) = b.query_elapsed_secs() {
        Line::from(vec![
            Span::raw(format!(" {} — ", t!("ui.db_editor"))),
            env_badge,
            Span::raw(format!(" {} ", t!("ui.db_query_running", secs = secs))),
        ])
    } else {
        Line::from(vec![
            Span::raw(format!(" {} — ", t!("ui.db_editor"))),
            env_badge,
            Span::raw(format!(" {badge}{tx} ")),
        ])
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(focused))
        .title(title);
    let body = block.inner(area);
    frame.render_widget(block, area);
    // Thread the block-comment state from the top of the buffer to the first
    // visible line so multi-line comments stay gray.
    let mut in_block = false;
    for line in b.query.lines().iter().take(b.query.scroll) {
        in_block = crate::db::highlight::highlight_line(line, in_block).1;
    }
    let height = usize::from(body.height);
    let mut lines: Vec<Line> = Vec::with_capacity(height);
    for line in b.query.lines().iter().skip(b.query.scroll).take(height) {
        let (spans, next) = crate::db::highlight::highlight_line(line, in_block);
        in_block = next;
        lines.push(Line::from(
            spans
                .into_iter()
                .map(|(tok, text)| Span::styled(text, crate::db::highlight::style(tok)))
                .collect::<Vec<_>>(),
        ));
    }
    frame.render_widget(Paragraph::new(lines), body);
    if focused {
        let row = b.query.row.saturating_sub(b.query.scroll);
        let col = b.query.col.min(usize::from(body.width.saturating_sub(1)));
        frame.set_cursor_position((
            body.x + u16::try_from(col).unwrap_or(u16::MAX),
            body.y
                + u16::try_from(row)
                    .unwrap_or(u16::MAX)
                    .min(body.height.saturating_sub(1)),
        ));
        draw_db_popup(b, frame, body);
    }
}

/// The autocomplete popup, anchored under the editor cursor.
fn draw_db_popup(b: &crate::db::Browser, frame: &mut Frame, body: Rect) {
    let Some(popup) = b.popup.as_ref() else {
        return;
    };
    let width = popup
        .items
        .iter()
        .map(|s| s.chars().count())
        .max()
        .unwrap_or(0)
        + 2;
    let width = u16::try_from(width).unwrap_or(u16::MAX).min(body.width);
    let height = u16::try_from(popup.items.len()).unwrap_or(u16::MAX).min(6);
    let row = u16::try_from(b.query.row.saturating_sub(b.query.scroll)).unwrap_or(u16::MAX);
    let col = u16::try_from(popup.start).unwrap_or(u16::MAX);
    let x = (body.x + col).min(body.right().saturating_sub(width));
    let y = if body.y + row + 1 + height <= body.bottom() {
        body.y + row + 1
    } else {
        (body.y + row).saturating_sub(height)
    };
    let rect = Rect {
        x,
        y,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let lines: Vec<Line> = popup
        .items
        .iter()
        .take(usize::from(height))
        .enumerate()
        .map(|(i, item)| {
            let text = format!(" {item} ");
            if i == popup.sel {
                Line::from(Span::styled(text, theme::selected()))
            } else {
                Line::from(Span::styled(text, theme::base()))
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(lines).style(theme::base()), rect);
}

/// The workbench's results pane: headers, filtered rows, and a row count.
fn draw_db_results(app: &App, frame: &mut Frame, area: Rect) {
    let Some(b) = app.db.as_ref() else {
        return;
    };
    let focused = b.focus == crate::db::Pane::Results;
    let filtered = b.grid.filtered();
    let filter_note = if b.grid.filter.is_empty() && !b.grid.filtering {
        String::new()
    } else {
        format!("/{} ", b.grid.filter)
    };
    let server_note = b
        .server_filter()
        .map(|clause| format!("WHERE {} ", trunc(clause, 40)))
        .unwrap_or_default();
    let title = format!(
        " {} \u{2014} {} {server_note}{filter_note}",
        t!("ui.db_results"),
        t!("msg.db_rows", count = filtered.len())
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(focused))
        .title(title);
    let body = block.inner(area);
    frame.render_widget(block, area);
    if b.grid.headers.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                t!("ui.db_results_empty").to_string(),
                theme::dim(),
            ))),
            body,
        );
        return;
    }
    // A plan tree (rows carrying marks) gets a wider first column so the
    // indented node titles stay readable.
    let plan_tree = b.plan_marks().is_some();
    let widths: Vec<usize> = b
        .grid
        .headers
        .iter()
        .enumerate()
        .map(|(c, h)| {
            b.grid
                .rows
                .iter()
                .map(|r| r.get(c).map_or(0, |s| s.chars().count()))
                .chain([h.chars().count()])
                .max()
                .unwrap_or(1)
                .clamp(1, if plan_tree && c == 0 { 60 } else { 24 })
        })
        .collect();
    let cols: Vec<usize> = (b.grid.col_off..b.grid.headers.len()).collect();
    let mut lines: Vec<Line> = Vec::new();
    let header: Vec<Span> = cols
        .iter()
        .map(|&c| {
            let marker = match b.grid.sort {
                Some((col, true)) if col == c => "\u{25b2}",
                Some((col, false)) if col == c => "\u{25bc}",
                _ => "",
            };
            let text = format!("{marker}{}", b.grid.headers[c]);
            let style = if c == b.grid.cur_col && focused {
                theme::selected()
            } else {
                theme::title(false)
            };
            Span::styled(
                format!("{:w$} ", trunc(&text, widths[c]), w = widths[c]),
                style,
            )
        })
        .collect();
    lines.push(Line::from(header));
    let height = usize::from(body.height).saturating_sub(1);
    lines.extend(db_result_rows(
        b, &cols, &widths, &filtered, focused, height,
    ));
    frame.render_widget(Paragraph::new(lines), body);
}

/// The data rows of the results grid. A row with staged changes is drawn
/// cell-by-cell and colored by kind: changed cells of a modified row bold
/// yellow, a new row bold green, a row staged for deletion red and struck
/// through. Other rows take the fast single-span path, colored by the row's
/// plan-tree mark (T776): the slowest nodes bold red, a far-off row estimate
/// yellow.
fn db_result_rows(
    b: &crate::db::Browser,
    cols: &[usize],
    widths: &[usize],
    filtered: &[usize],
    focused: bool,
    height: usize,
) -> Vec<Line<'static>> {
    use crate::db::staging::ChangeKind;
    use std::fmt::Write as _;
    let edited = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    let marks = b.plan_marks();
    let inserted = Style::default()
        .fg(Color::Green)
        .add_modifier(Modifier::BOLD);
    let deleted = Style::default()
        .fg(Color::Red)
        .add_modifier(Modifier::CROSSED_OUT);
    let mut lines = Vec::new();
    for (i, &ri) in filtered.iter().enumerate().skip(b.grid.scroll).take(height) {
        let selected = i == b.grid.sel && focused;
        if let Some(change) = b.row_change(ri) {
            let base = if selected {
                theme::selected()
            } else {
                Style::default()
            };
            let spans: Vec<Span> = cols
                .iter()
                .map(|&c| {
                    let staged = b.staged_value(ri, c);
                    let text =
                        staged.unwrap_or_else(|| b.grid.rows[ri].get(c).map_or("", String::as_str));
                    let cell = format!("{:w$} ", trunc(text, widths[c]), w = widths[c]);
                    let style = match change {
                        ChangeKind::Deleted => base.patch(deleted),
                        ChangeKind::Inserted => base.patch(inserted),
                        ChangeKind::Modified if staged.is_some() => edited,
                        ChangeKind::Modified => base,
                    };
                    Span::styled(cell, style)
                })
                .collect();
            lines.push(Line::from(spans));
        } else {
            let mut out = String::new();
            for &c in cols {
                let text = b.grid.rows[ri].get(c).map_or("", String::as_str);
                let _ = write!(out, "{:w$} ", trunc(text, widths[c]), w = widths[c]);
            }
            let mark = marks.and_then(|m| m.get(ri)).copied().unwrap_or_default();
            let line = if selected {
                Line::from(Span::styled(out, theme::selected()))
            } else if mark.hot {
                Line::from(Span::styled(
                    out,
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ))
            } else if mark.warn {
                Line::from(Span::styled(out, Style::default().fg(Color::Yellow)))
            } else {
                Line::from(Span::raw(out))
            };
            lines.push(line);
        }
    }
    lines
}

/// The hint line for a schema-compare screen (T785).
fn compare_hint(screen: crate::db::compare::review::Screen) -> String {
    use crate::db::compare::review::Screen;
    match screen {
        Screen::Pick => t!("ui.db_compare_pick_hint"),
        Screen::Password => t!("ui.db_password_hint"),
        Screen::Review => t!("ui.db_compare_review_hint"),
        Screen::Confirm => t!("ui.db_confirm_hint"),
        Screen::SavePath => t!("ui.db_compare_save_hint"),
        Screen::Script => t!("ui.db_compare_script_hint"),
        _ => t!("ui.db_compare_wait_hint"),
    }
    .to_string()
}

/// The schema compare (T785): target picker, password prompt, progress,
/// the review list with the highlighted change's details, the run
/// confirmation, the save prompt and the script viewer.
fn draw_db_compare(app: &mut App, frame: &mut Frame, area: Rect) {
    use crate::db::compare::review::Screen;
    let Some(b) = app.db.as_ref() else {
        return;
    };
    let Some(c) = b.compare() else {
        return;
    };
    let width = usize::from(area.width).saturating_sub(2);
    let title = |text: String| Line::from(Span::styled(format!(" {text}"), theme::title(true)));
    let mut lines: Vec<Line> = Vec::new();
    let mut cursor: Option<(usize, usize)> = None;
    match c.screen {
        Screen::Pick => {
            lines.push(title(format!(
                "{} — {}",
                t!("ui.db_compare_title"),
                t!("ui.db_compare_pick")
            )));
            let schema = t!("ui.db_compare_schema", schema = c.schema).to_string();
            cursor = Some((1, schema.chars().count() + 1));
            lines.push(Line::from(format!(" {schema}")));
            for (i, idx) in c.candidates.iter().enumerate() {
                let Some(conn) = b.connections.get(*idx) else {
                    continue;
                };
                let text = format!(" {} {:10} {}", conn.name, conn.kind.label(), conn.target());
                let style = if i == c.pick {
                    theme::selected()
                } else {
                    theme::base()
                };
                lines.push(Line::from(Span::styled(trunc(&text, width), style)));
            }
        }
        Screen::Password => {
            lines.push(title(
                t!("ui.db_compare_password", name = c.target_name()).to_string(),
            ));
            let masked = "\u{2022}".repeat(c.password.chars().count());
            cursor = Some((1, masked.chars().count() + 1));
            lines.push(Line::from(format!(" {masked}")));
        }
        Screen::Working => {
            let secs = c.elapsed_secs().unwrap_or(0);
            lines.push(title(if c.running_migration() {
                t!("ui.db_compare_running", name = c.target_name(), secs = secs).to_string()
            } else {
                t!("ui.db_compare_reading", secs = secs).to_string()
            }));
        }
        Screen::Review => {
            draw_db_compare_review(c, frame, area);
            return;
        }
        Screen::Confirm => {
            let count = c.plan().runnable().len();
            lines.push(title(
                t!(
                    "ui.db_compare_confirm",
                    count = count,
                    name = c.target_name()
                )
                .to_string(),
            ));
            if let Some(env) = c.target_environment() {
                lines.push(Line::from(format!(
                    " {}",
                    t!("ui.db_compare_confirm_env", env = env)
                )));
            }
            lines.push(Line::from(Span::styled(
                format!(" {}", t!("ui.db_compare_confirm_note")),
                theme::dim(),
            )));
        }
        Screen::SavePath => {
            lines.push(title(t!("ui.db_compare_save").to_string()));
            cursor = Some((1, c.path.chars().count() + 1));
            lines.push(Line::from(format!(" {}", c.path)));
        }
        Screen::Script => {
            lines.push(title(t!("ui.db_compare_script").to_string()));
            let height = usize::from(area.height).saturating_sub(1);
            for l in c.script().lines().skip(c.script_scroll).take(height) {
                lines.push(Line::from(format!(" {l}")));
            }
        }
        _ => {}
    }
    frame.render_widget(Paragraph::new(lines), area);
    if let Some((row, col)) = cursor {
        let col = u16::try_from(col).unwrap_or(u16::MAX);
        let row = u16::try_from(row).unwrap_or(u16::MAX);
        frame.set_cursor_position((
            area.x + col.min(area.width.saturating_sub(1)),
            area.y + row.min(area.height.saturating_sub(1)),
        ));
    }
}

/// The schema compare's review screen: a summary line, the change list
/// with checkboxes and flags, and the highlighted change's definitions and
/// statements.
fn draw_db_compare_review(c: &crate::db::compare::review::Compare, frame: &mut Frame, area: Rect) {
    let width = usize::from(area.width).saturating_sub(2);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Percentage(45),
            Constraint::Min(3),
        ])
        .split(area);
    let chosen = c.checked.iter().filter(|on| **on).count();
    let access = if c.target_write_enabled() {
        t!("ui.db_access_write")
    } else {
        t!("ui.db_access_read")
    };
    let summary = t!(
        "ui.db_compare_summary",
        source = c.source_name,
        target = c.target_name(),
        schema = c.schema,
        chosen = chosen,
        total = c.changes.len()
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            trunc(&format!(" {summary} · {access}"), width),
            theme::title(true),
        ))),
        rows[0],
    );
    if c.changes.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!(" {}", t!("ui.db_compare_same")),
                theme::dim(),
            ))),
            rows[1],
        );
        return;
    }
    let height = usize::from(rows[1].height);
    let skip = c.cursor.saturating_sub(height.saturating_sub(1));
    let mut list: Vec<Line> = Vec::with_capacity(height);
    for (i, change) in c.changes.iter().enumerate().skip(skip).take(height) {
        list.push(compare_change_line(c, i, change, width));
    }
    frame.render_widget(Paragraph::new(list), rows[1]);
    if let Some(change) = c.changes.get(c.cursor) {
        draw_db_compare_detail(change, frame, rows[2]);
    }
}

/// One row of the schema compare's change list: checkbox, action marker,
/// object kind, name and flags.
fn compare_change_line(
    c: &crate::db::compare::review::Compare,
    i: usize,
    change: &crate::db::compare::diff::Change,
    width: usize,
) -> Line<'static> {
    let mark = if c.checked.get(i).copied().unwrap_or(false) {
        "[x]"
    } else {
        "[ ]"
    };
    let mut flags = Vec::new();
    if change.destructive {
        flags.push(t!("ui.db_compare_destructive").to_string());
    }
    if change.risky {
        flags.push(t!("ui.db_compare_risky").to_string());
    }
    if change.is_manual() {
        flags.push(t!("ui.db_compare_manual").to_string());
    }
    let flags = if flags.is_empty() {
        String::new()
    } else {
        format!("  ! {}", flags.join(", "))
    };
    let text = format!(
        " {mark} {} {} {}{flags}",
        change.action.marker(),
        t!(change.object.label_key()),
        change.label()
    );
    // Monochrome like the built-in themes: the flags carry the severity.
    let style = if i == c.cursor {
        theme::selected()
    } else {
        theme::base()
    };
    Line::from(Span::styled(trunc(&text, width), style))
}

/// The highlighted change's source and target definitions and statements.
fn draw_db_compare_detail(
    change: &crate::db::compare::diff::Change,
    frame: &mut Frame,
    area: Rect,
) {
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(theme::dim())
        .title(format!(" {} ", change.label()));
    let body = block.inner(area);
    frame.render_widget(block, area);
    let mut detail: Vec<Line> = Vec::new();
    for (label, text) in [
        (t!("ui.db_compare_source"), &change.source),
        (t!("ui.db_compare_target"), &change.target),
    ] {
        detail.push(Line::from(Span::styled(format!(" {label}:"), theme::dim())));
        for l in text.lines() {
            detail.push(Line::from(format!("   {l}")));
        }
    }
    detail.push(Line::from(Span::styled(
        format!(" {}:", t!("ui.db_compare_sql")),
        theme::dim(),
    )));
    for s in &change.statements {
        for l in s.sql.lines() {
            detail.push(Line::from(format!("   {l}")));
        }
    }
    frame.render_widget(Paragraph::new(detail).wrap(Wrap { trim: false }), body);
}
