//! The DB workbench overlay (`spec/db`): connection list, add/edit form,
//! password/save/ask/params prompts, and the three-pane workbench itself
//! (schema tree, SQL editor with autocomplete, results grid).

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

use crate::geo::{Geo, chunk};
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
    let name = b.conn.as_ref().map_or_default(|c| format!(" — {}", c.name));
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
        draw_db_compare(app, frame, chunk(&chunks, 0));
    } else {
        draw_db_view(app, frame, view, chunk(&chunks, 0));
    }
    let Some(b) = app.db.as_ref() else { return };
    if let Some(msg) = &b.message {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(msg.clone(), theme::dim()))),
            chunk(&chunks, 1),
        );
    }
    let hint = match compare {
        Some(screen) => compare_hint(screen),
        None => view_hint(b, view),
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(hint, theme::dim()))),
        chunk(&chunks, 2),
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
        t!("ui.db_field_mcp_shared"),
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
        } else if i == crate::db::FORM_MCP_SHARED {
            on_off(b.form_mcp_sharing.is_shared()).to_string()
        } else if i == crate::db::FORM_ENVIRONMENT {
            crate::db::environment::badge(b.form_environment)
        } else {
            b.form.fields.get(i).cloned().unwrap_or_default()
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
    let col = u16::try_from(masked.chars().count().plus(1)).unwrap_or(u16::MAX);
    frame.set_cursor_position((area.x.plus(col.min(area.width.minus(1))), area.y.plus(1)));
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
    let height = usize::from(area.height).minus(1);
    let skip = b.list_sel.minus(height.minus(1));
    for (i, (name, sql)) in entries.iter().enumerate().skip(skip).take(height) {
        let one_line = sql.split_whitespace().collect::<Vec<_>>().join(" ");
        let text = if name.is_empty() {
            format!(" {}", trunc(&one_line, width.minus(2)))
        } else {
            format!(
                " {name}  {}",
                trunc(&one_line, width.minus(name.chars().count().plus(4)))
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
    let col = u16::try_from(b.save_name.chars().count().plus(1)).unwrap_or(u16::MAX);
    frame.set_cursor_position((area.x.plus(col.min(area.width.minus(1))), area.y.plus(1)));
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
    let col = u16::try_from(b.edit_input.chars().count().plus(1)).unwrap_or(u16::MAX);
    frame.set_cursor_position((area.x.plus(col.min(area.width.minus(1))), area.y.plus(1)));
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
        let col = u16::try_from(width.plus(3).plus(form.value.chars().count())).unwrap_or(u16::MAX);
        frame.set_cursor_position((area.x.plus(col.min(area.width.minus(1))), area.y.plus(3)));
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
    let col = u16::try_from(b.import_path.chars().count().plus(1)).unwrap_or(u16::MAX);
    frame.set_cursor_position((area.x.plus(col.min(area.width.minus(1))), area.y.plus(1)));
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
        done = p.values.len().plus(1),
        total = p.names.len()
    );
    let lines = vec![
        Line::from(Span::styled(format!(" {title}"), theme::title(true))),
        Line::from(format!(" {}", p.input)),
    ];
    frame.render_widget(Paragraph::new(lines), area);
    let col = u16::try_from(p.input.chars().count().plus(1)).unwrap_or(u16::MAX);
    frame.set_cursor_position((area.x.plus(col.min(area.width.minus(1))), area.y.plus(1)));
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
    let col = u16::try_from(b.ask_input.chars().count().plus(1)).unwrap_or(u16::MAX);
    frame.set_cursor_position((area.x.plus(col.min(area.width.minus(1))), area.y.plus(1)));
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
    let height = usize::from(area.height).minus(1);
    let skip = b.list_sel.minus(height.minus(1));
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
        .split(chunk(&cols, 1));
    let tree_body = Block::default()
        .borders(Borders::ALL)
        .inner(chunk(&cols, 0));
    let editor_body = Block::default()
        .borders(Borders::ALL)
        .inner(chunk(&right, 0));
    let results_area = Block::default()
        .borders(Borders::ALL)
        .inner(chunk(&right, 1));
    // The results grid keeps its first row for the headers.
    let results_body = Rect {
        y: results_area.y.plus(1),
        height: results_area.height.minus(1),
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
    draw_db_tree(app, frame, chunk(&cols, 0));
    draw_db_editor(app, frame, chunk(&right, 0));
    draw_db_results(app, frame, chunk(&right, 1));
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
        let text = format!("{}{arrow}{text}", " ".repeat(row.depth.times(2)));
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
        let row = b.query.row.minus(b.query.scroll);
        let col = b.query.col.min(usize::from(body.width.minus(1)));
        frame.set_cursor_position((
            body.x.plus(u16::try_from(col).unwrap_or(u16::MAX)),
            body.y.plus(
                u16::try_from(row)
                    .unwrap_or(u16::MAX)
                    .min(body.height.minus(1)),
            ),
        ));
        draw_db_popup(b, frame, body);
    }
}

/// The autocomplete popup, anchored under the editor cursor.
fn draw_db_popup(b: &crate::db::Browser, frame: &mut Frame, body: Rect) {
    let Some(popup) = b.popup.as_ref() else {
        return;
    };
    let width = (popup
        .items
        .iter()
        .map(|s| s.chars().count())
        .max()
        .unwrap_or(0))
    .plus(2);
    let width = u16::try_from(width).unwrap_or(u16::MAX).min(body.width);
    let height = u16::try_from(popup.items.len()).unwrap_or(u16::MAX).min(6);
    let row = u16::try_from(b.query.row.minus(b.query.scroll)).unwrap_or(u16::MAX);
    let col = u16::try_from(popup.start).unwrap_or(u16::MAX);
    let x = body.x.plus(col).min(body.right().minus(width));
    let y = if body.y.plus(row).plus(1).plus(height) <= body.bottom() {
        body.y.plus(row).plus(1)
    } else {
        body.y.plus(row).minus(height)
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
        .map_or_default(|clause| format!("WHERE {} ", trunc(clause, 40)));
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
            let name = b.grid.headers.get(c).map_or("", String::as_str);
            let text = format!("{marker}{name}");
            let w = widths.get(c).copied().unwrap_or(1);
            let style = if c == b.grid.cur_col && focused {
                theme::selected()
            } else {
                theme::title(false)
            };
            Span::styled(format!("{:w$} ", trunc(&text, w), w = w), style)
        })
        .collect();
    lines.push(Line::from(header));
    let height = usize::from(body.height).minus(1);
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
                    let text = staged.unwrap_or_else(|| {
                        b.grid
                            .rows
                            .get(ri)
                            .and_then(|r| r.get(c))
                            .map_or("", String::as_str)
                    });
                    let w = widths.get(c).copied().unwrap_or(1);
                    let cell = format!("{:w$} ", trunc(text, w), w = w);
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
                let text = b
                    .grid
                    .rows
                    .get(ri)
                    .and_then(|r| r.get(c))
                    .map_or("", String::as_str);
                let w = widths.get(c).copied().unwrap_or(1);
                let _ = write!(out, "{:w$} ", trunc(text, w), w = w);
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
    let width = usize::from(area.width).minus(2);
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
            cursor = Some((1, schema.chars().count().plus(1)));
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
            cursor = Some((1, masked.chars().count().plus(1)));
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
            cursor = Some((1, c.path.chars().count().plus(1)));
            lines.push(Line::from(format!(" {}", c.path)));
        }
        Screen::Script => {
            lines.push(title(t!("ui.db_compare_script").to_string()));
            let height = usize::from(area.height).minus(1);
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
            area.x.plus(col.min(area.width.minus(1))),
            area.y.plus(row.min(area.height.minus(1))),
        ));
    }
}

