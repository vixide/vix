//! Behavior tests for the corners of the Org text operations that the main
//! suite leaves out: sibling navigation past a parent boundary, keyword-only
//! headlines, statistics cookies over checkboxes, and subtree moves.

use super::*;
use crate::export::safe_href;

#[test]
fn sibling_navigation_stops_at_the_parent_boundary() {
    let text = "* A\n** A1\n** A2\n* B\n** B1";
    // A1 and A2 are siblings; B1 has no sibling and A2 has none after it.
    assert_eq!(nav_forward_same(text, 1), Some(2));
    assert_eq!(nav_backward_same(text, 2), Some(1));
    assert_eq!(nav_forward_same(text, 2), None);
    assert_eq!(nav_backward_same(text, 1), None);
    assert_eq!(nav_forward_same(text, 4), None);
    assert_eq!(nav_backward_same(text, 4), None);
    // Top-level headlines are siblings across subtrees.
    assert_eq!(nav_forward_same(text, 0), Some(3));
    assert_eq!(nav_backward_same(text, 3), Some(0));
    // Before any headline there is nothing to navigate from.
    assert_eq!(nav_forward_same("intro\n* A", 0), None);
    assert_eq!(nav_backward_same("intro\n* A", 0), None);
}

#[test]
fn headlines_lists_line_level_and_trimmed_title() {
    let text = "intro\n* Top  \n** TODO Child\nbody\n*** x";
    assert_eq!(
        headlines(text),
        vec![
            (1, 1, "Top".to_string()),
            (2, 2, "TODO Child".to_string()),
            (4, 3, "x".to_string()),
        ]
    );
    assert_eq!(headlines("no headlines here"), []);
}

#[test]
fn governing_subtree_walks_up_from_a_body_line() {
    let text = "intro\n* A\nbody\n** A1\n* B";
    assert_eq!(governing_subtree(text, 2), Some((1, 4)));
    assert_eq!(governing_subtree(text, 3), Some((3, 4)));
    assert_eq!(governing_subtree(text, 0), None);
}

#[test]
fn sort_children_orders_case_insensitively_and_needs_two() {
    let text = "* P\n** b\nbody b\n** A\n** c";
    assert_eq!(
        sort_children(text, 0).as_deref(),
        Some("* P\n** A\n** b\nbody b\n** c")
    );
    // Only one child: nothing to sort.
    assert_eq!(sort_children("* P\n** only", 0), None);
    // Before the first headline the top-level trees are sorted.
    assert_eq!(
        sort_children("intro\n* b\n* a", 0).as_deref(),
        Some("intro\n* a\n* b")
    );
}

#[test]
fn refile_moves_a_subtree_under_a_target_and_refuses_its_own_inside() {
    let text = "* A\n** A1\n* B\nb body";
    let (out, at) = refile(text, 1, 2).unwrap();
    assert_eq!(out, "* A\n* B\nb body\n** A1");
    assert_eq!(at, 3);
    // The target lies inside the moved subtree.
    assert_eq!(refile(text, 0, 1), None);
    // The target line is not a headline.
    assert_eq!(refile(text, 1, 3), None);
    // Before any headline there is nothing to refile.
    assert_eq!(refile("intro\n* A", 0, 1), None);
}

#[test]
fn paste_subtree_relevels_to_a_sibling_of_the_governing_headline() {
    let text = "* A\n** A1\n* B";
    let (out, at) = paste_subtree(text, 1, "* Clip\n** Sub\n").unwrap();
    assert_eq!(out, "* A\n** A1\n** Clip\n*** Sub\n* B");
    assert_eq!(at, 2);
    // Outside any subtree the paste lands after the cursor line at level 1.
    let (out, at) = paste_subtree("intro\n* A", 0, "** Deep").unwrap();
    assert_eq!(out, "intro\n* Deep\n* A");
    assert_eq!(at, 1);
    // A clip that does not start with a headline is refused.
    assert_eq!(paste_subtree(text, 0, "plain"), None);
}

