use std::collections::BTreeSet;

use super::{
    LinkCheck, check_link, conflict_marker_problems, decision_ids, is_markdown, markdown_links,
};

fn known<'a>(files: &[&'a str]) -> BTreeSet<&'a str> {
    files.iter().copied().collect()
}

fn check(from: &str, target: &str, files: &[&str]) -> LinkCheck {
    check_link(from, target, &known(files))
}

fn broken(outcome: LinkCheck) -> String {
    match outcome {
        LinkCheck::Broken(reason) => reason,
        other => panic!("expected a broken link, got {other:?}"),
    }
}

#[test]
fn relative_links_resolve_against_the_linking_file() {
    let files = ["docs/a.md", "docs/development/b.md", "README.md", "x.md"];
    assert_eq!(
        check("docs/a.md", "development/b.md", &files),
        LinkCheck::Resolved
    );
    assert_eq!(
        check("docs/a.md", "./development/b.md", &files),
        LinkCheck::Resolved
    );
    assert_eq!(
        check("docs/a.md", "../README.md", &files),
        LinkCheck::Resolved
    );
    assert_eq!(
        check("docs/development/b.md", "../../README.md#top", &files),
        LinkCheck::Resolved
    );
    assert_eq!(
        check("docs/a.md", "/README.md", &files),
        LinkCheck::Resolved
    );
    assert_eq!(
        check("x.md", "docs/a.md?plain=1", &files),
        LinkCheck::Resolved
    );
    assert_eq!(check("README.md", "docs/a.md", &files), LinkCheck::Resolved);
}

#[test]
fn a_missing_target_is_reported_with_its_resolved_path() {
    let files = ["docs/a.md", "README.md"];
    let reason = broken(check("docs/a.md", "missing.md", &files));
    assert!(reason.contains("`docs/missing.md`"), "{reason}");
    let reason = broken(check("docs/a.md", "../docs/Other.md", &files));
    assert!(reason.contains("`docs/Other.md`"), "{reason}");
    // Names are case-sensitive, like on GitHub.
    broken(check("docs/a.md", "../readme.md", &files));
    // A directory is not a file.
    broken(check("README.md", "docs", &files));
    broken(check("README.md", "docs/", &files));
}

#[test]
fn a_link_that_leaves_the_repository_is_broken() {
    let reason = broken(check("docs/a.md", "../../outside.md", &["docs/a.md"]));
    assert!(reason.contains("leaves the repository"), "{reason}");
    broken(check("a.md", "../a.md", &["a.md"]));
}

#[test]
fn anchors_external_and_empty_links_are_not_checked() {
    for target in [
        "#section",
        "",
        "https://example.com/missing.md",
        "http://example.com",
        "mailto:someone@example.com",
        "//example.com/x.md",
        "tel:123",
    ] {
        assert_eq!(
            check("a.md", target, &["a.md"]),
            LinkCheck::Skipped,
            "{target}"
        );
    }
}

#[test]
fn percent_encoded_spaces_resolve() {
    assert_eq!(
        check("a.md", "my%20notes.md", &["a.md", "my notes.md"]),
        LinkCheck::Resolved
    );
}

fn targets(text: &str) -> Vec<(usize, String)> {
    markdown_links(text)
        .into_iter()
        .map(|link| (link.line, link.target))
        .collect()
}

#[test]
fn inline_links_images_and_definitions_are_extracted() {
    let text = "See [a](docs/a.md) and ![img](pic.png \"title\").\n\
                Two: [x](<with space.md>) [y](f(1).md#h)\n\
                [ref]: ../other.md\n\
                Not a link: [text] (x.md)\n";
    assert_eq!(
        targets(text),
        vec![
            (1, "docs/a.md".to_owned()),
            (1, "pic.png".to_owned()),
            (2, "with space.md".to_owned()),
            (2, "f(1).md#h".to_owned()),
            (3, "../other.md".to_owned()),
        ]
    );
}

#[test]
fn code_blocks_and_code_spans_hold_no_links() {
    let text = "`[a](one.md)` and ``[b](two.md)`` but [c](three.md)\n\
                ```rust\n[d](four.md)\n```\n\
                ~~~\n[e](five.md)\n~~~\n\
                after [f](six.md)\n\
                an unclosed `tick [g](seven.md)\n";
    assert_eq!(
        targets(text),
        vec![
            (1, "three.md".to_owned()),
            (8, "six.md".to_owned()),
            (9, "seven.md".to_owned()),
        ]
    );
}

fn marker(kind: char, tail: &str) -> String {
    format!("{}{tail}", kind.to_string().repeat(7))
}

#[test]
fn conflict_markers_are_found_with_their_line() {
    let text = format!(
        "fine\n{}\nmine\n{}\ntheirs\n{}\n",
        marker('<', " HEAD"),
        marker('=', ""),
        marker('>', " branch")
    );
    let problems = conflict_marker_problems("src/lib.rs", &text);
    assert_eq!(problems.len(), 3, "{problems:?}");
    assert!(problems[0].starts_with("src/lib.rs:2: "), "{}", problems[0]);
    assert!(problems[1].starts_with("src/lib.rs:4: "), "{}", problems[1]);
    assert!(problems[2].starts_with("src/lib.rs:6: "), "{}", problems[2]);
    let crlf = text.replace('\n', "\r\n");
    assert_eq!(conflict_marker_problems("src/lib.rs", &crlf).len(), 3);
}

