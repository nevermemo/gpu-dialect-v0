use std::fs;

use super::{Expectation, UI_DIR, cargo_args, discover_cases, judge, parse_expected};
use crate::workspace::workspace_root;

const MESSAGE: &str = "`Outside` is not a GUST GPU POD type";

fn fails(substrings: &[&str]) -> Expectation {
    Expectation::Fails(substrings.iter().map(|s| (*s).to_owned()).collect())
}

const CANNED_STDERR: &str = "gpupod-missing\\src\\main.rs:13:62: error[E0277]: `Outside` is not a GUST GPU POD type\nerror: could not compile `gpupod-missing` (bin \"gpupod-missing\") due to 1 previous error\n";

#[test]
fn a_failing_crate_passes_when_every_expected_substring_is_present() {
    let wanted = fails(&[MESSAGE, "gpupod-missing/src/main.rs:13:"]);
    // Backslashes in the rustc output (Windows) match expected paths written with slashes.
    assert_eq!(
        judge("gpupod-missing", &wanted, false, CANNED_STDERR),
        Vec::<String>::new()
    );
}

#[test]
fn a_missing_substring_is_named_and_the_output_is_shown() {
    let wanted = fails(&[MESSAGE, "GUST-E0200", "gpupod-missing/src/main.rs:13:"]);
    let problems = judge("gpupod-missing", &wanted, false, CANNED_STDERR);
    assert_eq!(problems.len(), 2, "{problems:#?}");
    assert!(
        problems[0].contains("does not contain `GUST-E0200`"),
        "{problems:?}"
    );
    assert!(problems[1].contains("rustc output was:"), "{problems:?}");
    assert!(
        problems[1].contains("    gpupod-missing/src/main.rs:13:62"),
        "{problems:?}"
    );
    // Every missing substring is reported, not only the first.
    let both = judge("c", &fails(&["one", "two"]), false, "nothing relevant\n");
    assert_eq!(both.len(), 3, "{both:#?}");
}

#[test]
fn a_crate_that_compiles_when_it_must_fail_is_a_failure() {
    // Exit status zero is a failure even when the output happens to contain the text.
    let problems = judge("c", &fails(&[MESSAGE]), true, CANNED_STDERR);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(
        problems[0].contains("must fail, but it compiled"),
        "{problems:?}"
    );
}

#[test]
fn a_crate_without_expected_txt_must_compile() {
    assert!(judge("c", &Expectation::Compiles, true, "").is_empty());
    let problems = judge("c", &Expectation::Compiles, false, CANNED_STDERR);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(
        problems[0].contains("must compile, but `cargo check` failed"),
        "{problems:?}"
    );
    assert!(problems[0].contains("GUST GPU POD"), "{problems:?}");
}

#[test]
fn expected_txt_skips_blank_and_comment_lines_and_refuses_an_empty_list() {
    let text = "# the attribute message\n\n`Outside` is not a GUST GPU POD type\n  # indented comment\nsrc/main.rs:13:\n";
    assert_eq!(
        parse_expected(text),
        Ok(vec![
            "`Outside` is not a GUST GPU POD type".to_owned(),
            "src/main.rs:13:".to_owned()
        ])
    );
    // A file that lists nothing would accept any failure, so it is an error.
    for empty in ["", "\n\n", "# only a comment\n"] {
        let error = parse_expected(empty).unwrap_err();
        assert!(error.contains("lists no substring"), "{error}");
    }
}

#[test]
fn the_cargo_call_is_locked_per_crate_and_colorless() {
    let args = cargo_args("gpupod-missing");
    assert_eq!(args[0], "check");
    assert!(args.iter().any(|arg| arg == "--locked"), "{args:?}");
    let pair = |key: &str, value: &str| args.windows(2).any(|w| w[0] == key && w[1] == value);
    assert!(pair("--manifest-path", "tests/ui/Cargo.toml"), "{args:?}");
    assert!(pair("-p", "gpupod-missing"), "{args:?}");
    assert!(pair("--color", "never"), "{args:?}");
    assert!(pair("--message-format", "short"), "{args:?}");
}

#[test]
fn discovery_reads_each_crate_directory_and_its_expectation() {
    let root = std::env::temp_dir().join(format!("gust-ui-discovery-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    for name in ["b-fails", "a-compiles"] {
        fs::create_dir_all(root.join(name)).unwrap();
        fs::write(root.join(name).join("Cargo.toml"), "[package]\n").unwrap();
    }
    // Directories without a manifest, and plain files, are not cases.
    fs::create_dir_all(root.join("target")).unwrap();
    fs::write(root.join("Cargo.toml"), "[workspace]\n").unwrap();
    fs::write(root.join("b-fails").join("expected.txt"), "message\n").unwrap();
    let cases = discover_cases(&root).unwrap();
    let _ = fs::remove_dir_all(&root);
    assert_eq!(
        cases
            .iter()
            .map(|case| (case.name.as_str(), case.expectation.clone()))
            .collect::<Vec<_>>(),
        [
            ("a-compiles", Expectation::Compiles),
            ("b-fails", fails(&["message"]))
        ]
    );
}

#[test]
fn a_fixture_directory_with_no_crate_is_an_error_not_a_pass() {
    let root = std::env::temp_dir().join(format!("gust-ui-empty-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let error = discover_cases(&root).unwrap_err();
    let _ = fs::remove_dir_all(&root);
    assert!(error.contains("no fixture crate"), "{error}");
    assert!(
        discover_cases(&root).is_err(),
        "a missing directory is an error too"
    );
}

#[test]
fn an_empty_expected_txt_is_an_error_in_discovery() {
    let root = std::env::temp_dir().join(format!("gust-ui-blank-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("c")).unwrap();
    fs::write(root.join("c").join("Cargo.toml"), "[package]\n").unwrap();
    fs::write(root.join("c").join("expected.txt"), "# nothing\n").unwrap();
    let error = discover_cases(&root).unwrap_err();
    let _ = fs::remove_dir_all(&root);
    assert!(error.contains("lists no substring"), "{error}");
}

#[test]
fn the_repository_fixtures_are_a_workspace_of_their_own_and_every_crate_is_listed() {
    let root = workspace_root().unwrap();
    let ui = root.join(UI_DIR);
    let manifest = fs::read_to_string(ui.join("Cargo.toml")).unwrap();
    // A root of its own, so it can sit inside the repository without joining its workspace.
    assert!(
        manifest.lines().any(|line| line.trim() == "[workspace]"),
        "{manifest}"
    );
    let cases = discover_cases(&ui).unwrap();
    for case in &cases {
        assert!(
            manifest.contains(&format!("\"{}\"", case.name)),
            "{} is not a member of tests/ui/Cargo.toml",
            case.name
        );
    }
    // The root workspace neither lists nor includes the fixtures: no member under tests/.
    let root_manifest = fs::read_to_string(root.join("Cargo.toml")).unwrap();
    assert!(!root_manifest.contains("tests/ui"), "{root_manifest}");
    // The two committed cases: one that must fail with the attribute message, one that compiles.
    let find = |name: &str| cases.iter().find(|case| case.name == name).unwrap();
    assert!(matches!(
        &find("gpupod-missing").expectation,
        Expectation::Fails(wanted) if wanted.iter().any(|w| w.contains("is not a GUST GPU POD type"))
    ));
    assert_eq!(find("gpupod-present").expectation, Expectation::Compiles);
    // Each fixture crate has a lockfile through the shared one at the fixture root.
    assert!(ui.join("Cargo.lock").is_file());
}