#[test]
fn new_heading_matches_the_governing_level() {
    let (out, at) = new_heading("* A\n** A1\nbody", 2);
    assert_eq!(out, "* A\n** A1\nbody\n** ");
    assert_eq!(at, 3);
    let (out, at) = new_heading("intro", 0);
    assert_eq!(out, "intro\n* ");
    assert_eq!(at, 1);
}

#[test]
fn cycling_keyword_only_headlines() {
    assert_eq!(cycle_todo("* TODO", 0).as_deref(), Some("* DONE"));
    assert_eq!(cycle_todo("* DONE", 0).as_deref(), Some("* "));
    assert_eq!(cycle_todo("* DONE x", 0).as_deref(), Some("* x"));
    assert_eq!(cycle_todo("not a headline", 0), None);
    assert_eq!(cycle_todo("* x", 5), None);
}

#[test]
fn set_headline_keyword_replaces_inserts_and_ignores_non_headlines() {
    assert_eq!(set_headline_keyword("plain", DONE), "plain");
    assert_eq!(set_headline_keyword("* TODO x", DONE), "* DONE x");
    assert_eq!(set_headline_keyword("* DONE x", TODO), "* TODO x");
    assert_eq!(set_headline_keyword("* TODO", DONE), "* DONE");
    assert_eq!(set_headline_keyword("* DONE", TODO), "* TODO");
    assert_eq!(set_headline_keyword("* x", DONE), "* DONE x");
}

#[test]
fn priorities_survive_bare_keywords_and_clamp_at_the_ends() {
    assert_eq!(priority("* TODO [#B] x"), Some('B'));
    assert_eq!(priority("* DONE [#C]"), Some('C'));
    assert_eq!(priority("* TODO"), None);
    assert_eq!(priority("not a headline"), None);
    assert_eq!(
        set_priority("* TODO", 0, Some('A')).as_deref(),
        Some("* TODO [#A]")
    );
    assert_eq!(
        set_priority("* DONE", 0, Some('A')).as_deref(),
        Some("* DONE [#A]")
    );
    assert_eq!(
        set_priority("* DONE [#A] x", 0, None).as_deref(),
        Some("* DONE x")
    );
    assert_eq!(set_priority("plain", 0, Some('A')), None);
    // Stepping: no cookie yields the default; the ends clamp (no wraparound).
    assert_eq!(
        priority_up("* x", 0, 'A', 'C', 'B').as_deref(),
        Some("* [#B] x")
    );
    assert_eq!(
        priority_up("* [#B] x", 0, 'A', 'C', 'B').as_deref(),
        Some("* [#A] x")
    );
    assert_eq!(
        priority_up("* [#A] x", 0, 'A', 'C', 'B').as_deref(),
        Some("* [#A] x")
    );
    assert_eq!(
        priority_down("* [#C] x", 0, 'A', 'C', 'B').as_deref(),
        Some("* [#C] x")
    );
    assert_eq!(
        priority_down("* [#A] x", 0, 'A', 'C', 'B').as_deref(),
        Some("* [#B] x")
    );
    // The range may be given low-to-high too ("1" is the highest here).
    assert_eq!(
        priority_down("* [#3] x", 0, '3', '1', '2').as_deref(),
        Some("* [#2] x")
    );
    assert_eq!(priority_up("plain", 0, 'A', 'C', 'B'), None);
    assert_eq!(priority_down("* x", 9, 'A', 'C', 'B'), None);
}

#[test]
fn closing_a_headline_adds_closed_and_logs_notes_newest_first() {
    let text = "* TODO job\nbody";
    let once = close_headline(text, 0, "2026-01-02 Fri 10:00", "first").unwrap();
    assert_eq!(
        once,
        "* DONE job\nCLOSED: [2026-01-02 Fri 10:00]\n:LOGBOOK:\n- Note taken on [2026-01-02 Fri 10:00] \\\\\n  first\n:END:\nbody"
    );
    // Closing again refreshes CLOSED and prepends to the existing drawer.
    let twice = close_headline(&once, 0, "2026-02-03 Tue 11:00", "second\nline two").unwrap();
    assert!(twice.contains("CLOSED: [2026-02-03 Tue 11:00]"));
    assert!(!twice.contains("CLOSED: [2026-01-02"));
    assert_eq!(twice.matches(":LOGBOOK:").count(), 1);
    let second = twice.find("second").unwrap();
    let first = twice.find("first").unwrap();
    assert!(second < first, "{twice}");
    assert!(twice.contains("  line two"));
    // No note: only CLOSED is recorded.
    assert_eq!(
        close_headline("* x", 0, "T", "").as_deref(),
        Some("* DONE x\nCLOSED: [T]")
    );
    assert_eq!(close_headline("plain", 0, "T", ""), None);
}

