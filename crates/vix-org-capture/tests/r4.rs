//! Round-4 behavior tests for org-capture: target parsing, placeholder
//! expansion edge cases, previews, entry wrapping and filing placements.

use vix_org_capture::{
    Context, EntryType, Target, datetree_path, expand, extract_prompts, insert_datetree,
    insert_top_level, insert_under_headline, insert_under_id, preview, wants_tags, wrap_entry,
};

fn ctx() -> Context {
    Context {
        year: 2026,
        month: 3,
        day: 9,
        hour: 7,
        minute: 5,
        weekday: "Mon".into(),
        annotation: "[[file:a.org][a]]".into(),
        initial: "sel".into(),
        file_name: "a.org".into(),
        file_path: "/tmp/a.org".into(),
        clipboard: "clip".into(),
    }
}

#[test]
fn targets_parse_every_form() {
    assert_eq!(Target::parse(" id: abc-1 "), Target::Id("abc-1".into()));
    assert_eq!(
        Target::parse("file:notes.org"),
        Target::File("notes.org".into())
    );
    assert_eq!(
        Target::parse("file+datetree: j.org "),
        Target::FileDatetree("j.org".into())
    );
    assert_eq!(
        Target::parse("file+headline: n.org # Inbox "),
        Target::FileHeadline("n.org".into(), "Inbox".into())
    );
    assert_eq!(
        Target::parse("file+headline:n.org"),
        Target::File("n.org".into())
    );
    assert_eq!(Target::parse("cursor"), Target::Cursor);
    assert_eq!(Target::parse(""), Target::Cursor);
}

#[test]
fn strftime_placeholders_in_angle_brackets() {
    let c = ctx();
    let out = expand("%<%Y-%m-%d %H:%M %a 100%% %q>", &[], None, &c);
    assert_eq!(out.text, "2026-03-09 07:05 Mon 100% %q");
    // A format ending in a lone `%` keeps it.
    assert_eq!(expand("%<ab%>", &[], None, &c).text, "ab%");
    // An unterminated `%<` keeps the percent and the rest as text.
    assert_eq!(expand("a %<b", &[], None, &c).text, "a %<b");
}

#[test]
fn simple_placeholders_and_literals() {
    let c = ctx();
    let out = expand("%t|%T|%u|%U|%a|%i|%f|%F|%c|%%|%z|end%", &[], None, &c);
    assert_eq!(
        out.text,
        "<2026-03-09 Mon>|<2026-03-09 Mon 07:05>|[2026-03-09 Mon]|[2026-03-09 Mon 07:05]|\
         [[file:a.org][a]]|sel|a.org|/tmp/a.org|clip|%|%z|end%"
    );
    assert_eq!(out.cursor_offset, None);
    // The last `%?` wins and offsets count chars, not bytes.
    let out = expand("é%?x%?y", &[], None, &c);
    assert_eq!(out.text, "éxy");
    assert_eq!(out.cursor_offset, Some(2));
}

#[test]
fn caret_placeholders_consume_answers_and_tags() {
    let c = ctx();
    let answers = vec!["one".to_string()];
    let out = expand("%^{A} %^{B} %^g %^G", &answers, Some(":x:"), &c);
    assert_eq!(out.text, "one  :x: :x:");
    // No tags given: the tag placeholders expand to nothing.
    assert_eq!(expand("[%^g]", &[], None, &c).text, "[]");
    // A `%^{` with no close and a bare `%^z` stay literal.
    assert_eq!(expand("%^{open", &[], None, &c).text, "%^{open");
    assert_eq!(expand("%^z", &[], None, &c).text, "%^z");
    assert_eq!(expand("%^", &[], None, &c).text, "%^");
}

#[test]
fn prompts_and_tags_detection() {
    let p = extract_prompts("%^{Title} and %^{Pri|A|B|C} and %^{unclosed");
    assert_eq!(p.len(), 2);
    assert_eq!((p[0].label.as_str(), p[0].default.as_str()), ("Title", ""));
    assert_eq!(p[0].choices, Vec::<String>::new());
    assert_eq!(p[1].choices, ["A", "B", "C"]);
    assert!(wants_tags("x %^G") && wants_tags("%^g") && !wants_tags("%^h") && !wants_tags("%g"));
}

