//! Fuzz the Org table editor: parsing, rendering, alignment, and `TBLFM`.
//!
//! Tables are parsed out of whatever the buffer happens to contain — half-typed
//! rows, ragged pipes, unicode-wide cells, a formula line referring to columns
//! that are not there. Every entry point takes a `(line, col)` cursor from the
//! editor, so out-of-range coordinates are normal input, not a contract
//! violation.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Two leading bytes choose the cursor; the rest is buffer text.
    let (head, text) = data.split_at(data.len().min(2));
    let Ok(text) = std::str::from_utf8(text) else {
        return;
    };
    let lines = text.lines().count().max(1);
    let line = head.first().map_or(0, |b| usize::from(*b)) % lines;
    let col = head.get(1).map_or(0, |b| usize::from(*b));

    if let Some((first, last)) = vix_org_table::table_range(text, line) {
        assert!(first <= last, "table range is inverted");
        let table = vix_org_table::parse(text, first, last);
        let rendered = vix_org_table::render(&table);
        // Rendering a parsed table must produce something parseable again, with
        // the same shape: the editor re-parses after every structural edit.
        // `parse` takes an *inclusive* last line.
        let last_rendered = rendered.lines().count().saturating_sub(1);
        let reparsed = vix_org_table::parse(&rendered, 0, last_rendered);
        assert_eq!(
            reparsed.rows.len(),
            table.rows.len(),
            "render/parse lost rows: {rendered:?}"
        );
        let _ = vix_org_table::to_tsv(&table);
    }

    let _ = vix_org_table::align(text, line);
    let _ = vix_org_table::recalc(text, line);
    let _ = vix_org_table::next_field(text, line, col);
    let _ = vix_org_table::previous_field(text, line, col);
    let _ = vix_org_table::next_row(text, line, col);
    let _ = vix_org_table::blank_field(text, line, col);
    let _ = vix_org_table::sum_column(text, line, col);
    let _ = vix_org_table::transpose(text, line);
    let _ = vix_org_table::from_delimited(text);

    // A `#+TBLFM:` line is user-authored arithmetic; parsing it must be total.
    for l in text.lines() {
        let _ = vix_org_table::parse_tblfm(l);
    }
});