#[test]
fn checkbox_toggle_and_detection() {
    assert!(has_checkbox("  - [ ] a"));
    assert!(has_checkbox("1. [X] b"));
    assert!(!has_checkbox("- plain"));
    assert_eq!(toggle_checkbox("- [ ] a", 0).as_deref(), Some("- [x] a"));
    assert_eq!(toggle_checkbox("- [X] a", 0).as_deref(), Some("- [ ] a"));
    assert_eq!(toggle_checkbox("- [-] a", 0).as_deref(), Some("- [ ] a"));
    assert_eq!(toggle_checkbox("- plain", 0), None);
    assert_eq!(toggle_checkbox("- [ ] a", 3), None);
}

#[test]
fn parent_checkboxes_follow_their_children_and_count_them() {
    let text = "- [ ] parent [/]\n  - [x] a\n  - [ ] b\n- [ ] other\n  - [x] c";
    assert_eq!(
        update_statistics(text),
        "- [-] parent [1/2]\n  - [x] a\n  - [ ] b\n- [X] other\n  - [x] c"
    );
    // A partially checked child makes its parent partial even with none done.
    let nested = "- [ ] top\n  - [-] mid\n    - [ ] leaf";
    let out = update_statistics(nested);
    assert!(out.starts_with("- [ ] top"), "{out}");
    // A headline between two lists resets the nesting.
    let split = "- [ ] a\n* H\n  - [x] b";
    assert_eq!(update_statistics(split), split);
}

#[test]
fn headline_cookies_count_checkboxes_or_todo_children() {
    let boxes = "* Tasks [/]\n- [x] a\n- [ ] b\n- [x] c";
    assert_eq!(
        update_statistics(boxes),
        "* Tasks [2/3]\n- [x] a\n- [ ] b\n- [x] c"
    );
    let pct = "* Tasks [%]\n- [x] a\n- [ ] b\n- [ ] c";
    assert_eq!(update_statistics(pct).lines().next(), Some("* Tasks [33%]"));
    // With no checkboxes, direct TODO children are counted.
    let todos = "* Project [/]\n** TODO a\n** DONE b\n*** TODO nested\n** plain";
    assert_eq!(
        update_statistics(todos).lines().next(),
        Some("* Project [1/2]")
    );
    // Only the shallowest checkboxes of the body count, and a parent box
    // first follows its unchecked child.
    let deep = "* Tasks [/]\n  - [x] top\n    - [ ] inner";
    assert_eq!(
        update_statistics(deep).lines().next(),
        Some("* Tasks [0/1]")
    );
    // No checkboxes and no todos: 0/0.
    assert_eq!(update_statistics("* Empty [/]").as_str(), "* Empty [0/0]");
}

#[test]
fn cookie_data_property_chooses_what_a_headline_cookie_counts() {
    let todo = "* P [/]\n:PROPERTIES:\n:COOKIE_DATA: todo\n:END:\n- [x] box\n** TODO a\n** DONE b";
    assert_eq!(update_statistics(todo).lines().next(), Some("* P [1/2]"));
    let recursive = "* P [/]\n:PROPERTIES:\n:COOKIE_DATA: todo recursive\n:END:\n** DONE a\n*** TODO b\n*** DONE c";
    assert_eq!(
        update_statistics(recursive).lines().next(),
        Some("* P [2/3]")
    );
    let checkbox = "* P [/]\n:PROPERTIES:\n:COOKIE_DATA: checkbox\n:END:\n- [x] box\n** TODO a";
    assert_eq!(
        update_statistics(checkbox).lines().next(),
        Some("* P [1/1]")
    );
    let other = "* P [/]\n:PROPERTIES:\n:COOKIE_DATA: whatever\n:END:\n- [x] box";
    assert_eq!(update_statistics(other).lines().next(), Some("* P [1/1]"));
    // Properties after :END: are not part of the drawer.
    let stray = "* P [/]\n:PROPERTIES:\n:END:\n:COOKIE_DATA: todo\n- [x] box";
    assert_eq!(update_statistics(stray).lines().next(), Some("* P [1/1]"));
}

