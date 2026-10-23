use super::{Step, Suite};
use crate::verify::VerifyMode;

const UNIT: [Step; 5] = [
    Step::Cargo(&["test", "--locked", "-p", "gust-macros"]),
    Step::Cargo(&["test", "--locked", "-p", "xtask"]),
    Step::Lint,
    Step::DocsCheck,
    Step::CheckArch,
];

#[test]
fn names_parse_case_insensitively_and_unknown_names_list_the_choices() {
    for (text, suite) in [
        ("Unit", Suite::Unit),
        ("slangc", Suite::Slangc),
        ("SPIRV", Suite::Spirv),
        ("Reflect", Suite::Reflect),
        ("gpu", Suite::Gpu),
        ("Full", Suite::Full),
    ] {
        assert_eq!(Suite::parse(text), Ok(suite), "{text}");
        assert!(Suite::names().contains(&suite.name()));
    }
    let error = Suite::parse("Smoke").unwrap_err();
    assert!(
        error.contains("Unit|Slangc|Spirv|Reflect|Gpu|Full"),
        "{error}"
    );
    assert!(Suite::parse("").is_err());
}

#[test]
fn unit_runs_tests_lint_docs_and_architecture_without_external_tools() {
    assert_eq!(Suite::Unit.steps(), UNIT.to_vec());
}

#[test]
fn slangc_is_unit_plus_the_core_library_tests_and_requires_slangc() {
    let mut expected = vec![Step::RequireTool("slangc")];
    expected.extend(UNIT);
    expected.push(Step::Cargo(&["test", "--locked", "-p", "gust", "--lib"]));
    assert_eq!(Suite::Slangc.steps(), expected);
}

#[test]
fn the_other_tiers_alias_existing_commands() {
    assert_eq!(
        Suite::Spirv.steps(),
        vec![Step::Verify(VerifyMode::Artifacts, false)]
    );
    assert_eq!(Suite::Reflect.steps(), vec![Step::Feature("reflection")]);
    assert_eq!(
        Suite::Gpu.steps(),
        vec![
            Step::RequireTool("slangc"),
            Step::Cargo(&["test", "--locked", "-p", "gust-wgpu"])
        ]
    );
    assert_eq!(
        Suite::Full.steps(),
        vec![Step::Verify(VerifyMode::Full, true)]
    );
}

#[test]
fn every_cargo_test_step_is_locked() {
    for suite in Suite::names().into_iter().map(|n| Suite::parse(n).unwrap()) {
        for step in suite.steps() {
            if let Step::Cargo(args) = step {
                assert!(args.contains(&"--locked"), "{suite:?}: {args:?}");
            }
        }
    }
}

#[test]
fn every_suite_says_what_it_does_and_does_not_cover() {
    for name in Suite::names() {
        let suite = Suite::parse(name).unwrap();
        assert!(
            !suite.covers().is_empty() && !suite.not_covered().is_empty(),
            "{name}"
        );
        assert!(!suite.steps().is_empty(), "{name}");
    }
    // The Unit tier must not claim anything that needs a tool.
    assert!(Suite::Unit.not_covered().contains("Slang compiles"));
    assert!(
        Suite::Unit
            .steps()
            .iter()
            .all(|s| !matches!(s, Step::RequireTool(_)))
    );
}
