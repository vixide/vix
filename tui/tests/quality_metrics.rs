//! The quality-metrics dashboard's counters (T833,
//! `tests/support/quality_metrics.rs`, which writes
//! `docs/reference/quality-metrics.md` through `list_commands --write`).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "benches, examples and tests fail loudly on a broken fixture"
)]

#[path = "support/quality_metrics.rs"]
mod quality_metrics;

use quality_metrics::{Catalog, add_catalog_file, allowlist, count_file, mask, percent, thousands};

#[test]
fn mask_keeps_offsets_and_blanks_comments_and_literals() {
    let src = "let s = \"#[test] é\"; // #[test]\n/* a /* #[test] */ b */ fn f<'a>(c: char) { 'x'; '\\''; r#\"#[test]\"#; }\n";
    let m = mask(src);
    assert_eq!(m.len(), src.len());
    assert!(!m.contains("#[test]"), "{m}");
    assert!(m.contains("fn f<'a>(c: char)"), "lifetimes stay code: {m}");
    assert_eq!(m.lines().count(), src.lines().count());
    assert!(m.contains("r#\"") && m.contains("\"#;"), "{m}");
}

#[test]
fn raw_identifiers_and_byte_strings_are_handled() {
    let m = mask("let r#type = br\"#[test]\"; #[test] fn t() {}");
    assert_eq!(m.matches("#[test]").count(), 1, "{m}");
    assert!(m.contains("r#type"));
}

#[test]
fn counts_tests_properties_ignores_and_suppressions() {
    let src = r##"
#![allow(dead_code, reason = "x")]
#[test]
fn a() {}
#[tokio::test(flavor = "current_thread")]
async fn b() {}
#[test]
#[ignore = "needs a real Vim"]
fn c() {}
#[test]
#[ignore = "flaky: T9, races the watcher"]
fn d() {}
#[test]
#[ignore]
fn e() {}
#[ignore_me]
fn f() {}
proptest! {
    #[test]
    fn p1(x in 0..3u8) { prop_assert!(x < 3, "{}", "}"); }
    #[test]
    fn p2(x in any::<u8>()) { let _ = x; }
}
#[expect(clippy::too_many_lines, reason = "y")]
fn g() { let _ = "#[test] #[ignore] proptest! {"; }
// #[test] #[ignore]
"##;
    let c = count_file(src);
    assert_eq!(c.tests, 7, "{c:?}");
    assert_eq!(c.properties, 2);
    assert_eq!(c.proptest_blocks, 1);
    assert_eq!(c.ignored, 3);
    assert_eq!(c.flaky, 1);
    assert_eq!(c.suppressions, 2);
    assert_eq!(c.lines, src.lines().count());
}

#[test]
fn catalog_counts_keys_per_locale() {
    let mut cat = Catalog::default();
    add_catalog_file(
        &mut cat,
        "_version: 2\na.one:\n  en: \"One\"\n  fr: \"Un\"\na.two:\n  en: |\n    Two\n  fr: \"\"\n",
    )
    .unwrap();
    add_catalog_file(&mut cat, "b.three:\n  en: \"Three\"\n  tlh: \"wej\"\n").unwrap();
    assert_eq!(cat.files, 2);
    assert_eq!(cat.keys, 3);
    assert_eq!(cat.per_locale.get("en"), Some(&3));
    assert_eq!(
        cat.per_locale.get("fr"),
        Some(&1),
        "an empty string is missing"
    );
    assert_eq!(cat.per_locale.get("tlh"), Some(&1));
    assert!(add_catalog_file(&mut cat, "- a list").is_err());
}

#[test]
fn allowlists_count_entries_and_their_numbers() {
    let src = "const PRINT_ALLOWED: &[(&str, usize, &str)] = &[\n    (\n        \"a.rs\",\n        13,\n        \"why\",\n    ),\n    (\n        \"b.rs\",\n        2,\n        \"3\",\n    ),\n];\nconst SINK_ALLOWED: &[(&str, &str, &str)] = &[];\n";
    assert_eq!(allowlist(src, "PRINT_ALLOWED"), Some((2, 15)));
    assert_eq!(allowlist(src, "SINK_ALLOWED"), Some((0, 0)));
    assert_eq!(allowlist(src, "MISSING"), None);
}

#[test]
fn percent_rounds_down_to_one_decimal() {
    assert_eq!(percent(4020, 4024), "99.9%");
    assert_eq!(percent(5, 5), "100.0%");
    assert_eq!(percent(0, 7), "0.0%");
    assert_eq!(percent(1, 0), "n/a");
}

#[test]
fn the_real_allowlists_are_found() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let lints = std::fs::read_to_string(root.join("tests/code_lints.rs")).unwrap();
    let (files, prints) = allowlist(&lints, "PRINT_ALLOWED").expect("PRINT_ALLOWED moved");
    assert!(
        files > 0 && prints >= files,
        "{files} files, {prints} prints"
    );
    assert!(
        allowlist(&lints, "SINK_ALLOWED").is_some(),
        "SINK_ALLOWED moved"
    );
}

#[test]
fn thousands_groups_digits() {
    assert_eq!(thousands(0), "0");
    assert_eq!(thousands(999), "999");
    assert_eq!(thousands(1000), "1,000");
    assert_eq!(thousands(330_838), "330,838");
    assert_eq!(thousands(1_234_567), "1,234,567");
}
