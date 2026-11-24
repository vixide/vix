//! Round-3 behavior tests for `vix-org-table` edge paths: copy-down
//! increments, time sorting, rectangles, formulas and degenerate inputs.
#![allow(clippy::unwrap_used, reason = "tests fail loudly on a broken fixture")]

use vix_org_table::{
    FormulaTarget, Row, SortKind, align, apply_tblfm, blank_field, copy_down, copy_rectangle,
    cut_rectangle, delete_column, insert_column, insert_row, kill_row, move_cell_down,
    move_cell_left, move_cell_right, move_cell_up, move_column_left, move_column_right, next_field,
    parse, parse_tblfm, paste_rectangle, previous_field, recalc, sort_rows, sum_column,
    table_range, transpose,
};

fn cell(table: &vix_org_table::Table, r: usize, c: usize) -> String {
    match &table.rows[r] {
        Row::Data(cells) => cells[c].clone(),
        Row::Hline => String::new(),
    }
}

fn down(value: &str) -> String {
    let text = format!("| {value} |");
    let (out, line, _) = copy_down(&text, 0, 2, true).unwrap();
    assert_eq!(line, 1, "the copy lands on a new second row");
    let table = parse(&out, 0, 1);
    cell(&table, 1, 0)
}

#[test]
fn copy_down_increment_covers_every_value_shape() {
    assert_eq!(down("07"), "08", "zero padded integer keeps its width");
    assert_eq!(down("-5"), "-4", "signed integer is plain arithmetic");
    assert_eq!(down("9:59"), "10:00", "minutes roll into hours");
    assert_eq!(down("x09"), "x10", "trailing digits are incremented");
    assert_eq!(down("x9"), "x10");
    assert_eq!(down("5abc"), "6abc", "leading digits when no trailing run");
    assert_eq!(down("abc"), "abc", "no digits: copied verbatim");
}

#[test]
fn copy_down_without_increment_copies_verbatim() {
    let (out, line, _) = copy_down("| a1 |", 0, 2, false).unwrap();
    assert_eq!(line, 1);
    assert_eq!(cell(&parse(&out, 0, 1), 1, 0), "a1");
}

#[test]
fn copy_down_into_empty_field_fills_from_nearest_above() {
    let text = "| a | b |\n| x |   |\n|   | y |";
    // Second row, second column is empty: fills from the nearest non-empty above ("b").
    let (out, line, _) = copy_down(text, 1, 6, false).unwrap();
    assert_eq!(line, 1);
    assert_eq!(cell(&parse(&out, 0, 2), 1, 1), "b");
    // Nothing non-empty above the very first row: no-op.
    assert!(copy_down("|   |\n| x |", 0, 2, false).is_none());
    assert!(copy_down("not a table", 0, 0, false).is_none());
}

#[test]
fn sort_time_orders_durations_after_dates_and_handles_weekday_and_time() {
    let text = "| 10:30 |\n| 9:05 |\n| <2024-03-01 Fri 08:15> |\n| [2024-03-01 Fri 07:00] |\n| 2024-02-01 |";
    let out = sort_rows(text, 0, 4, 0, SortKind::Time, false, false).unwrap();
    let t = parse(&out, 0, 4);
    let col: Vec<String> = (0..5).map(|r| cell(&t, r, 0)).collect();
    assert_eq!(
        col,
        vec![
            "2024-02-01",
            "[2024-03-01 Fri 07:00]",
            "<2024-03-01 Fri 08:15>",
            "9:05",
            "10:30"
        ]
    );
}

#[test]
fn sort_rows_degenerate_inputs() {
    assert!(sort_rows("| a |", 3, 1, 0, SortKind::Alphabetic, false, false).is_none());
    assert!(sort_rows("| a |", 0, 5, 0, SortKind::Alphabetic, false, false).is_none());
    assert!(sort_rows("plain\ntext", 0, 1, 0, SortKind::Alphabetic, false, false).is_none());
    assert!(sort_rows("|---|", 0, 0, 0, SortKind::Alphabetic, false, false).is_none());
    // Numeric: non-numbers sort last; reverse flips.
    let text = "| b |\n| 2 |\n| 1 |";
    let out = sort_rows(text, 0, 2, 0, SortKind::Numeric, false, false).unwrap();
    let t = parse(&out, 0, 2);
    assert_eq!(
        (cell(&t, 0, 0), cell(&t, 1, 0), cell(&t, 2, 0)),
        ("1".into(), "2".into(), "b".into())
    );
}