#[test]
fn subtrees_move_past_siblings_and_report_the_new_line() {
    let text = "* A\na body\n* B\n** B1\n* C";
    let (down, line) = move_subtree_down(text, 0).unwrap();
    assert_eq!(down, "* B\n** B1\n* A\na body\n* C");
    assert_eq!(line, 2);
    let (up, line) = move_subtree_up(text, 2).unwrap();
    assert_eq!(up, "* B\n** B1\n* A\na body\n* C");
    assert_eq!(line, 0);
    // The last sibling cannot move down, the first cannot move up.
    assert_eq!(move_subtree_down(text, 4), None);
    assert_eq!(move_subtree_up(text, 0), None);
    // A sibling is not found across a parent boundary.
    assert_eq!(move_subtree_up("* P\n** a\n* Q\n** b", 3), None);
    assert_eq!(move_subtree_down("* P\n** a\n* Q\n** b", 1), None);
    assert_eq!(move_subtree_down("plain\n* A", 0), None);
}

// ----- Export ----------------------------------------------------------------

#[test]
fn markdown_export_maps_title_author_blocks_and_inline_markup() {
    let org = "#+TITLE: Doc\n#+author: Ana\n#+BEGIN_SRC rust\nlet x = 1;\n#+end_src\n** Sub *bold* /it/ ~c~ =v= +gone+\nsee [[https://e.test][site]]";
    let md = to_markdown(org);
    assert_eq!(
        md,
        "# Doc\n*Ana*\nlet x = 1;\n## Sub **bold** *it* `c` `v` ~~gone~~\nsee [site](https://e.test)"
    );
    assert_eq!(
        to_markdown("go to [[mailto:a@b.test]] now"),
        "go to <mailto:a@b.test> now"
    );
    assert_eq!(to_markdown("#+AUTHOR: Bo"), "*Bo*");
    assert_eq!(to_markdown("#+title: T"), "# T");
}

#[test]
fn html_export_builds_a_document_with_lists_and_a_title() {
    let org =
        "#+TITLE: A & B\n#+OPTIONS: toc:nil\n* Head\ntext <here>\n- one\n+ two\n\nafter\n** Deep";
    let html = to_html(org);
    assert!(html.contains("<title>A &amp; B</title>"), "{html}");
    assert!(html.contains("<h1>A &amp; B</h1>"));
    assert!(!html.contains("OPTIONS"));
    assert!(html.contains("<h1>Head</h1>"));
    assert!(html.contains("<p>text &lt;here&gt;</p>"));
    assert!(html.contains("<ul>\n<li>one</li>\n<li>two</li>\n</ul>"));
    assert!(html.contains("<p>after</p>"));
    assert!(html.contains("<h2>Deep</h2>"));
    assert!(to_html("plain").contains("<title>Org</title>"));
    // Headings deeper than six stay h6; a list left open is closed at the end.
    let deep = to_html("******* seven\n- open");
    assert!(deep.contains("<h6>seven</h6>"), "{deep}");
    assert!(deep.contains("</ul>"));
}

#[test]
fn html_links_are_scheme_checked_and_emphasis_converts() {
    let html = to_html("[[javascript:alert(1)][bad]]");
    assert!(html.contains("href=\"#\""), "{html}");
    let html = to_html("see [[mailto:a@b.test]] now");
    assert!(
        html.contains("<a href=\"mailto:a@b.test\">mailto:a@b.test</a>"),
        "{html}"
    );
    for (org, want) in [
        ("*b*", "<b>b</b>"),
        ("/i/", "<i>i</i>"),
        ("_u_", "<u>u</u>"),
        ("~c~", "<code>c</code>"),
        ("=v=", "<code>v</code>"),
        ("+d+", "<del>d</del>"),
    ] {
        let html = to_html(org);
        assert!(html.contains(want), "{org}: {html}");
    }
    assert_eq!(safe_href("data:text/html,x"), "#");
    assert_eq!(safe_href("relative/path.html"), "relative/path.html");
    assert_eq!(safe_href("/abs"), "/abs");
    assert_eq!(safe_href("file:///x"), "file:///x");
}

