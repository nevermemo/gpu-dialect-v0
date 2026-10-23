use std::{
    env,
    ffi::OsString,
    fs,
    path::Path,
    process::{Command, Stdio},
};

use crate::process::{CheckRecord, report_check};

/// The fixture workspace, relative to the repository root. It has a `[workspace]` of its own
/// and is not a member of the root workspace, so `cargo test --workspace` never builds it.
pub(crate) const UI_DIR: &str = "tests/ui";

/// What `cargo check` of one fixture crate must do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Expectation {
    /// No `expected.txt`: the crate must compile.
    Compiles,
    /// `expected.txt` lists substrings: the crate must fail and its output must contain them all.
    Fails(Vec<String>),
}

/// One fixture crate under `tests/ui/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Case {
    pub(crate) name: String,
    pub(crate) expectation: Expectation,
}

/// `cargo xtask check-ui`: `cargo check` of each fixture crate under `tests/ui/`, compared with
/// its expectation. Needs cargo only; no slangc and no GPU. Also part of the Unit suite.
pub(crate) fn check_ui(root: &Path, checks: &mut Vec<CheckRecord>) -> Result<(), String> {
    let outcome = run_cases(root, checks);
    report_check(checks, "check-ui", outcome)
}

/// Returns a summary on success, or one line per problem.
fn run_cases(root: &Path, checks: &mut Vec<CheckRecord>) -> Result<String, Vec<String>> {
    let cases = discover_cases(&root.join(UI_DIR)).map_err(|error| vec![error])?;
    let mut problems = Vec::new();
    for case in &cases {
        let args = cargo_args(&case.name);
        let output = Command::new(env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo")))
            .args(&args)
            .current_dir(root)
            .stdin(Stdio::null())
            .output();
        let output = match output {
            Ok(output) => output,
            Err(error) => {
                problems.push(format!("{}: could not launch cargo: {error}", case.name));
                continue;
            }
        };
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        problems.extend(judge(
            &case.name,
            &case.expectation,
            output.status.success(),
            &stderr,
        ));
        checks.push(CheckRecord {
            program: "cargo".to_owned(),
            arguments: args,
            exit_code: output.status.code(),
            stdout,
            stderr,
        });
    }
    if !problems.is_empty() {
        return Err(problems);
    }
    let failing = cases
        .iter()
        .filter(|case| matches!(case.expectation, Expectation::Fails(_)))
        .count();
    Ok(format!(
        "UI check passed: {failing} fixture crate(s) failed to compile with the expected rustc output and {} compiled cleanly (tests/ui, `cargo check --locked`). Only the substrings of each expected.txt are compared, not the whole rustc output; no slangc and no GPU were involved.\n",
        cases.len() - failing
    ))
}

/// The arguments after `cargo` that check one fixture crate. `--locked` keeps the fixture's own
/// `Cargo.lock` honest, and `--color never` keeps ANSI codes out of the compared text.
pub(crate) fn cargo_args(case: &str) -> Vec<String> {
    [
        "check",
        "--locked",
        "--color",
        "never",
        "--manifest-path",
        "tests/ui/Cargo.toml",
        "-p",
        case,
        "--message-format",
        "short",
    ]
    .iter()
    .map(|arg| (*arg).to_owned())
    .collect()
}

/// The fixture crates of `ui_root`, sorted by name: every subdirectory with a `Cargo.toml`.
pub(crate) fn discover_cases(ui_root: &Path) -> Result<Vec<Case>, String> {
    let entries =
        fs::read_dir(ui_root).map_err(|error| format!("{UI_DIR}: could not be read: {error}"))?;
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| format!("{UI_DIR}: {error}"))?;
        if entry.path().join("Cargo.toml").is_file() {
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    names.sort();
    if names.is_empty() {
        return Err(format!(
            "{UI_DIR} has no fixture crate (a directory with a Cargo.toml); a check without a case would pass without checking anything"
        ));
    }
    names
        .into_iter()
        .map(|name| {
            let expected = ui_root.join(&name).join("expected.txt");
            let expectation = if expected.is_file() {
                let text = fs::read_to_string(&expected)
                    .map_err(|error| format!("{UI_DIR}/{name}/expected.txt: {error}"))?;
                Expectation::Fails(
                    parse_expected(&text).map_err(|error| format!("{UI_DIR}/{name}: {error}"))?,
                )
            } else {
                Expectation::Compiles
            };
            Ok(Case { name, expectation })
        })
        .collect()
}

/// The required substrings of an `expected.txt`: one per line, blank lines and lines starting
/// with `#` ignored. A file with no substring is an error, since it would accept any failure.
pub(crate) fn parse_expected(text: &str) -> Result<Vec<String>, String> {
    let substrings: Vec<String> = text
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
        .map(str::to_owned)
        .collect();
    if substrings.is_empty() {
        return Err(
            "expected.txt lists no substring; delete it if the crate must compile".to_owned(),
        );
    }
    Ok(substrings)
}

/// The problems of one fixture run: none when the outcome matches the expectation. Backslashes
/// in `stderr` count as slashes, so an expected path reads the same on every platform.
pub(crate) fn judge(
    case: &str,
    expectation: &Expectation,
    success: bool,
    stderr: &str,
) -> Vec<String> {
    let shown = stderr.replace('\\', "/");
    match expectation {
        Expectation::Compiles if success => Vec::new(),
        Expectation::Compiles => vec![format!(
            "{case}: has no expected.txt so it must compile, but `cargo check` failed:\n{}",
            indent(&shown)
        )],
        Expectation::Fails(_) if success => vec![format!(
            "{case}: has an expected.txt so `cargo check` must fail, but it compiled"
        )],
        Expectation::Fails(wanted) => {
            let missing: Vec<&String> = wanted
                .iter()
                .filter(|substring| !shown.contains(substring.as_str()))
                .collect();
            if missing.is_empty() {
                return Vec::new();
            }
            let mut problems: Vec<String> = missing
                .iter()
                .map(|substring| {
                    format!("{case}: rustc output does not contain `{substring}` (expected.txt)")
                })
                .collect();
            problems.push(format!("{case}: rustc output was:\n{}", indent(&shown)));
            problems
        }
    }
}

fn indent(text: &str) -> String {
    text.lines()
        .map(|line| format!("    {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
#[path = "tests/ui.rs"]
mod ui_tests;