#[test]
fn rectangles_copy_cut_paste() {
    let text = "| a | b | c |\n|---+---+---|\n| d | e | f |\n| g | h | i |";
    // hlines inside the block are skipped.
    let rect = copy_rectangle(text, (2, 1), (0, 0));
    assert_eq!(
        rect,
        vec![
            vec!["a".to_string(), "b".into()],
            vec!["d".into(), "e".into()]
        ]
    );
    // Columns beyond the row yield empty strings.
    let wide = copy_rectangle("| a |", (0, 0), (0, 2));
    assert_eq!(
        wide,
        vec![vec!["a".to_string(), String::new(), String::new()]]
    );

    let (cut, got) = cut_rectangle(text, (2, 0), (3, 1));
    assert_eq!(got.len(), 2);
    let t = parse(&cut, 0, 3);
    assert_eq!(cell(&t, 2, 0), "");
    assert_eq!(cell(&t, 3, 1), "");
    assert_eq!(cell(&t, 3, 2), "i");
    // Rows outside the table are skipped; a non-table line leaves text alone.
    // (copy_rectangle does not check the line is a table row: it reads "plain"
    // as a one-cell row, current behavior pinned here.)
    let (same, copied) = cut_rectangle("plain\n| a |", (0, 0), (0, 0));
    assert_eq!(same, "plain\n| a |");
    assert_eq!(copied, vec![vec!["plain".to_string()]]);
    let (partial, _) = cut_rectangle("| a | b |\nplain", (0, 0), (1, 0));
    assert_eq!(partial, "|   | b |\nplain");

    // Paste extends rows and columns, stepping over hlines.
    let pasted = paste_rectangle(
        "| a |\n|---|\n| b |",
        0,
        2,
        &[
            vec!["1".into(), "2".into()],
            vec!["3".into()],
            vec!["4".into()],
        ],
    );
    let t = parse(&pasted, 0, pasted.lines().count() - 1);
    assert_eq!(t.rows.len(), 4);
    assert_eq!(cell(&t, 0, 1), "2");
    assert_eq!(cell(&t, 2, 0), "3");
    assert_eq!(cell(&t, 3, 0), "4");
    assert_eq!(paste_rectangle("| a |", 0, 2, &[]), "| a |");
    assert_eq!(paste_rectangle("zzz", 0, 0, &[vec!["x".into()]]), "zzz");
}

#[test]
fn transpose_and_sum_column() {
    assert_eq!(transpose("|---|", 0), None);
    assert_eq!(transpose("nothing", 0), None);
    let out = transpose("| a | b |\n|---+---|\n| c |", 0).unwrap();
    let t = parse(&out, 0, 1);
    assert_eq!(t.rows.len(), 2);
    assert_eq!(cell(&t, 0, 1), "c");
    assert_eq!(cell(&t, 1, 1), "");
    let text = "| x | 1.5 |\n|---+-----|\n| y | 2 |\n| z | n/a |";
    assert_eq!(sum_column(text, 0, 8), Some(3.5));
    assert_eq!(sum_column(text, 0, 2), Some(0.0));
    assert_eq!(sum_column("plain", 0, 0), None);
}

#[test]
fn navigation_and_structure_edges() {
    let text = "| a | b |\n|---+---|\n| c | d |";
    // previous_field at the first field of the first data row is a no-op.
    assert!(previous_field(text, 0, 2).is_none());
    // From the first field of the third row it wraps back over the hline.
    let (_, line, _) = previous_field(text, 2, 2).unwrap();
    assert_eq!(line, 0);
    // From an hline row.
    let (_, line, _) = previous_field(text, 1, 1).unwrap();
    assert_eq!(line, 0);
    // next_field from the last cell appends a row.
    let (out, line, _) = next_field(text, 2, 6).unwrap();
    assert_eq!(line, 3);
    assert_eq!(out.lines().count(), 4);
    assert!(next_field("plain", 0, 0).is_none());
    assert!(blank_field("plain", 0, 0).is_none());
    // blank_field past the end of a short row leaves cells untouched.
    assert_eq!(blank_field("| a |\n|---|", 1, 1).unwrap(), "| a |\n|---|");
    assert!(insert_row("plain", 0, true).is_none());
    assert!(kill_row("plain", 0).is_none());
    assert_eq!(
        kill_row("a\n| x |\nb", 1).unwrap(),
        "a\n\nb".replace("\n\n", "\n")
    );
    assert!(insert_column("plain", 0, 0).is_none());
    assert!(delete_column("plain", 0, 0).is_none());
    // Moving the last column right / first left is refused.
    assert!(move_column_right(text, 0, 6).is_none());
    assert!(move_column_left(text, 0, 2).is_none());
    // A ragged short row is padded when its column is swapped.
    let (out, _) = move_column_right("| a | b |\n| c |", 0, 2).unwrap();
    let t = parse(&out, 0, 1);
    assert_eq!((cell(&t, 0, 0), cell(&t, 0, 1)), ("b".into(), "a".into()));
    assert_eq!(
        (cell(&t, 1, 0), cell(&t, 1, 1)),
        (String::new(), "c".into())
    );
    // Cell moves refuse at the edges and on hlines.
    assert!(move_cell_up(text, 0, 2).is_none());
    assert!(move_cell_down(text, 2, 2).is_none());
    assert!(move_cell_left(text, 0, 2).is_none());
    assert!(move_cell_right(text, 0, 6).is_none());
    assert!(move_cell_left(text, 1, 2).is_none());
    assert!(move_cell_right("plain", 0, 0).is_none());
    // Deleting the last column lands on the new last column.
    let (out, _) = delete_column("| a | b |", 0, 6).unwrap();
    assert_eq!(out, "| a |");
    assert!(align("nope", 0).is_none());
    assert_eq!(table_range("x", 9), None);
}