#[test]
fn latex_export_has_title_author_sections_lists_and_verbatim() {
    let org = "#+TITLE: Paper_1\n#+AUTHOR: A & B\n* One\n** Two\n*** Three\n**** Four\n***** Five\ntext 50% of {x} #1 $y ~z ^w \\n\n- item\n+ more\n\n#+begin_src c\nint x = 1; // not escaped_\n#+end_src\n#+OPTIONS: x";
    let tex = to_latex(org);
    assert!(tex.contains("\\title{Paper\\_1}"), "{tex}");
    assert!(tex.contains("\\author{A \\& B}"));
    assert!(tex.contains("\\begin{document}\n\\maketitle"));
    for (stars, cmd) in [
        ("One", "\\section"),
        ("Two", "\\subsection"),
        ("Three", "\\subsubsection"),
        ("Four", "\\paragraph"),
        ("Five", "\\subparagraph"),
    ] {
        assert!(tex.contains(&format!("{cmd}{{{stars}}}")), "{cmd}: {tex}");
    }
    assert!(tex.contains("text 50\\% of \\{x\\} \\#1 \\$y \\~{}z \\^{}w \\textbackslash{}n"));
    assert!(tex.contains("\\begin{itemize}\n\\item item\n\\item more\n\\end{itemize}"));
    assert!(tex.contains("\\begin{verbatim}\nint x = 1; // not escaped_\n\\end{verbatim}"));
    assert!(!tex.contains("OPTIONS"));
    assert!(tex.ends_with("\\end{document}\n"));
}

#[test]
fn latex_without_title_has_no_maketitle_and_closes_open_environments() {
    let tex = to_latex("- open item\n#+begin_example\nunterminated");
    assert!(!tex.contains("\\maketitle"));
    assert!(!tex.contains("\\title"));
    assert!(tex.contains("\\end{itemize}\n\\begin{verbatim}"), "{tex}");
    assert!(tex.contains("unterminated\n\\end{verbatim}"));
    let link = to_latex("[[mailto:a@b.test][site *bold*]]");
    assert!(
        link.contains("\\href{mailto:a@b.test}{site \\textbf{bold}}"),
        "{link}"
    );
    let lower = to_latex("#+title: T\n#+author: Me");
    assert!(lower.contains("\\title{T}") && lower.contains("\\author{Me}"));
}

#[test]
fn ics_export_emits_one_all_day_event_per_dated_item() {
    let org = "* TODO Ship, now; ok\nDEADLINE: <2026-08-05 Wed>\n* Party\nSCHEDULED: <2026-09-01 Tue>\n* Undated";
    let ics = to_ics(org, "my;plan.org", "20260101T000000Z");
    assert!(ics.starts_with("BEGIN:VCALENDAR\r\nVERSION:2.0\r\n"));
    assert!(ics.ends_with("END:VCALENDAR\r\n"));
    assert_eq!(ics.matches("BEGIN:VEVENT").count(), 2);
    assert!(ics.contains("DTSTART;VALUE=DATE:20260805"), "{ics}");
    assert!(ics.contains("DTSTAMP:20260101T000000Z"));
    assert!(
        ics.contains("SUMMARY:DEADLINE: TODO Ship\\, now\\; ok")
            || ics.contains("SUMMARY:DEADLINE: Ship\\, now\\; ok"),
        "{ics}"
    );
    assert!(ics.contains("SUMMARY:Party"));
    assert!(ics.contains("@vix") && ics.contains("my\\;plan.org"));
    assert!(!ics.contains("Undated"));
}

// ----- Dates, links, footnotes, folds ------------------------------------------

