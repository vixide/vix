//! Round-4 behavior tests for the small tool panels (test-only): what each
//! one draws for its phases and results, and that a closed tool draws
//! nothing.

use super::*;
use crate::app::cov4::{app_at, temp};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

/// Draw one tool panel into a fresh `w` x `h` buffer.
fn render(app: &mut App, w: u16, h: u16, f: impl FnOnce(&mut App, &mut Frame, Rect)) -> Buffer {
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

#[test]
fn closed_tools_draw_nothing() {
    let mut a = app_at(&temp("r4-tp-closed"));
    a.welcome = None;
    a.dialog = None;
    assert!(blank(&render(&mut a, 80, 24, draw_pomodoro)));
    assert!(blank(&render(&mut a, 80, 24, draw_welcome)));
    assert!(blank(&render(&mut a, 80, 24, draw_dialog)));
    assert!(blank(&render(&mut a, 80, 24, draw_color_converter)));
    assert!(blank(&render(&mut a, 80, 24, draw_regex_tester)));
    assert!(blank(&render(&mut a, 80, 24, draw_calculator)));
    assert!(blank(&render(&mut a, 80, 24, draw_unit_converter)));
}

#[test]
fn the_pomodoro_button_and_hint_follow_the_phase() {
    let mut a = app_at(&temp("r4-tp-pomo"));
    a.run_action("tools.pomodoro");
    let idle = text(&render(&mut a, 80, 24, draw_pomodoro));
    assert!(
        idle.contains(&format!("[ {} ]", t!("ui.pomodoro_start"))),
        "{idle}"
    );
    assert!(
        idle.contains(&t!("menu.item.tools.pomodoro").to_string()),
        "work title: {idle}"
    );
    assert!(
        a.layout.pomodoro_button.width > 0,
        "the button rect is recorded"
    );

    a.pomodoro.as_mut().unwrap().start();
    let work = text(&render(&mut a, 80, 24, draw_pomodoro));
    assert!(
        work.contains(&format!("[ {} ]", t!("ui.pomodoro_stop"))),
        "{work}"
    );
    assert!(
        work.contains("25:00"),
        "the full work length is counted down: {work}"
    );

    a.pomodoro.as_mut().unwrap().tick(25 * 60);
    assert!(a.pomodoro.as_ref().unwrap().is_break());
    let rest = text(&render(&mut a, 80, 24, draw_pomodoro));
    assert!(
        rest.contains(&format!("[ {} ]", t!("ui.pomodoro_cancel"))),
        "{rest}"
    );
    assert!(rest.contains(&t!("ui.pomodoro_break_label").to_string()));
    assert!(
        !rest.contains(&t!("menu.item.tools.pomodoro").to_string()),
        "break title replaces it"
    );
}

#[test]
fn the_calculator_shows_a_result_or_the_error() {
    let mut a = app_at(&temp("r4-tp-calc"));
    a.run_action("tools.calculator");
    {
        let calc = a.calculator.as_mut().unwrap();
        for c in "2+2".chars() {
            calc.push(c);
        }
        calc.run();
    }
    let ok = text(&render(&mut a, 80, 24, draw_calculator));
    assert!(ok.contains(" = 4"), "{ok}");
    assert!(ok.contains(&format!("[ {} ]", t!("ui.calculator_run"))));
    assert!(a.layout.calculator_rects[1].width > 0 && a.layout.calculator_rects[2].width > 0);

    {
        let calc = a.calculator.as_mut().unwrap();
        calc.push('+');
        calc.run();
        assert!(matches!(
            calc.outcome,
            Some(crate::calculator_tool::Outcome::Err(_))
        ));
    }
    let err = text(&render(&mut a, 80, 24, draw_calculator));
    assert!(!err.contains(" = 4"), "the stale result is gone: {err}");
    assert!(err.contains("2+2+"), "the formula stays visible: {err}");
}

#[test]
fn the_regex_tester_lists_matches_errors_and_no_matches() {
    let mut a = app_at(&temp("r4-tp-regex"));
    a.run_action("tools.regex_tester");
    {
        let t = a.regex_tester.as_mut().unwrap();
        t.subject = "a1 b22 c333".to_string();
        t.pattern = "[0-9]+".to_string();
    }
    let hits = text(&render(&mut a, 80, 24, draw_regex_tester));
    assert!(
        hits.contains(&t!("status.matches_n", n = 3).to_string()),
        "{hits}"
    );
    for m in ["1", "22", "333"] {
        assert!(hits.contains(&format!("  {m}")), "{hits}");
    }
    assert_eq!(a.layout.regex_tester_rows[0].height, 1);

    a.regex_tester.as_mut().unwrap().pattern = "[z]+".to_string();
    let none = text(&render(&mut a, 80, 24, draw_regex_tester));
    assert!(
        none.contains(&t!("status.no_matches").to_string()),
        "{none}"
    );

    a.regex_tester.as_mut().unwrap().pattern = "(unclosed".to_string();
    let bad = text(&render(&mut a, 80, 24, draw_regex_tester));
    assert!(
        bad.contains("regex parse error") || bad.contains("unclosed group"),
        "{bad}"
    );
    assert!(!bad.contains(&t!("status.no_matches").to_string()));
}

#[test]
fn the_color_converter_paints_a_swatch_of_the_current_color() {
    let mut a = app_at(&temp("r4-tp-color"));
    a.run_action("tools.color_converter");
    {
        let conv = a.color_converter.as_mut().unwrap();
        conv.set_color(crate::color_converter_tool::Color { r: 255, g: 0, b: 0 });
    }
    let buf = render(&mut a, 80, 24, draw_color_converter);
    let drawn = text(&buf);
    assert!(
        drawn.contains("#ff0000") || drawn.contains("#FF0000"),
        "{drawn}"
    );
    let has_swatch = (0..buf.area.height)
        .any(|y| (0..buf.area.width).any(|x| buf[(x, y)].bg == Color::Rgb(255, 0, 0)));
    assert!(has_swatch, "a red swatch row is painted");

    // An unparsable field leaves no swatch behind.
    a.color_converter.as_mut().unwrap().fields[0] = "nonsense".to_string();
    let buf = render(&mut a, 80, 24, draw_color_converter);
    let any_red = (0..buf.area.height)
        .any(|y| (0..buf.area.width).any(|x| buf[(x, y)].bg == Color::Rgb(255, 0, 0)));
    assert!(!any_red);
}

#[test]
fn the_unit_converter_shows_value_units_and_output() {
    let mut a = app_at(&temp("r4-tp-unit"));
    a.run_action("convert.unit");
    if a.unit_converter.is_none() {
        a.unit_converter = Some(crate::unit_converter_tool::Converter::new());
    }
    a.unit_converter.as_mut().unwrap().value = "12".to_string();
    let drawn = text(&render(&mut a, 80, 24, draw_unit_converter));
    assert!(drawn.contains("12"), "{drawn}");
    assert!(drawn.contains(&t!("ui.unit_from").to_string()));
    assert!(drawn.contains(&t!("ui.unit_to").to_string()));
    assert!(drawn.contains(&format!(
        "= {}",
        a.unit_converter.as_ref().unwrap().output_text()
    )));
    assert_eq!(a.layout.unit_converter_rows[1].height, 1);
}

#[test]
fn an_about_dialog_draws_its_title_body_and_ok() {
    let mut a = app_at(&temp("r4-tp-dialog"));
    a.dialog = Some(crate::app::Dialog {
        title: "About R4".to_string(),
        body: "Version 9.9".to_string(),
        editor: None,
    });
    let drawn = text(&render(&mut a, 60, 12, draw_dialog));
    assert!(
        drawn.contains("About R4") && drawn.contains("Version 9.9"),
        "{drawn}"
    );
    assert!(drawn.contains(&format!("[ {} ]", t!("ui.ok"))));
    assert!(a.layout.dialog_body.width > 0);
}