#[test]
fn lookalikes_are_not_conflict_markers() {
    let lookalikes = [
        marker('<', "x"),
        marker('>', "x"),
        format!(" {}", marker('=', "")),
        format!("{}=", marker('=', "")),
        format!("{}-", marker('<', "")),
        "<<<<<< six".to_owned(),
        "========".to_owned(),
        format!("text {}", marker('<', "")),
    ];
    for line in lookalikes {
        let text = format!("{line}\n");
        assert!(
            conflict_marker_problems("src/lib.rs", &text).is_empty(),
            "{line:?}"
        );
    }
}

#[test]
fn a_lone_separator_in_markdown_is_a_heading_underline() {
    let heading = format!("Title\n{}\nbody\n", marker('=', ""));
    assert!(conflict_marker_problems("docs/a.md", &heading).is_empty());
    // The same line in a source file is a leftover separator.
    assert_eq!(conflict_marker_problems("src/a.rs", &heading).len(), 1);
    // With another marker in the Markdown file, the separator counts too.
    let conflict = format!(
        "{}\nmine\n{}\ntheirs\n{}\n",
        marker('<', " HEAD"),
        marker('=', ""),
        marker('>', " other")
    );
    assert_eq!(conflict_marker_problems("docs/a.md", &conflict).len(), 3);
}

fn table(ids: &[&str], next: &str) -> String {
    let mut text = format!("# Decisions\n\nAppend-only log. Next id: **{next}**. More.\n\n");
    text.push_str("| ID | Decision | Rationale |\n| --- | --- | --- |\n");
    for id in ids {
        text.push_str(&format!("| {id} | something | because |\n"));
    }
    text.push_str("\n## Notes\n\n| D99 is prose, not a row\n");
    text
}

#[test]
fn a_consistent_decision_table_passes() {
    let ids = decision_ids(&table(&["D01", "D02", "D04"], "D05")).unwrap();
    assert_eq!((ids.count, ids.largest), (3, 4));
    // Reserved gaps are allowed; ids only have to increase.
    assert!(decision_ids(&table(&["D19", "D20", "D27"], "D28")).is_ok());
}

#[test]
fn a_duplicate_decision_id_is_reported() {
    let problems = decision_ids(&table(&["D01", "D02", "D02", "D03"], "D04")).unwrap_err();
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(
        problems[0].contains("duplicate decision id D02"),
        "{}",
        problems[0]
    );
}

#[test]
fn an_out_of_order_decision_id_is_reported() {
    let problems = decision_ids(&table(&["D01", "D03", "D02"], "D04")).unwrap_err();
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(
        problems[0].contains("D02 is out of order after D03"),
        "{}",
        problems[0]
    );
}

#[test]
fn the_next_id_must_follow_the_largest_id() {
    let problems = decision_ids(&table(&["D01", "D02"], "D02")).unwrap_err();
    assert!(problems[0].contains("Next id D02"), "{}", problems[0]);
    assert!(problems[0].contains("expected D03"), "{}", problems[0]);
    assert!(decision_ids(&table(&["D01", "D02"], "D04")).is_err());
}

#[test]
fn a_missing_header_or_table_is_reported() {
    let no_header = "| D01 | a | b |\n";
    let problems = decision_ids(no_header).unwrap_err();
    assert!(
        problems.iter().any(|p| p.contains("Next id")),
        "{problems:?}"
    );
    let no_rows = "Next id: **D01**\n\n| ID | Decision |\n| --- | --- |\n";
    let problems = decision_ids(no_rows).unwrap_err();
    assert!(
        problems.iter().any(|p| p.contains("no decision rows")),
        "{problems:?}"
    );
}

#[test]
fn a_duplicate_that_is_not_adjacent_is_still_a_duplicate() {
    let problems = decision_ids(&table(&["D01", "D02", "D03", "D02"], "D04")).unwrap_err();
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(
        problems[0].contains("duplicate decision id D02"),
        "{}",
        problems[0]
    );
    // D02 is out of order after D03 and D03 repeats: both are named.
    let problems = decision_ids(&table(&["D01", "D03", "D02", "D03"], "D04")).unwrap_err();
    assert_eq!(problems.len(), 2, "{problems:?}");
    assert!(
        problems[0].contains("D02 is out of order after D03"),
        "{}",
        problems[0]
    );
    assert!(
        problems[1].contains("duplicate decision id D03"),
        "{}",
        problems[1]
    );
}

#[test]
fn markdown_extensions_match_in_any_case() {
    for path in ["a.md", "README.MD", "docs/Notes.Md", "x.mD", ".md"] {
        assert!(is_markdown(path), "{path}");
    }
    for path in ["a.mdx", "md", "a.md.txt", "docs/md/a", "a.markdown", ""] {
        assert!(!is_markdown(path), "{path}");
    }
    // Conflict separators are heading underlines in `.MD` files too.
    let heading = format!("Title\n{}\nbody\n", marker('=', ""));
    assert!(conflict_marker_problems("docs/A.MD", &heading).is_empty());
    assert_eq!(conflict_marker_problems("docs/a.txt", &heading).len(), 1);
}

#[test]
fn footnote_definitions_are_not_link_targets() {
    let text = "Claim.[^1] Another.[^note]\n\
                [^1]: see docs/missing.md for details\n\
                [^note]: https://example.com\n\
                [real]: docs/a.md\n\
                [^x]: and [a link](docs/b.md) inside\n";
    assert_eq!(
        targets(text),
        vec![(4, "docs/a.md".to_owned()), (5, "docs/b.md".to_owned())]
    );
}
