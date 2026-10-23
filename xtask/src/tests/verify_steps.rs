use super::{RELEASE_SLANG_TESTS, VerifyMode, cargo_calls, example_run_args};
use crate::workspace::{EXAMPLES, SMOKE_EXAMPLES, workspace_test_args};

const ALL_MODES: [VerifyMode; 6] = [
    VerifyMode::Smoke,
    VerifyMode::Fast,
    VerifyMode::Gpu,
    VerifyMode::Examples,
    VerifyMode::Artifacts,
    VerifyMode::Full,
];

fn owned(args: &[&str]) -> Vec<String> {
    args.iter().map(|arg| (*arg).to_owned()).collect()
}

const FMT: [&str; 4] = ["fmt", "--all", "--", "--check"];
const CLIPPY: [&str; 7] = [
    "clippy",
    "--locked",
    "--workspace",
    "--all-targets",
    "--",
    "-D",
    "warnings",
];

fn ignored_example_tests() -> Vec<Vec<String>> {
    EXAMPLES
        .iter()
        .map(|example| owned(&["test", "-p", example, "--", "--ignored"]))
        .collect()
}

fn example_runs(examples: &[&str]) -> Vec<Vec<String>> {
    examples
        .iter()
        .map(|example| owned(&example_run_args(example)))
        .collect()
}

#[test]
fn the_full_gate_is_fmt_clippy_workspace_tests_release_slang_tests_examples_and_runs_in_order() {
    let mut expected = vec![owned(&FMT), owned(&CLIPPY), workspace_test_args()];
    expected.push(owned(&[
        "test",
        "--locked",
        "--release",
        "-p",
        "gust",
        "--lib",
        "slang",
    ]));
    expected.extend(ignored_example_tests());
    expected.extend(example_runs(EXAMPLES));
    assert_eq!(cargo_calls(VerifyMode::Full), expected);
}

#[test]
fn only_the_full_gate_runs_the_release_slang_tests_and_exactly_once() {
    // The release-only fallback tests (`cfg(not(debug_assertions))`) run nowhere else.
    let release = owned(&RELEASE_SLANG_TESTS);
    for mode in ALL_MODES {
        let count = cargo_calls(mode)
            .iter()
            .filter(|call| **call == release)
            .count();
        assert_eq!(count, usize::from(mode == VerifyMode::Full), "{mode:?}");
    }
    // It follows the debug workspace tests and precedes the examples.
    let calls = cargo_calls(VerifyMode::Full);
    let position = |wanted: &Vec<String>| calls.iter().position(|call| call == wanted).unwrap();
    assert!(position(&workspace_test_args()) < position(&release));
    assert!(position(&release) < position(&ignored_example_tests()[0]));
    assert!(position(&release) < position(&example_runs(EXAMPLES)[0]));
}

#[test]
fn the_other_modes_keep_their_cargo_calls() {
    let macros = owned(&["test", "--locked", "-p", "gust-macros"]);
    let core = owned(&["test", "--locked", "-p", "gust", "--lib"]);
    let wgpu = owned(&["test", "--locked", "-p", "gust-wgpu"]);
    assert_eq!(
        cargo_calls(VerifyMode::Fast),
        [owned(&FMT), macros, core, wgpu.clone()]
    );
    assert_eq!(cargo_calls(VerifyMode::Gpu), [wgpu]);
    let mut smoke = vec![owned(&FMT), owned(&CLIPPY), workspace_test_args()];
    smoke.extend(example_runs(SMOKE_EXAMPLES));
    assert_eq!(cargo_calls(VerifyMode::Smoke), smoke);
    let mut examples = ignored_example_tests();
    examples.extend(example_runs(EXAMPLES));
    assert_eq!(cargo_calls(VerifyMode::Examples), examples);
    assert_eq!(cargo_calls(VerifyMode::Artifacts), example_runs(EXAMPLES));
}

#[test]
fn only_fmt_and_the_ignored_example_tests_are_not_locked() {
    // docs/development/VALIDATION.md names exactly these as the unlocked cargo calls of the
    // verify modes; a new unlocked call must change both.
    for mode in ALL_MODES {
        for call in cargo_calls(mode) {
            let locked = call.iter().any(|arg| arg == "--locked");
            let exempt = call[0] == "fmt" || call.iter().any(|arg| arg == "--ignored");
            assert!(locked || exempt, "{mode:?}: cargo {call:?}");
            assert!(
                !(locked && exempt),
                "{mode:?}: cargo {call:?} is exempt but locked"
            );
        }
    }
}