#[test]
fn shifting_a_dated_line_rewrites_the_weekday_for_every_day_of_the_week() {
    // 2026-01-01 is a Thursday.
    let names = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    for (delta, name) in names.iter().enumerate() {
        let delta = i64::try_from(delta).unwrap();
        let (out, _) = shift_timestamp_at("<2026-01-01 Thu>", 3, delta).unwrap();
        let day = 1 + delta;
        assert_eq!(out, format!("<2026-01-{day:02} {name}>"));
    }
    // Before 1970 the weekday still follows the date.
    let (out, _) = shift_timestamp_at("<1969-12-31 Wed>", 3, 1).unwrap();
    assert_eq!(out, "<1970-01-01 Thu>");
    // A date without a weekday stays without one.
    let (out, _) = shift_timestamp_at("due 2026-01-31", 6, 1).unwrap();
    assert_eq!(out, "due 2026-02-01");
}

#[test]
fn shifting_needs_one_valid_date_near_the_cursor() {
    // Two dates and the cursor on neither: ambiguous.
    assert_eq!(shift_timestamp_at("2026-01-01 ; 2026-02-02", 12, 1), None);
    // The cursor chooses between two dates.
    let (out, _) = shift_timestamp_at("2026-01-01 ; 2026-02-02", 20, 1).unwrap();
    assert_eq!(out, "2026-01-01 ; 2026-02-03");
    // No date, and an impossible one, give nothing.
    assert_eq!(shift_timestamp_at("no date here", 0, 1), None);
    assert_eq!(shift_timestamp_at("<2026-13-45>", 3, 1), None);
    // A cursor past the end of the text names no line.
    assert_eq!(shift_timestamp_at("x", 99, 1), None);
}

#[test]
fn line_of_char_clamps_a_cursor_beyond_the_text() {
    assert_eq!(line_of_char("ab\ncd", 0), (0, 0));
    assert_eq!(line_of_char("ab\ncd", 4), (1, 3));
    let (line, _) = line_of_char("ab\ncd", 99);
    assert_eq!(line, 1);
}

#[test]
fn links_are_found_under_the_cursor_and_navigated_in_both_directions() {
    let text = "go [[https://a.test][A]] then [[file:b.org]] end";
    assert_eq!(
        link_at(text, 5),
        Some(("https://a.test".to_string(), Some("A".to_string())))
    );
    assert_eq!(link_at(text, 35), Some(("file:b.org".to_string(), None)));
    assert_eq!(link_at(text, 0), None);
    let first = link_pos(text, 0, true).unwrap();
    let second = link_pos(text, first, true).unwrap();
    assert_eq!(first, 3);
    assert!(second > first);
    assert_eq!(link_pos(text, second, true), None);
    assert_eq!(link_pos(text, second, false), Some(first));
    assert_eq!(link_pos(text, first, false), None);
}

#[test]
fn footnotes_jump_between_reference_and_definition_and_create_both() {
    // On a reference whose definition exists: jump to it without editing.
    let text = "Text[fn:1] here\n\n* Footnotes\n\n[fn:1] The note";
    let (same, cursor) = footnote(text, 5);
    assert_eq!(same, text);
    let def_start = text.chars().count() - "[fn:1] The note".chars().count();
    assert_eq!(cursor, def_start);
    // On the definition: jump back to the reference.
    let (same, back) = footnote(text, def_start + 2);
    assert_eq!(same, text);
    assert_eq!(back, 4);
    // On a reference with no definition: one is created under * Footnotes.
    let (new, pos) = footnote("See[fn:x] now", 5);
    assert_eq!(new, "See[fn:x] now\n\n* Footnotes\n\n[fn:x] ");
    assert_eq!(pos, new.chars().count());
    // Elsewhere: a numbered reference is inserted and its definition appended.
    let (new, _) = footnote("Hello", 5);
    assert_eq!(new, "Hello[fn:1]\n\n* Footnotes\n\n[fn:1] ");
    let (new, _) = footnote("A[fn:3] B", 8);
    assert!(new.contains("B[fn:4]") || new.contains("[fn:4]"), "{new}");
    assert!(new.ends_with("[fn:4] "));
    // An existing Footnotes section is reused.
    let (new, _) = footnote("x\n\n* Footnotes\n\n[fn:1] old", 1);
    assert_eq!(new.matches("* Footnotes").count(), 1);
    assert!(new.ends_with("[fn:2] "), "{new}");
}