fn tbl(rows: &str, fm: &str) -> String {
    let text = format!("{rows}\n#+TBLFM: {fm}");
    let n = rows.lines().count();
    apply_tblfm(&text, 0, n - 1, n).unwrap()
}

fn data(text: &str, n: usize) -> Vec<Vec<String>> {
    let t = parse(text, 0, n - 1);
    t.rows
        .iter()
        .map(|r| match r {
            Row::Data(c) => c.clone(),
            Row::Hline => vec!["-".into()],
        })
        .collect()
}

#[test]
fn formulas_aggregates_and_formats() {
    let rows = "| 1 | 4 | |\n| 3 | 8 | |\n| x | 2 | |";
    let out = tbl(rows, "$3=vsum(@1$2..@3$2)");
    assert_eq!(data(&out, 3)[0][2], "14");
    let out = tbl(rows, "@1$3=vmean(@1$2..@3$2)");
    assert_eq!(data(&out, 3)[0][2], "4.67");
    let out = tbl(
        rows,
        "@1$3=vmax(@1$2..@3$2)::@2$3=vmin(@1$2..@3$2)::@3$3=vcount(@1$1..@3$1)",
    );
    let d = data(&out, 3);
    assert_eq!(
        (d[0][2].as_str(), d[1][2].as_str(), d[2][2].as_str()),
        ("8", "2", "2")
    );
    // Empty ranges: mean is zero.
    let out = tbl("| x |\n| y |", "@1$2=vmean(@1$1..@2$1)");
    assert_eq!(data(&out, 2)[0][1], "0");
    // printf format.
    let out = tbl("| 1 |\n| 3 |", "@1$2=$1/3;%.3f");
    assert_eq!(data(&out, 2)[0][1], "0.333");
    // A tiny negative renders as "-0" today (cosmetic quirk, pinned).
    let out = tbl("| 1 |", "@1$2=-$1/1000");
    assert_eq!(data(&out, 1)[0][1], "-0");
    let out = tbl("| 1 |", "@1$2=-$1/4");
    assert_eq!(data(&out, 1)[0][1], "-0.25");
}

#[test]
fn formula_errors_become_error_markers() {
    for (fm, label) in [
        ("@1$2=@x", "digits missing after @"),
        ("@1$2=@1", "missing $"),
        ("@1$2=@1$", "missing column digits"),
        ("@1$2=$", "missing column after $"),
        ("@1$2=@9$1", "row out of range"),
        ("@1$2=vsum(@1$1)", "not a range"),
        ("@1$2=vsum(@1$1..zz)", "bad endpoint right"),
        ("@1$2=vsum(zz..@1$1)", "bad endpoint left"),
        ("@1$2=vsum(@1$1..@1$1", "unbalanced"),
        ("@1$2=1==1", "not a number"),
        ("@1$2=1/0", "non-finite"),
        ("@1$2=(", "syntax"),
    ] {
        let out = tbl("| 1 |", fm);
        assert_eq!(data(&out, 1)[0][1], "#ERROR", "{label}: {fm}");
    }
    // `$N` in a field formula has a current row, so it works.
    let out = tbl("| 2 |", "@1$2=$1*2");
    assert_eq!(data(&out, 1)[0][1], "4");
}

#[test]
fn formula_targets_out_of_range_are_ignored_and_malformed_lines_rejected() {
    let out = tbl("| 1 |", "@5$1=1::@1$0=1::$3=$1+1");
    let d = data(&out, 1);
    assert_eq!(
        d[0][0], "1",
        "writes to a missing row/column zero are dropped"
    );
    assert_eq!(d[0][2], "2", "a column formula grows the short row");
    let out = tbl("| 1 |\n|---|\n| 2 |", "$2=$1");
    assert_eq!(data(&out, 3)[1], vec!["-".to_string()]);
    assert!(parse_tblfm("not a formula line").is_none());
    assert!(apply_tblfm("| 1 |", 0, 0, 7).is_none());
    let parsed = parse_tblfm("#+TBLFM: @2$3=1;%.1f::$4=2").unwrap();
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].target, FormulaTarget::Field(2, 3));
    assert_eq!(parsed[0].format.as_deref(), Some("%.1f"));
    assert_eq!(parsed[1].target, FormulaTarget::Column(4));
    // Bad lines inside the TBLFM are skipped/None without panicking.
    let _ = parse_tblfm("#+TBLFM: @x$y=1");
    let _ = parse_tblfm("#+TBLFM: =1");
    let _ = parse_tblfm("#+TBLFM: $0=1");
    let _ = parse_tblfm("#+TBLFM: @$=1");
}

#[test]
fn recalc_realigns_and_applies_adjacent_tblfm() {
    let text = "|1|\n#+TBLFM: $2=$1+1";
    let out = recalc(text, 0).unwrap();
    assert!(out.starts_with("| 1 | 2 |"), "got {out:?}");
    assert_eq!(
        recalc("|1|\n\n#+TBLFM: $2=1", 0).unwrap(),
        "| 1 |\n\n#+TBLFM: $2=1"
    );
    assert!(recalc("zz", 0).is_none());
}
