//! The OC panel overlay (T702-T704): a tab line naming the lists, the rows of
//! the current list, and a footer that is the key hint, the exact command
//! awaiting a yes, or a text prompt.

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
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use vix_oci::argv::Kind;
use vix_oci_panel::{Mode, Panel, Purpose};

use super::{draw_scrollbar, trunc};
use crate::app::App;
use crate::theme;

/// The translated name of a list.
fn kind_name(kind: Kind) -> String {
    match kind {
        Kind::Container => t!("ui.oc_containers"),
        Kind::Image => t!("ui.oc_images"),
        Kind::Volume => t!("ui.oc_volumes"),
        Kind::Network => t!("ui.oc_networks"),
        _ => t!("ui.oc_pods"),
    }
    .to_string()
}

/// Which list a prune command (`container prune -f`, ...) clears.
fn prune_kind(args: &[String]) -> Option<Kind> {
    if args.get(1).map(String::as_str) != Some("prune") {
        return None;
    }
    match args.first().map(String::as_str) {
        Some("container") => Some(Kind::Container),
        Some("image") => Some(Kind::Image),
        Some("volume") => Some(Kind::Volume),
        Some("network") => Some(Kind::Network),
        _ => None,
    }
}

/// The bottom line.
fn footer(p: &Panel) -> String {
    match &p.mode {
        Mode::Browse => match p.kind {
            Kind::Container => t!("ui.oc_hint_containers"),
            Kind::Image => t!("ui.oc_hint_images"),
            _ => t!("ui.oc_hint_other"),
        }
        .to_string(),
        Mode::Confirm(pending) => {
            let note = prune_kind(&pending.args)
                .map_or_default(|k| format!("{} ", t!("ui.oc_prune_note", kind = kind_name(k))));
            t!(
                "ui.oc_confirm",
                note = note,
                command = pending.rendered.as_str()
            )
            .to_string()
        }
        Mode::Input {
            purpose,
            text,
            subject,
        } => match purpose {
            Purpose::Run => t!("ui.oc_input_run", text = text),
            Purpose::Tag => t!("ui.oc_input_tag", image = subject, text = text),
            Purpose::Pull => t!("ui.oc_input_pull", text = text),
            Purpose::Build => t!("ui.oc_input_build", text = text),
            _ => t!("ui.oc_input_prune", text = text),
        }
        .to_string(),
        _ => String::new(),
    }
}

/// Draw the OC panel overlay: a tab line naming the lists, the rows of the
/// current list and a footer (key hint, the exact command awaiting a yes, or a
/// text prompt). No-op when the panel is closed.
pub(super) fn draw_oc(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.oc.is_none() {
        return;
    }
    let width = 120u16.min(area.width);
    let height = area.height.minus(2).max(6).min(area.height);
    let rect = Rect {
        x: area.x.plus(area.width.minus(width).quot(2)),
        y: area.y.plus(area.height.minus(height).quot(2)),
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let engine = app.oc.as_ref().map_or("", |p| p.engine.binary());
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} ({engine}) ", t!("ui.oc")));
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

    let view_h = chunk(&chunks, 1).height as usize;
    if let Some(p) = app.oc.as_mut() {
        p.ensure_visible(view_h);
    }
    let Some(p) = app.oc.as_ref() else {
        return;
    };

    let tabs: Vec<Span> = p
        .kinds()
        .into_iter()
        .flat_map(|k| {
            let name = format!(" {} ", kind_name(k));
            let style = if k == p.kind {
                theme::selected()
            } else {
                theme::dim()
            };
            [Span::styled(name, style), Span::raw(" ")]
        })
        .collect();
    frame.render_widget(Paragraph::new(Line::from(tabs)), chunk(&chunks, 0));

    let total = p.len();
    let avail = chunk(&chunks, 1).width as usize;
    let mut lines: Vec<Line> = Vec::with_capacity(view_h);
    if total == 0 {
        lines.push(Line::from(Span::styled(
            t!("ui.oc_empty").to_string(),
            theme::dim(),
        )));
    }
    for i in p.scroll..total.min(p.scroll.plus(view_h)) {
        let text = trunc(&p.row(i), avail.minus(1));
        let style = if i == p.selected {
            theme::selected()
        } else {
            Style::default()
        };
        lines.push(Line::from(Span::styled(text, style)));
    }
    let show_bar = total > view_h && chunk(&chunks, 1).width > 1;
    let row_area = if show_bar {
        Rect {
            width: chunk(&chunks, 1).width.minus(1),
            ..chunks[1]
        }
    } else {
        chunk(&chunks, 1)
    };
    frame.render_widget(Paragraph::new(lines), row_area);
    if show_bar {
        let sb_area = Rect {
            x: chunk(&chunks, 1).x.plus(chunk(&chunks, 1).width).minus(1),
            ..chunks[1]
        };
        draw_scrollbar(frame, sb_area, p.selected, total.minus(1));
    }
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(footer(p), theme::dim()))),
        chunk(&chunks, 2),
    );
    app.layout.oc = Rect {
        height: u16::try_from(view_h).unwrap_or(u16::MAX),
        ..row_area
    };
}