#[test]
fn definition_without_another_reference_stays_put() {
    let text = "intro\n[fn:1] only definition";
    let (same, cursor) = footnote(text, 8);
    assert_eq!(same, text);
    assert_eq!(cursor, 8);
}

#[test]
fn source_block_body_replacement_requires_the_opening_line() {
    let text = "intro\n#+begin_src rust\nold\n#+end_src\nrest";
    assert_eq!(
        replace_src_body(text, 1, "new line\nsecond\n").as_deref(),
        Some("intro\n#+begin_src rust\nnew line\nsecond\n#+end_src\nrest")
    );
    // An empty body empties the block.
    assert_eq!(
        replace_src_body(text, 1, "").as_deref(),
        Some("intro\n#+begin_src rust\n#+end_src\nrest")
    );
    // A line that does not open a block, or an unterminated block, is refused.
    assert_eq!(replace_src_body(text, 0, "x"), None);
    assert_eq!(replace_src_body("#+begin_src c\nno end", 0, "x"), None);
    assert_eq!(replace_src_body(text, 9, "x"), None);
}

#[test]
fn sparse_tree_folds_hide_subtrees_without_a_match() {
    let text = "* Keep\n** TODO a\n** plain\nbody\n* Drop\nbody";
    let folds = todo_tree_folds(text);
    // `Keep` has a TODO and is descended into: only the non-matching child
    // folds, then the whole `Drop` subtree.
    assert_eq!(folds, vec![(2, 3), (4, 5)]);
    assert_eq!(todo_tree_folds("* only heading"), []);
}

// ----- Agenda -------------------------------------------------------------------

fn doc(text: &str) -> Vec<(String, String)> {
    vec![("f.org".to_string(), text.to_string())]
}

#[test]
fn timestamps_without_a_real_date_are_not_scheduled() {
    let items = agenda_items(&doc("* Task\nSCHEDULED: <soon>\nDEADLINE: <2026-1-5>"));
    assert!(items.is_empty(), "{items:?}");
    let ok = agenda_items(&doc("* Task\nDEADLINE: <2026-01-05 Mon>"));
    assert_eq!(ok.len(), 1);
    assert_eq!(ok[0].date.as_deref(), Some("2026-01-05"));
    assert_eq!(ok[0].kind, "DEADLINE");
    assert_eq!(ok[0].line, 0);
}

#[test]
fn tag_queries_combine_required_and_excluded_tags() {
    let files = doc("* A :work:urgent:\n* B :Work:boss:\n* C :home:\n* D no tags\n* E :bad tag:");
    let names = |query: &str| -> Vec<String> {
        tags_match(&files, query)
            .into_iter()
            .map(|i| i.headline)
            .collect()
    };
    assert_eq!(names("work"), ["A :work:urgent:", "B :Work:boss:"]);
    assert_eq!(names("work-boss"), ["A :work:urgent:"]);
    assert_eq!(names("+work +urgent"), ["A :work:urgent:"]);
    assert_eq!(names("WORK+boss"), ["B :Work:boss:"]);
    assert_eq!(names("-work").len(), 1);
    // An empty query matches every validly tagged headline.
    assert_eq!(names("").len(), 3);
    assert_eq!(names("nonexistent"), Vec::<String>::new());
}

#[test]
fn clock_report_totals_per_headline_and_top_level() {
    let text = "CLOCK: [2026-01-01 Thu 09:00]--[2026-01-01 Thu 10:30] =>  1:30\n* Work\nCLOCK: [2026-01-01 Thu 11:00]--[2026-01-01 Thu 11:45] =>  0:45\nCLOCK: [2026-01-02 Fri 11:00]--[2026-01-02 Fri 11:15] =>  0:15\n* Play\nCLOCK: [2026-01-03 Sat 11:00]--[2026-01-03 Sat 12:00] =>  1:00";
    let report = time_report(text);
    assert!(report.contains("| (top level) | 1:30 |"), "{report}");
    assert!(report.contains("| Work | 1:00 |"), "{report}");
    assert!(report.contains("| Play | 1:00 |"), "{report}");
    assert!(report.contains("3:30"), "{report}");
}