#[test]
fn preview_marks_the_current_and_pending_fields() {
    let tpl = "* %^{Title} / %^{Where} %^g";
    let prompts = extract_prompts(tpl);
    let none: Vec<String> = Vec::new();
    assert_eq!(
        preview(tpl, &prompts, &none, Some(0), None, false, &ctx()),
        "* \u{2039}Title\u{203a} / [Where] [Tags]"
    );
    assert_eq!(
        preview(
            tpl,
            &prompts,
            &["T".to_string(), "W".to_string()],
            None,
            None,
            true,
            &ctx()
        ),
        "* T / W \u{2039}Tags\u{203a}"
    );
    assert_eq!(
        preview(
            tpl,
            &prompts,
            &["T".to_string()],
            Some(1),
            Some(":a:"),
            false,
            &ctx()
        ),
        "* T / \u{2039}Where\u{203a} :a:"
    );
}

#[test]
fn wrap_entry_shapes_each_type_once() {
    assert_eq!(wrap_entry(EntryType::Entry, "Buy"), "* Buy");
    assert_eq!(wrap_entry(EntryType::Entry, "** Sub"), "** Sub");
    assert_eq!(wrap_entry(EntryType::Item, "x"), "- x");
    assert_eq!(wrap_entry(EntryType::Item, "+ x"), "+ x");
    assert_eq!(wrap_entry(EntryType::CheckItem, "todo"), "- [ ] todo");
    assert_eq!(wrap_entry(EntryType::CheckItem, "- [X] done"), "- [X] done");
    assert_eq!(wrap_entry(EntryType::TableLine, "a | b"), "| a | b |");
    assert_eq!(wrap_entry(EntryType::TableLine, "| a |"), "| a |");
    assert_eq!(wrap_entry(EntryType::Plain, "raw\nlines"), "raw\nlines");
    // Only the first line is wrapped.
    assert_eq!(wrap_entry(EntryType::Entry, "A\nbody"), "* A\nbody");
}

#[test]
fn top_level_insertion_appends_prepends_and_pads() {
    assert_eq!(insert_top_level("* A", "* B", false, 0), "* A\n* B");
    assert_eq!(
        insert_top_level(
            ":PROPERTIES:\n:ID: x\n:END:\n#+TITLE: T\n* A",
            "* B",
            true,
            0
        ),
        ":PROPERTIES:\n:ID: x\n:END:\n#+TITLE: T\n* B\n* A"
    );
    // Leading blank lines are front matter too.
    assert_eq!(insert_top_level("\n\n* A", "* B", true, 0), "\n\n* B\n* A");
    // An unterminated file-level drawer is left as found.
    assert_eq!(
        insert_top_level(":PROPERTIES:\n:ID: x", "* B", true, 0),
        ":PROPERTIES:\n:ID: x\n* B"
    );
    assert!(insert_top_level("* A", "* B", false, 1).contains("\n\n"));
}