/// The schema compare's review screen: a summary line, the change list
/// with checkboxes and flags, and the highlighted change's definitions and
/// statements.
fn draw_db_compare_review(c: &crate::db::compare::review::Compare, frame: &mut Frame, area: Rect) {
    let width = usize::from(area.width).minus(2);
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
        chunk(&rows, 0),
    );
    if c.changes.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!(" {}", t!("ui.db_compare_same")),
                theme::dim(),
            ))),
            chunk(&rows, 1),
        );
        return;
    }
    let height = usize::from(chunk(&rows, 1).height);
    let skip = c.cursor.minus(height.minus(1));
    let mut list: Vec<Line> = Vec::with_capacity(height);
    for (i, change) in c.changes.iter().enumerate().skip(skip).take(height) {
        list.push(compare_change_line(c, i, change, width));
    }
    frame.render_widget(Paragraph::new(list), chunk(&rows, 1));
    if let Some(change) = c.changes.get(c.cursor) {
        draw_db_compare_detail(change, frame, chunk(&rows, 2));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Browser, Pane, Popup, View};
    use crate::settings::Settings;
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

    fn conn(name: &str, kind: crate::db::connect::Kind) -> crate::db::connect::Connection {
        crate::db::connect::Connection {
            name: name.to_string(),
            kind,
            host: "db.example.com".to_string(),
            database: "orders".to_string(),
            ..crate::db::connect::Connection::default()
        }
    }

    fn app_with(b: Browser) -> App {
        let mut app = App::new(std::env::temp_dir(), Settings::default());
        app.db = Some(b);
        app
    }

    fn draw(app: &mut App, w: u16, h: u16) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                draw_db(app, frame, area);
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

    /// The start of a hint line, which a narrow overlay may cut off.
    fn head(hint: &str) -> String {
        hint.chars().take(20).collect()
    }

    fn view(b: Browser, v: View) -> App {
        let mut app = app_with(b);
        app.db.as_mut().unwrap().view = v;
        app
    }

    #[test]
    fn nothing_is_drawn_without_a_browser() {
        let mut app = App::new(std::env::temp_dir(), Settings::default());
        let t = text(&draw(&mut app, 80, 20));
        assert!(t.chars().all(|c| c == ' ' || c == '\n'));
    }

    #[test]
    fn the_connections_list_marks_the_selection_and_shows_targets() {
        let mut b = Browser::new(vec![
            conn("prod", crate::db::connect::Kind::Postgres),
            conn("scratch", crate::db::connect::Kind::Sqlite),
        ]);
        b.sel = 1;
        b.message = Some("hello from the db".to_string());
        let mut app = app_with(b);
        let buf = draw(&mut app, 100, 12);
        let t = text(&buf);
        assert!(t.contains(&t!("ui.db_connections").to_string()));
        assert!(t.contains("prod") && t.contains("scratch"));
        assert!(t.contains("db.example.com"));
        assert!(t.contains("hello from the db"), "the status line shows the message");
        assert!(t.contains(&t!("ui.db_connections_hint").to_string()));
        assert!(t.contains(&t!("ui.db").to_string()));

        let mut empty = app_with(Browser::new(Vec::new()));
        let t = text(&draw(&mut empty, 100, 12));
        assert!(t.contains(&t!("ui.db_connections_empty").to_string()));
    }

    #[test]
    fn the_form_shows_every_field_and_the_held_password_note() {
        let mut b = Browser::new(Vec::new());
        b.view = View::Form;
        b.form.fields[0] = "mydb".to_string();
        b.form.writable = true;
        b.form.store_keyring = true;
        b.form.sel = 2;
        let mut app = app_with(b);
        let t = text(&draw(&mut app, 100, 30));
        assert!(t.contains(&t!("ui.db_form").to_string()));
        assert!(t.contains("mydb"));
        for key in ["ui.db_field_name", "ui.db_field_host", "ui.db_field_sslmode"] {
            assert!(t.contains(&t!(key).to_string()), "{key} is labelled");
        }
        assert!(t.contains(&t!("ui.db_access_write").to_string()));
        assert!(t.contains(&t!("ui.db_toggle_on").to_string()));
        assert!(t.contains(&head(&t!("ui.db_form_hint"))));
    }

    #[test]
    fn the_password_prompt_masks_what_was_typed() {
        let mut b = Browser::new(Vec::new());
        b.password = "hunter2".to_string();
        let mut app = view(b, View::Password);
        let t = text(&draw(&mut app, 80, 10));
        assert!(t.contains(&t!("ui.db_password").to_string()));
        assert!(t.contains(&"\u{2022}".repeat(7)));
        assert!(!t.contains("hunter2"));
    }

    #[test]
    fn history_and_saved_lists_show_entries_and_empty_states() {
        let mut b = Browser::new(Vec::new());
        let empty = text(&draw(&mut view(Browser::new(Vec::new()), View::History), 80, 8));
        assert!(empty.contains(&t!("ui.db_history_empty").to_string()));
        let empty = text(&draw(&mut view(Browser::new(Vec::new()), View::Saved), 80, 8));
        assert!(empty.contains(&t!("ui.db_saved_empty").to_string()));

        for i in 0..30 {
            b.history.entries.push(format!("SELECT {i}\n  FROM t"));
        }
        b.list_sel = 25;
        let mut app = view(b, View::History);
        let t = text(&draw(&mut app, 80, 10));
        assert!(t.contains(&t!("ui.db_history").to_string()));
        assert!(t.contains("SELECT 25 FROM t"), "multi-line SQL is shown on one line");
        assert!(!t.contains("SELECT 0 "), "the list scrolled to the selection");

        let mut b = Browser::new(Vec::new());
        b.saved.queries.push(crate::db::store::SavedQuery {
            name: "daily".to_string(),
            sql: "SELECT count(*)\nFROM orders".to_string(),
        });
        let mut app = view(b, View::Saved);
        let t = text(&draw(&mut app, 80, 8));
        assert!(t.contains(&t!("ui.db_saved_title").to_string()));
        assert!(t.contains("daily  SELECT count(*) FROM orders"));
    }

    #[test]
    fn the_small_prompts_show_what_was_typed() {
        let mut b = Browser::new(Vec::new());
        b.save_name = "my query".to_string();
        let t = text(&draw(&mut view(b, View::SaveName), 80, 8));
        assert!(t.contains(&t!("ui.db_save_name").to_string()));
        assert!(t.contains("my query"));

        let mut b = Browser::new(Vec::new());
        b.ask_input = "who bought most".to_string();
        let t = text(&draw(&mut view(b, View::Ask), 80, 8));
        assert!(t.contains(&t!("ui.db_ask_title").to_string()));
        assert!(t.contains("who bought most"));

        let mut b = Browser::new(Vec::new());
        b.edit_input = "new value".to_string();
        let t = text(&draw(&mut view(b, View::CellEdit), 80, 8));
        assert!(t.contains(&t!("ui.db_cell_edit_title").to_string()));
        assert!(t.contains("new value"));

        let mut b = Browser::new(Vec::new());
        b.import_path = "/tmp/data.csv".to_string();
        let t = text(&draw(&mut view(b, View::Import), 80, 8));
        assert!(t.contains(&t!("ui.db_import_title").to_string()));
        assert!(t.contains("/tmp/data.csv"));
    }

    #[test]
    fn the_confirm_screen_is_badged_with_the_environment() {
        let t = text(&draw(&mut view(Browser::new(Vec::new()), View::Confirm), 80, 8));
        assert!(t.contains(&t!("ui.db_confirm").to_string()));
        assert!(t.contains(&t!("ui.db_confirm_hint").to_string()));
    }

    #[test]
    fn the_cell_viewer_pretty_prints_json_on_request() {
        let mut b = Browser::new(Vec::new());
        b.cell_text = "{\"a\":1,\"b\":[2,3]}".to_string();
        let raw = text(&draw(&mut view(b, View::Cell), 80, 14));
        assert!(raw.contains("{\"a\":1,\"b\":[2,3]}"));

        let mut b = Browser::new(Vec::new());
        b.cell_text = "{\"a\":1,\"b\":[2,3]}".to_string();
        b.cell_pretty = true;
        let pretty = rows(&draw(&mut view(b, View::Cell), 80, 14));
        assert!(pretty.iter().any(|r| r.contains("\"a\": 1")));
        assert!(!pretty.iter().any(|r| r.contains("{\"a\":1")));
    }

    #[test]
    fn the_erd_viewer_scrolls_its_text() {
        let mut b = Browser::new(Vec::new());
        b.cell_text = (0..40).map(|i| format!("entity{i}")).collect::<Vec<_>>().join("\n");
        b.view_scroll = 10;
        let t = text(&draw(&mut view(b, View::Erd), 80, 10));
        assert!(t.contains(&t!("ui.db_erd_title").to_string()));
        assert!(t.contains("entity10"));
        assert!(!t.contains("entity9\n") && !t.contains("entity0 "));
    }

    #[test]
    fn the_query_log_shows_timing_rows_and_origin() {
        use crate::db::store::{LogEntry, Origin};
        let mut b = Browser::new(Vec::new());
        let t = text(&draw(&mut view(Browser::new(Vec::new()), View::Log), 100, 8));
        assert!(t.contains(&t!("ui.db_log_empty").to_string()));
        for (sql, ms, ok, origin) in [
            ("SELECT fast", 5, true, Origin::User),
            ("SELECT medium", 120, true, Origin::User),
            ("SELECT slow", 900, true, Origin::App),
            ("SELECT broken", 1, false, Origin::User),
        ] {
            b.log.push(LogEntry {
                sql: sql.to_string(),
                ms,
                rows: 3,
                ok,
                origin,
            });
        }
        b.list_sel = 0;
        let mut app = view(b, View::Log);
        let buf = draw(&mut app, 100, 10);
        let t = text(&buf);
        assert!(t.contains(&t!("ui.db_log_title").to_string()));
        for sql in ["SELECT fast", "SELECT medium", "SELECT slow", "SELECT broken"] {
            assert!(t.contains(sql));
        }
        assert!(t.contains(Origin::App.label()));
        // The failing statement and the slow one are red, the quick one green.
        let color_of = |needle: &str| {
            let rows = rows(&buf);
            let (y, line) = rows.iter().enumerate().find(|(_, l)| l.contains(needle)).unwrap();
            let x = line.find(needle).map(|b| line[..b].chars().count()).unwrap();
            let meta_x = u16::try_from(x.saturating_sub(10)).unwrap();
            buf[(meta_x, u16::try_from(y).unwrap())].fg
        };
        assert_eq!(color_of("SELECT slow"), Color::Red);
        assert_eq!(color_of("SELECT medium"), Color::Yellow);
        assert_eq!(color_of("SELECT fast"), Color::Green);
    }

    #[test]
    fn the_export_dialogs_show_formats_and_destination() {
        let mut b = Browser::new(Vec::new());
        b.export_path = "/tmp/out.csv".to_string();
        let t = text(&draw(&mut view(b, View::Export), 100, 8));
        assert!(t.contains(&t!("ui.db_export").to_string()));
        assert!(t.contains(&crate::db::export::FORMATS[0].label().to_string()));
        assert!(t.contains("/tmp/out.csv"));

        let mut b = Browser::new(Vec::new());
        b.export_clipboard = true;
        let t = text(&draw(&mut view(b, View::Export), 100, 8));
        assert!(t.contains(&t!("ui.db_export_clipboard").to_string()));

        let mut b = workbench();
        b.export_marks.insert(("main".to_string(), "users".to_string()));
        b.open_table_export();
        let dialog = b.table_export.as_ref().unwrap();
        assert!(!dialog.single_file(), "CSV exports into a directory");
        let dir = dialog.path.clone();
        let mut app = app_with(b);
        let t = text(&draw(&mut app, 120, 10));
        assert!(t.contains(&t!("ui.db_table_export").to_string()));
        assert!(t.contains("users"));
        assert!(t.contains(&t!("ui.db_table_export_dir").to_string()));
        assert!(t.contains(&dir));
        // The SQL format is one file instead.
        let sql = crate::db::table_export::TABLE_FORMATS.len() - 1;
        app.db.as_mut().unwrap().table_export.as_mut().unwrap().format = sql;
        let t = text(&draw(&mut app, 120, 10));
        assert!(!t.contains(&t!("ui.db_table_export_dir").to_string()));
    }

    #[test]
    fn the_parameter_prompt_shows_progress() {
        let mut b = Browser::new(Vec::new());
        let mut prompt = crate::db::ParamPrompt::default();
        prompt.names = vec!["id".to_string(), "name".to_string()];
        prompt.values = vec!["7".to_string()];
        prompt.input = "ada".to_string();
        b.params = Some(prompt);
        let t = text(&draw(&mut view(b, View::Params), 100, 8));
        assert!(t.contains(&t!("ui.db_params_title", name = "name", done = 2, total = 2).to_string()));
        assert!(t.contains("ada"));
        // With no prompt state the screen stays empty rather than panicking.
        let t = text(&draw(&mut view(Browser::new(Vec::new()), View::Params), 100, 8));
        assert!(!t.contains("ada"));
    }

    fn workbench() -> Browser {
        let mut b = Browser::new(vec![conn("prod", crate::db::connect::Kind::Postgres)]);
        b.conn = Some(conn("prod", crate::db::connect::Kind::Postgres));
        b.view = View::Workbench;
        b.tree = crate::db::catalog::Tree::from_objects(&[
            ("main".to_string(), "users".to_string(), "table".to_string()),
            ("main".to_string(), "orders".to_string(), "table".to_string()),
            ("main".to_string(), "active".to_string(), "view".to_string()),
        ]);
        b.query = crate::db::editor::Query::from_text("SELECT id, name\nFROM users\nWHERE id > 1;");
        b.grid.set(
            vec!["id".to_string(), "name".to_string()],
            vec![
                vec!["1".to_string(), "ada".to_string()],
                vec!["2".to_string(), "grace".to_string()],
            ],
        );
        b
    }

    #[test]
    fn the_workbench_draws_tree_editor_and_results_and_records_their_areas() {
        let mut app = app_with(workbench());
        let buf = draw(&mut app, 120, 32);
        let t = text(&buf);
        assert!(t.contains("prod"), "the connection name is in the title");
        assert!(t.contains(&t!("ui.db_tree").to_string()));
        assert!(t.contains("users") && t.contains("orders"));
        assert!(t.contains("SELECT id, name"));
        assert!(t.contains(&t!("ui.db_results").to_string()));
        assert!(t.contains("ada") && t.contains("grace"));
        assert!(t.contains(&t!("msg.db_rows", count = 2).to_string()));
        assert!(t.contains(&head(&t!("ui.db_hint"))));
        assert!(t.contains(&t!("ui.db_access_read").to_string()));
        let l = &app.layout;
        assert!(l.db_tree.width > 0 && l.db_editor.width > 0 && l.db_results.width > 0);
        assert!(l.db_tree.right() <= l.db_editor.x);
        assert!(l.db_editor.bottom() <= l.db_results.y);
    }

    #[test]
    fn the_tree_pane_marks_selection_exports_and_empty_state() {
        let mut b = workbench();
        b.focus = Pane::Tree;
        b.tree.sel = 2;
        b.export_marks.insert(("main".to_string(), "orders".to_string()));
        let mut app = app_with(b);
        let buf = draw(&mut app, 120, 32);
        let t = text(&buf);
        assert!(t.contains("* orders"), "a marked table shows an asterisk");
        assert!(t.contains(&t!("ui.db_tables").to_string()) || t.contains("users"));
        // The filter shows in the pane title.
        app.db.as_mut().unwrap().tree.filter = "ord".to_string();
        let t = text(&draw(&mut app, 120, 32));
        assert!(t.contains("/ord"));

        let mut b = workbench();
        b.tree = crate::db::catalog::Tree::default();
        let t = text(&draw(&mut app_with(b), 120, 32));
        assert!(t.contains(&t!("ui.db_tree_empty").to_string()));
    }

    #[test]
    fn the_editor_title_reports_write_mode_transactions_and_elapsed_time() {
        let mut b = workbench();
        b.focus = Pane::Editor;
        let mut app = app_with(b);
        let t = text(&draw(&mut app, 120, 32));
        assert!(t.contains(&t!("ui.db_editor").to_string()));
        assert!(!t.contains("TX"));
        // The autocomplete popup draws under the cursor while the editor is focused.
        app.db.as_mut().unwrap().popup = Some(Popup {
            items: vec!["users".to_string(), "user_roles".to_string()],
            sel: 1,
            start: 0,
        });
        let t = text(&draw(&mut app, 120, 32));
        assert!(t.contains("user_roles"));
        // An unfocused editor draws no popup.
        app.db.as_mut().unwrap().focus = Pane::Results;
        let t = text(&draw(&mut app, 120, 32));
        assert!(!t.contains("user_roles"));
    }

    #[test]
    fn the_results_grid_shows_sort_marks_filters_and_scrolls_columns() {
        let mut b = workbench();
        b.focus = Pane::Results;
        b.grid.sort = Some((0, true));
        let mut app = app_with(b);
        let t = text(&draw(&mut app, 120, 32));
        assert!(t.contains('\u{25b2}'));
        app.db.as_mut().unwrap().grid.sort = Some((1, false));
        let t = text(&draw(&mut app, 120, 32));
        assert!(t.contains('\u{25bc}'));
        // A client-side filter narrows the rows and shows in the title.
        {
            let g = &mut app.db.as_mut().unwrap().grid;
            g.filter = "gra".to_string();
        }
        let t = text(&draw(&mut app, 120, 32));
        assert!(t.contains("/gra"));
        assert!(t.contains("grace") && !t.contains("ada"));
        // Horizontal scroll hides the leading column.
        {
            let g = &mut app.db.as_mut().unwrap().grid;
            g.filter.clear();
            g.col_off = 1;
        }
        let buf = draw(&mut app, 120, 32);
        let r = rows(&buf);
        let title = r.iter().position(|l| l.contains(&t!("ui.db_results").to_string())).unwrap();
        let header = r.get(title + 1).unwrap();
        assert!(header.contains("name"));
        assert!(!header.contains("id"), "the first column scrolled out: {header:?}");
        // An empty grid says there are no results yet.
        let mut empty = workbench();
        empty.grid = crate::db::results::Grid::default();
        let t = text(&draw(&mut app_with(empty), 120, 32));
        assert!(t.contains(&t!("ui.db_results_empty").to_string()));
    }

    #[test]
    fn the_row_filter_form_previews_its_where_clause() {
        let mut b = workbench();
        b.row_filter.open = true;
        b.row_filter.field = 2;
        b.row_filter.column = 1;
        b.row_filter.value = "ada".to_string();
        let mut app = app_with(b);
        let t = text(&draw(&mut app, 100, 12));
        assert!(t.contains(&t!("ui.db_filter_title").to_string()));
        assert!(t.contains("name"));
        assert!(t.contains("ada"));
        assert!(t.contains(&t!("ui.db_filter_hint").to_string()));
    }

    #[test]
    fn a_long_message_and_hint_stay_on_their_own_lines() {
        let mut b = workbench();
        b.message = Some("a very informative status".to_string());
        let buf = draw(&mut app_with(b), 120, 32);
        let r = rows(&buf);
        let h = r.len();
        // Message sits on the line above the hint, inside the border.
        assert!(r[h - 3].contains("a very informative status"));
        assert!(r[h - 2].contains(&head(&t!("ui.db_hint"))));
    }

    #[test]
    fn staged_row_changes_are_colored_by_kind() {
        let mut b = workbench();
        b.focus = Pane::Results;
        b.grid.set(
            vec!["id".to_string(), "name".to_string()],
            vec![
                vec!["1".to_string(), "ada".to_string()],
                vec!["2".to_string(), "grace".to_string()],
                vec!["3".to_string(), "linus".to_string()],
            ],
        );
        crate::db::staging::set_cell(&mut b.edits, 0, 1, "ada", "dame");
        b.staged.toggle_delete(1);
        b.staged.insert_row(2);
        let mut app = app_with(b);
        let buf = draw(&mut app, 120, 32);
        let t = text(&buf);
        assert!(t.contains("dame"), "the staged value replaces the loaded one");
        let at = |needle: &str| {
            let r = rows(&buf);
            let (y, line) = r.iter().enumerate().find(|(_, l)| l.contains(needle)).unwrap();
            let x = line.find(needle).map(|b| line[..b].chars().count()).unwrap();
            let cell = &buf[(u16::try_from(x).unwrap(), u16::try_from(y).unwrap())];
            (cell.fg, cell.modifier)
        };
        let (fg, m) = at("dame");
        assert_eq!(fg, Color::Yellow);
        assert!(m.contains(Modifier::BOLD));
        let (fg, m) = at("grace");
        assert_eq!(fg, Color::Red);
        assert!(m.contains(Modifier::CROSSED_OUT));
        let (fg, m) = at("linus");
        assert_eq!(fg, Color::Green);
        assert!(m.contains(Modifier::BOLD));
    }
}
