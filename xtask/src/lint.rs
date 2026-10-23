use std::{fs, path::Path, process::Command};

use crate::process::CheckRecord;

/// Extensions of shell scripts that must not be tracked (D27: workflow commands live in
/// `cargo xtask`).
const SCRIPT_EXTENSIONS: &[&str] = &["ps1", "psm1", "psd1", "bat", "cmd", "sh"];

/// Tracked files that spell machine-specific paths on purpose, as fixtures for path
/// redaction and for this lint. Anything else that contains one is a leak.
const MACHINE_PATH_FIXTURES: &[&str] = &[
    "xtask/src/tests/lint.rs",
    "xtask/src/tests/validation_redaction.rs",
];

/// `cargo xtask lint`: tracked-file checks that need no external tools. Prints its findings
/// and records the run like any other check.
pub(crate) fn lint(root: &Path, checks: &mut Vec<CheckRecord>) -> Result<(), String> {
    let outcome = lint_tracked_files(root);
    let (stdout, stderr, exit_code, result) = match outcome {
        Ok(summary) => (summary, String::new(), 0, Ok(())),
        Err(problems) => {
            let message = format!("lint found {} problem(s)", problems.len());
            (String::new(), problems.join("\n"), 1, Err(message))
        }
    };
    print!("{stdout}");
    if !stderr.is_empty() {
        eprintln!("{stderr}");
    }
    checks.push(CheckRecord {
        program: "cargo".to_owned(),
        arguments: vec!["xtask".to_owned(), "lint".to_owned()],
        exit_code: Some(exit_code),
        stdout,
        stderr,
    });
    result
}

/// Returns a summary on success, or one line per problem.
fn lint_tracked_files(root: &Path) -> Result<String, Vec<String>> {
    let files = tracked_files(root).map_err(|error| vec![error])?;
    let mut problems = Vec::new();
    let mut scanned = 0usize;
    let mut text_files = 0usize;
    for relative in &files {
        // An index entry whose file was deleted from the working tree is not a script that
        // the next commit keeps; only files that exist can be linted.
        let path = root.join(relative);
        if !path.is_file() {
            continue;
        }
        scanned += 1;
        if is_forbidden_script(relative) {
            problems.push(format!(
                "{relative}: tracked shell script; workflow commands live in `cargo xtask` (D27)"
            ));
        }
        let Ok(bytes) = fs::read(&path) else {
            problems.push(format!("{relative}: could not be read"));
            continue;
        };
        // Binary and non-UTF-8 files carry no readable paths worth linting.
        let Some(text) = std::str::from_utf8(&bytes)
            .ok()
            .filter(|_| !bytes.contains(&0))
        else {
            continue;
        };
        text_files += 1;
        problems.extend(machine_path_problems(relative, text));
    }
    if problems.is_empty() {
        Ok(format!(
            "Lint passed: {scanned} tracked files have no shell scripts; {text_files} text files have no machine-specific paths.\n"
        ))
    } else {
        Err(problems)
    }
}

fn tracked_files(root: &Path) -> Result<Vec<String>, String> {
    let output = Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(root)
        .output()
        .map_err(|error| format!("could not launch git: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git ls-files failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .collect())
}

/// True for a repository-relative path whose file name ends in a shell-script extension.
pub(crate) fn is_forbidden_script(path: &str) -> bool {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    name.rsplit_once('.').is_some_and(|(_, extension)| {
        SCRIPT_EXTENSIONS
            .iter()
            .any(|script| extension.eq_ignore_ascii_case(script))
    })
}

/// One `file:line` message per line of `text` that spells a machine-specific path.
pub(crate) fn machine_path_problems(path: &str, text: &str) -> Vec<String> {
    if MACHINE_PATH_FIXTURES.contains(&path) {
        return Vec::new();
    }
    text.lines()
        .enumerate()
        .filter_map(|(index, line)| {
            find_machine_path(line).map(|found| {
                format!(
                    "{path}:{}: machine-specific path `{found}`; use `<workspace>`, `~` or a relative path",
                    index + 1
                )
            })
        })
        .collect()
}

/// The first absolute user-home path in `line`: `X:\Users\<name>` or `X:/Users/<name>` in any
/// slash style (doubled backslashes included), `/home/<name>/` or `/Users/<name>/`.
/// Placeholders such as `<name>`, `%USERNAME%`, `~` and `<workspace>` do not match.
pub(crate) fn find_machine_path(line: &str) -> Option<&str> {
    (0..line.len()).find_map(|start| home_path_end(line, start).map(|end| &line[start..end]))
}

fn home_path_end(line: &str, start: usize) -> Option<usize> {
    if !line.is_char_boundary(start) {
        return None;
    }
    let bytes = line.as_bytes();
    let before = line[..start].chars().next_back();
    if bytes[start].is_ascii_alphabetic() && bytes.get(start + 1) == Some(&b':') {
        // A drive letter, not the tail of a word such as `PATH:`.
        if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
            return None;
        }
        let users = skip_separators(bytes, start + 2)?;
        if !bytes.get(users..users + 5)?.eq_ignore_ascii_case(b"users") {
            return None;
        }
        let name = skip_separators(bytes, users + 5)?;
        return name_end(line, name);
    }
    if bytes[start] == b'/' {
        // The root of an absolute path, not a segment of a relative one (`docs/home/x/`).
        if before.is_some_and(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | '~')) {
            return None;
        }
        let name = ["/home/", "/Users/"]
            .iter()
            .find(|prefix| line[start..].starts_with(*prefix))
            .map(|prefix| start + prefix.len())?;
        let end = name_end(line, name)?;
        return (bytes.get(end) == Some(&b'/')).then_some(end);
    }
    None
}

/// Index after one or more path separators starting at `from`, or `None` if there is none.
fn skip_separators(bytes: &[u8], from: usize) -> Option<usize> {
    let count = bytes
        .get(from..)?
        .iter()
        .take_while(|byte| matches!(byte, b'\\' | b'/'))
        .count();
    (count > 0).then_some(from + count)
}

/// End of the user name at `from`; it must start with a letter or digit so that
/// placeholders such as `<name>` and `%USERNAME%` are not user names.
fn name_end(line: &str, from: usize) -> Option<usize> {
    let rest = line.get(from..)?;
    if !rest.chars().next()?.is_alphanumeric() {
        return None;
    }
    let length = rest
        .char_indices()
        .find(|(_, c)| !(c.is_alphanumeric() || matches!(c, '_' | '-' | '.')))
        .map_or(rest.len(), |(index, _)| index);
    Some(from + length)
}

#[cfg(test)]
#[path = "tests/lint.rs"]
mod lint_tests;