#[test]
fn headline_insertion_creates_nests_and_skips_drawers() {
    // Missing headline: created at the end (blank line separates).
    assert_eq!(
        insert_under_headline("* A", "Inbox", "* T", false, 0),
        "* A\n\n* Inbox\n** T"
    );
    // Missing headline in an empty file: no leading blank.
    assert_eq!(
        insert_under_headline("", "Inbox", "* T", false, 0),
        "* Inbox\n** T"
    );
    // Case and trailing tags are ignored when matching; deeper entries keep their shape.
    let text = "* Inbox :work:\n:PROPERTIES:\n:CREATED: 1\n:END:\n** Old\n* Other";
    assert_eq!(
        insert_under_headline(text, "inbox", "* New\n** Kid\nbody", true, 0),
        "* Inbox :work:\n:PROPERTIES:\n:CREATED: 1\n:END:\n** New\n*** Kid\nbody\n** Old\n* Other"
    );
    assert_eq!(
        insert_under_headline(text, "Inbox", "* New", false, 0),
        "* Inbox :work:\n:PROPERTIES:\n:CREATED: 1\n:END:\n** Old\n** New\n* Other"
    );
    // A non-headline entry is inserted verbatim.
    assert_eq!(
        insert_under_headline("* Inbox", "Inbox", "- item", false, 0),
        "* Inbox\n- item"
    );
    // An unterminated drawer: the entry goes at its last line.
    assert_eq!(
        insert_under_headline("* Inbox\n:PROPERTIES:\n:X: 1", "Inbox", "* N", true, 0),
        "* Inbox\n:PROPERTIES:\n:X: 1\n** N"
    );
    // A title ending in a colon word is not mistaken for tags.
    assert!(insert_under_headline("* Re: plan :", "Re: plan :", "* N", false, 0).contains("** N"));
}

#[test]
fn id_insertion_finds_owners_and_files_file_level_ids() {
    let text = "* Project\n:PROPERTIES:\n:ID: abc\n:END:\n** Sub\n* Next";
    assert_eq!(
        insert_under_id(text, "abc", "* Note", false, 0).unwrap(),
        "* Project\n:PROPERTIES:\n:ID: abc\n:END:\n** Sub\n** Note\n* Next"
    );
    assert_eq!(
        insert_under_id(text, " abc ", "* Note", true, 0).unwrap(),
        "* Project\n:PROPERTIES:\n:ID: abc\n:END:\n** Note\n** Sub\n* Next"
    );
    // A file-level id (no headline above it) files at the top level.
    let file_level = ":PROPERTIES:\n:id: top\n:END:\n#+TITLE: T\n* A";
    assert_eq!(
        insert_under_id(file_level, "top", "* New", true, 0).unwrap(),
        ":PROPERTIES:\n:id: top\n:END:\n#+TITLE: T\n* New\n* A"
    );
    assert!(
        insert_under_id(file_level, "top", "* New", false, 0)
            .unwrap()
            .ends_with("* A\n* New")
    );
    assert!(insert_under_id(text, "nope", "* Note", false, 0).is_none());
    // Matching is case-sensitive on the value.
    assert!(insert_under_id(text, "ABC", "* Note", false, 0).is_none());
}

#[test]
fn datetree_builds_reuses_and_prepends() {
    let c = ctx();
    assert_eq!(
        datetree_path(&c),
        [
            "2026".to_string(),
            "2026-03 March".to_string(),
            "2026-03-09 Mon".to_string()
        ]
    );
    let bad_month = Context { month: 13, ..ctx() };
    assert_eq!(datetree_path(&bad_month)[1], "2026-13 ");
    let zero_month = Context { month: 0, ..ctx() };
    assert_eq!(datetree_path(&zero_month)[1], "2026-00 ");

    let first = insert_datetree("* Other", &c, "* Task", false, 0);
    assert_eq!(
        first,
        "* Other\n\n* 2026\n** 2026-03 March\n*** 2026-03-09 Mon\n**** Task"
    );
    // A second capture the same day lands under the same day node.
    let second = insert_datetree(&first, &c, "* Later", false, 0);
    assert!(second.ends_with("**** Task\n**** Later"));
    assert_eq!(second.matches("2026-03-09 Mon").count(), 1);
    // Prepending puts it before existing day children.
    let third = insert_datetree(&second, &c, "* First", true, 0);
    assert!(third.contains("*** 2026-03-09 Mon\n**** First\n**** Task"));
    // A new day in the same month reuses the year and month.
    let next = Context {
        day: 10,
        weekday: "Tue".into(),
        ..ctx()
    };
    let fourth = insert_datetree(&third, &next, "* Tue task", false, 0);
    assert_eq!(fourth.matches("** 2026-03 March").count(), 1);
    assert!(fourth.ends_with("*** 2026-03-10 Tue\n**** Tue task"));
}
