//! Fuzz the shared CSV/TSV/JSON conversion core (`vix-convert-tabular`),
//! shared by all six CSV/TSV/JSON `Tools → Convert` tool crates.
//!
//! `vix-convert-tabular` already has `proptest` coverage for the no-panic
//! property (`.*`-generated strings); this target adds coverage-guided
//! fuzzing on top, and checks a round-trip property proptest's uniform
//! random generation is unlikely to hit: writing then reparsing rows is a
//! **fixed point** — `write_csv(parse_csv(write_csv(rows)))` equals
//! `write_csv(rows)`.
//!
//! This is deliberately not "reparsing gives back the exact original rows":
//! `write_csv` intentionally *changes* fields that would be interpreted as a
//! spreadsheet formula (`needs_formula_guard` — a leading `=`, `+`, `-`, `@`,
//! tab, or CR), prefixing a `'` to neutralize CSV/formula injection (OWASP
//! "CSV Injection"). The first fuzz run here found exactly that: `["@"]`
//! writes as `'@` and reparses as `["'@"]` — correct, intended behavior, not
//! a round-trip bug. The fixed-point framing survives it: guarding is
//! idempotent (a field already prefixed with `'` no longer matches the
//! guard), so a second write of the reparsed rows must equal the first.

#![no_main]

use libfuzzer_sys::fuzz_target;
use vix_convert_tabular::{json_to_rows, parse_csv, parse_tsv, rows_to_json, write_csv, write_tsv};

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };

    let csv_rows = parse_csv(text);
    let rewritten = write_csv(&csv_rows);
    let reparsed = parse_csv(&rewritten);
    let rewritten_again = write_csv(&reparsed);
    assert_eq!(
        rewritten_again, rewritten,
        "CSV write is not a fixed point after one parse/write round trip"
    );

    let tsv_rows = parse_tsv(text);
    let rewritten = write_tsv(&tsv_rows);
    let reparsed = parse_tsv(&rewritten);
    let rewritten_again = write_tsv(&reparsed);
    assert_eq!(
        rewritten_again, rewritten,
        "TSV write is not a fixed point after one parse/write round trip"
    );

    // rows_to_json never panics on any rows shape (including a ragged one:
    // parse_csv/parse_tsv can return rows of differing lengths), and always
    // emits well-formed JSON — `json_to_rows` must always accept it back,
    // even though the two functions' row shapes are not otherwise symmetric
    // (a duplicate column name in the header, e.g., silently collapses when
    // `rows_to_json` builds each row's `Map`).
    let json = rows_to_json(&csv_rows);
    assert!(
        json_to_rows(&json).is_ok(),
        "rows_to_json produced JSON that json_to_rows rejected: {json:?}"
    );

    // The reverse direction must be total too, and never panic on truncated
    // or malformed JSON.
    let _ = json_to_rows(text);
});