#[test]
fn emphasis_never_reads_urls_or_generated_tags_as_markers() {
    // A bold run's closing tag has a slash; italics used to pair it with the
    // next slash and eat the tag.
    let html = to_html("*one* then *two* then /three/");
    assert!(
        html.contains("<b>one</b> then <b>two</b> then <i>three</i>"),
        "{html}"
    );

    // A URL's slashes are not italic markers, in a link target or bare text.
    let html = to_html("see [[https://example.com/a/b/][the /docs/]] now");
    assert!(
        html.contains("<a href=\"https://example.com/a/b/\">the <i>docs</i></a>"),
        "{html}"
    );
    let html = to_html("visit https://example.com/a/b/c or https://example.org/x/y/z");
    assert!(!html.contains("<i>"), "{html}");
    assert!(html.contains("https://example.com/a/b/c"), "{html}");

    // Org's boundary rules: `and/or`, `a_b_c` and `2*3*4` are plain text.
    for plain in ["and/or 5/6", "snake_case_name here", "2*3*4", "a+b+c"] {
        let html = to_html(plain);
        for tag in ["<i>", "<u>", "<b>", "<del>"] {
            assert!(!html.contains(tag), "{plain}: {html}");
        }
    }
    // Verbatim text is literal: markers inside `=...=` stay.
    let html = to_html("run =a/b/c= now");
    assert!(html.contains("<code>a/b/c</code>"), "{html}");
    // Markers next to punctuation still work.
    let html = to_html("(/aside/), *bold*. \"/quoted/\"");
    assert!(
        html.contains("(<i>aside</i>),") && html.contains("<b>bold</b>."),
        "{html}"
    );
    assert!(html.contains("<i>quoted</i>"), "{html}");
}

#[test]
fn markdown_and_latex_emphasis_leave_links_and_tags_alone() {
    let md = to_markdown("see [[https://example.com/a/b/][the /docs/]] and /it/ *b* /it2/");
    assert!(
        md.contains("[the *docs*](https://example.com/a/b/)"),
        "{md}"
    );
    assert!(md.contains("*it* **b** *it2*"), "{md}");
    let md = to_markdown("bare [[https://example.com/x/y/]] and =a/b/c=");
    assert!(md.contains("<https://example.com/x/y/>"), "{md}");
    assert!(md.contains("`a/b/c`"), "{md}");

    let tex = to_latex("a [[https://example.com/a/b/][site]] then /it/ and *b* and /it2/");
    assert!(
        tex.contains(r"\href{https://example.com/a/b/}{site}"),
        "{tex}"
    );
    assert!(
        tex.contains(r"\textit{it} and \textbf{b} and \textit{it2}"),
        "{tex}"
    );
}

#[test]
fn input_cannot_forge_a_stash_placeholder() {
    // The private-use characters the exporter reserves are dropped from input.
    let html = to_html("x \u{E000}0\u{E001} *y*");
    assert!(
        !html.contains('\u{E000}') && html.contains("<b>y</b>"),
        "{html}"
    );
}

#[test]
fn shifting_a_date_leaves_the_word_after_it_alone() {
    // A short word after the date used to be taken for its weekday and replaced.
    let (out, _) = shift_timestamp_at("meet 2026-08-05 on the 5th", 8, 1).unwrap();
    assert_eq!(out, "meet 2026-08-06 on the 5th");
    let (out, _) = shift_timestamp_at("due 2026-08-05 to Bob", 6, 1).unwrap();
    assert_eq!(out, "due 2026-08-06 to Bob");
    // A word that merely starts like a weekday is prose too.
    let (out, _) = shift_timestamp_at("2026-08-05 Wedding day", 2, 1).unwrap();
    assert_eq!(out, "2026-08-06 Wedding day");
    // A real weekday is still rewritten (2026-08-05 is a Wednesday).
    let (out, _) = shift_timestamp_at("<2026-08-05 Wed> and more", 3, 1).unwrap();
    assert_eq!(out, "<2026-08-06 Thu> and more");
    let (out, _) = shift_timestamp_at("[2026-08-05 Wed]", 3, -1).unwrap();
    assert_eq!(out, "[2026-08-04 Tue]");
}
