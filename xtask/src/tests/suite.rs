use super::{Step, Suite};
use crate::{
    checks::REFLECTION_TESTS,
    verify::{RELEASE_SLANG_TESTS, VerifyMode, cargo_calls as mode_cargo_calls, example_run_args},
    workspace::EXAMPLES,
};

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

/// The cargo calls a step makes, as argument lists. Steps that call cargo through another
/// xtask command are listed by that command's data, so that no call hides behind a step.
/// The Full suite is not expanded: it runs `cargo fmt` (which has no `--locked`) and the
/// examples' `--ignored` tests, which are not locked, so it is not checked here.
fn cargo_calls(step: Step) -> Vec<Vec<String>> {
    let owned = |args: &[&str]| args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
    match step {
        Step::Cargo(args) => vec![owned(args)],
        Step::Feature("reflection") => REFLECTION_TESTS.iter().map(|args| owned(args)).collect(),
        Step::Verify(VerifyMode::Artifacts, _) => EXAMPLES
            .iter()
            .map(|example| owned(&example_run_args(example)))
            .collect(),
        Step::Verify(VerifyMode::Full, _)
        | Step::RequireTool(_)
        | Step::Lint
        | Step::DocsCheck
        | Step::CheckArch => Vec::new(),
        other => panic!("{other:?}: list the cargo calls of this step in cargo_calls"),
    }
}

#[test]
fn every_cargo_step_is_locked() {
    let mut seen = Vec::new();
    for suite in Suite::names().into_iter().map(|n| Suite::parse(n).unwrap()) {
        for step in suite.steps() {
            for args in cargo_calls(step) {
                assert!(
                    args.iter().any(|arg| arg == "--locked"),
                    "{suite:?}: cargo {args:?}"
                );
                seen.push((suite, args));
            }
        }
    }
    // The nested calls of Reflect and Spirv are really among the calls checked.
    let count = |wanted: Suite| seen.iter().filter(|(suite, _)| *suite == wanted).count();
    assert_eq!(count(Suite::Reflect), 2);
    assert_eq!(count(Suite::Spirv), EXAMPLES.len());
    assert!(count(Suite::Unit) >= 2 && count(Suite::Gpu) >= 1);
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

#[test]
fn the_full_gate_runs_the_release_slang_tests() {
    // The release-only fallback tests (`cfg(not(debug_assertions))`) run nowhere else.
    assert_eq!(
        RELEASE_SLANG_TESTS,
        [
            "test",
            "--locked",
            "--release",
            "-p",
            "gust",
            "--lib",
            "slang"
        ]
    );
    assert!(Suite::Full.covers().contains("release-profile"));
    // Suite::Full and check-full both run verify's Full mode (above and in main.rs); the
    // structured list of that mode's cargo calls is tested in tests/verify_steps.rs.
    let release = RELEASE_SLANG_TESTS.map(str::to_owned).to_vec();
    assert_eq!(
        mode_cargo_calls(VerifyMode::Full)
            .iter()
            .filter(|call| **call == release)
            .count(),
        1
    );
}
