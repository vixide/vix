//! Round-3 tests of the picker panels' drawing: closed panels draw nothing,
//! open ones show their rows in small and large areas (scrollbars appear
//! when the list is longer than the box), filters show, and groupings
//! change the Todo Tree's rows.

use ratatui::Frame;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use super::{
    draw_ascii_panel, draw_media_type_panel, draw_nerd_palette, draw_qrcode, draw_theme_editor,
    draw_todo_tree, draw_x11_panel,
};
use crate::app::App;
use crate::app::cov4::{app_at, temp};
use crate::app::info_panel::InfoSlot;

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

fn text(buf: &Buffer) -> String {
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn blank(buf: &Buffer) -> bool {
    text(buf).chars().all(|c| c == ' ' || c == '\n')
}

fn item(tag: &str, file: &str, line: usize, body: &str) -> vix_todo_tree::Item {
    vix_todo_tree::Item {
        tag: tag.into(),
        file: file.into(),
        line,
        col: 4,
        text: body.into(),
    }
}

#[test]
fn closed_picker_panels_draw_nothing() {
    let mut app = app_at(&temp("pp-draw-closed"));
    let draws: [fn(&mut App, &mut Frame, Rect); 7] = [
        draw_nerd_palette,
        draw_ascii_panel,
        draw_qrcode,
        draw_x11_panel,
        draw_theme_editor,
        draw_media_type_panel,
        draw_todo_tree,
    ];
    for draw in draws {
        assert!(blank(&render(&mut app, 60, 20, draw)));
    }
}

#[test]
fn the_x11_and_ascii_lists_scroll_with_a_bar_in_a_short_box() {
    let mut app = app_at(&temp("pp-draw-lists"));
    app.settings.viewport.show_scrollbar = true;
    app.run_action("tools.x11_colors");
    let tall = text(&render(&mut app, 80, 40, draw_x11_panel));
    assert!(tall.contains('#'), "{tall}");
    let short = render(&mut app, 80, 10, draw_x11_panel);
    assert!(!blank(&short));
    app.run_action("tools.ascii");
    let short = render(&mut app, 80, 10, draw_ascii_panel);
    assert!(!blank(&short));
    let tall = render(&mut app, 80, 40, draw_ascii_panel);
    assert!(!blank(&tall));
}

#[test]
fn the_nerd_palette_fits_narrow_terminals() {
    let mut app = app_at(&temp("pp-draw-nerd"));
    app.run_action("tools.nerd_palette");
    let wide = render(&mut app, 100, 30, draw_nerd_palette);
    assert!(!blank(&wide));
    assert!(app.layout.nerd_palette.width > 0);
    // A box narrower than the grid still draws (clipped), never panicking.
    let narrow = render(&mut app, 12, 6, draw_nerd_palette);
    assert!(!blank(&narrow));
    assert!(app.layout.nerd_palette.width <= 12);
}

#[test]
fn the_media_type_picker_shows_its_prompt_filter_and_scrollbar() {
    let mut app = app_at(&temp("pp-draw-media"));
    app.settings.viewport.show_scrollbar = true;
    app.run_action("tools.media_types");
    let idle = text(&render(&mut app, 80, 12, draw_media_type_panel));
    assert!(
        idle.contains(&t!("ui.media_types_filter").to_string()),
        "{idle}"
    );
    for c in "png".chars() {
        app.info.media_type_panel_mut().unwrap().push(c);
    }
    let filtered = text(&render(&mut app, 80, 12, draw_media_type_panel));
    assert!(filtered.contains("png"), "{filtered}");
    assert!(!filtered.contains(&t!("ui.media_types_filter").to_string()));
    assert!(filtered.contains("image/png"));
}

#[test]
fn the_todo_tree_lists_by_tag_or_file_and_scrolls() {
    let mut app = app_at(&temp("pp-draw-todo"));
    app.settings.viewport.show_scrollbar = true;
    let mut items = vec![
        item("TODO", "src/a.rs", 3, "TODO: first thing"),
        item("FIXME", "src/b.rs", 9, "FIXME: broken"),
    ];
    items.extend((0..40).map(|i| item("TODO", "src/c.rs", 20 + i, &format!("TODO: bulk {i}"))));
    app.todo_tree = Some(vix_todo_tree::Panel::new(items));
    let by_tag = text(&render(&mut app, 90, 14, draw_todo_tree));
    assert!(
        by_tag.contains(&t!("ui.todo_tree_by_tag").to_string()),
        "{by_tag}"
    );
    assert!(
        by_tag.contains("src/a.rs:3"),
        "tag grouping names the file: {by_tag}"
    );

    app.todo_tree.as_mut().unwrap().toggle_group_by();
    let by_file = text(&render(&mut app, 90, 14, draw_todo_tree));
    assert!(
        by_file.contains(&t!("ui.todo_tree_by_file").to_string()),
        "{by_file}"
    );
    assert!(by_file.contains("src/a.rs"));
    assert!(
        by_file.contains("    3  TODO: first thing"),
        "line numbers are right-aligned: {by_file}"
    );

    // A tall box shows more rows and no bar.
    let tall = text(&render(&mut app, 90, 60, draw_todo_tree));
    assert!(tall.contains("bulk 39"));
    assert!(!by_file.contains("bulk 39"));
}

#[test]
fn the_theme_editor_lists_slots_with_swatches() {
    let mut app = app_at(&temp("pp-draw-theme"));
    app.run_action("view.theme_edit");
    let shown = text(&render(&mut app, 80, 30, draw_theme_editor));
    assert!(shown.contains('#'), "slot colors are shown as hex: {shown}");
    let short = render(&mut app, 80, 8, draw_theme_editor);
    assert!(!blank(&short));
}

/// T845: while the file index is still building the palette says so in place
/// of the prefix hint, and the hint comes back once every file is in.
#[test]
fn the_file_finder_says_when_its_index_is_still_building() {
    let dir = temp("pp-draw-indexing");
    for i in 0..5_100 {
        let sub = dir.join(format!("d{}", i / 500));
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join(format!("f{i}.txt")), "").unwrap();
    }
    let mut app = app_at(&dir);
    app.run_action("tools.palette");
    assert!(app.file_index_busy());
    let building = text(&render(&mut app, 120, 30, crate::ui::search::draw_palette));
    assert!(building.contains("Indexing files"), "{building}");
    let start = std::time::Instant::now();
    while app.file_index_busy() {
        assert!(start.elapsed().as_secs() < 60, "index never landed");
        std::thread::sleep(std::time::Duration::from_millis(5));
        app.tick();
    }
    assert_eq!(app.file_index_len(), 5_100);
    let done = text(&render(&mut app, 120, 30, crate::ui::search::draw_palette));
    assert!(!done.contains("Indexing files"), "{done}");
    assert!(done.contains("prefixes"), "{done}");
}
