use std::{fs, path::Path, process::Command};

use crate::process::{CheckRecord, report_check};

/// Extensions of shell scripts that must not be tracked (D27: workflow commands live in
/// `cargo xtask`).
const SCRIPT_EXTENSIONS: &[&str] = &["ps1", "psm1", "psd1", "bat", "cmd", "sh"];

/// Tracked files that spell machine-specific paths on purpose, as fixtures for path
/// redaction and for this lint. The matcher still runs on them: a path is accepted only when
/// its user name is one of `PLACEHOLDER_USERS`.
const MACHINE_PATH_FIXTURES: &[&str] = &[
    "xtask/src/tests/lint.rs",
    "xtask/src/tests/validation_redaction.rs",
];

/// The only user names a fixture file may spell in a home path.
const PLACEHOLDER_USERS: &[&str] = &["someone", "bob.smith", "José", "a_b-c", "some-one"];

/// `cargo xtask lint`: tracked-file checks that need no external tools. Prints its findings
/// and records the run like any other check.
pub(crate) fn lint(root: &Path, checks: &mut Vec<CheckRecord>) -> Result<(), String> {
    report_check(checks, "lint", lint_tracked_files(root))
}

/// Returns a summary on success, or one line per problem.
fn lint_tracked_files(root: &Path) -> Result<String, Vec<String>> {
    let files = tracked_files(root).map_err(|error| vec![error])?;
    let mut problems = Vec::new();
    let mut scanned = 0usize;
    let mut text_files = 0usize;
    let mut skipped = 0usize;
    for relative in &files {
        // A path in the index whose file is missing from the working tree (deleted, deletion
        // not yet staged) is skipped: there is nothing to read. A plain `git commit` would
        // still keep it, so stage the deletion before relying on this result.
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
        // Binary and non-UTF-8 files are not scanned for paths; the summary counts them.
        let Some(text) = std::str::from_utf8(&bytes)
            .ok()
            .filter(|_| !bytes.contains(&0))
        else {
            skipped += 1;
            continue;
        };
        text_files += 1;
        problems.extend(machine_path_problems(relative, text));
    }
    if problems.is_empty() {
        Ok(format!(
            "Lint passed: {scanned} tracked files have no shell scripts; {text_files} text files have no machine-specific paths ({skipped} binary or non-UTF-8 files were skipped, not scanned).\n"
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

/// One `file:line` message per machine-specific path in `text`. In a fixture file a path is
/// accepted when its user name is a placeholder; everywhere else every path is a leak.
pub(crate) fn machine_path_problems(path: &str, text: &str) -> Vec<String> {
    let fixture = MACHINE_PATH_FIXTURES.contains(&path);
    let mut problems = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let mut from = 0;
        while let Some((start, end)) = find_machine_span(line, from) {
            from = end;
            let found = &line[start..end];
            if fixture && is_placeholder_user(user_name(found)) {
                continue;
            }
            let hint = if fixture {
                format!(
                    "; fixtures may only use the user names {}",
                    PLACEHOLDER_USERS.join(", ")
                )
            } else {
                String::new()
            };
            problems.push(format!(
                "{path}:{}: machine-specific path `{found}`; use `<workspace>`, `~` or a relative path{hint}",
                index + 1
            ));
        }
    }
    problems
}

/// Placeholder names are matched ignoring case, because the redaction tests also check
/// case-insensitive spellings such as `SOMEONE`.
fn is_placeholder_user(name: &str) -> bool {
    let name = name.to_lowercase();
    PLACEHOLDER_USERS
        .iter()
        .any(|placeholder| placeholder.to_lowercase() == name)
}

/// The user name of a matched path: its last non-empty component.
fn user_name(found: &str) -> &str {
    found
        .rsplit(['/', '\\'])
        .find(|part| !part.is_empty())
        .unwrap_or(found)
}

/// The first absolute user-home path in `line`: `X:\Users\<name>` or `X:/Users/<name>` in any
/// slash style (doubled backslashes included), `/home/<name>/`, `/Users/<name>/`, the WSL
/// spelling `/mnt/<letter>/Users/<name>`, and a WSL network path such as
/// `\\wsl.localhost\<distro>\home\<name>` (also `\\wsl$\...`).
/// Placeholders such as `<name>`, `%USERNAME%`, `~` and `<workspace>` do not match.
#[cfg(test)]
pub(crate) fn find_machine_path(line: &str) -> Option<&str> {
    find_machine_span(line, 0).map(|(start, end)| &line[start..end])
}

/// Byte range of the first home path that starts at or after `from`.
fn find_machine_span(line: &str, from: usize) -> Option<(usize, usize)> {
    (from..line.len()).find_map(|start| home_path_end(line, start).map(|end| (start, end)))
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
        if !starts_with_ignore_case(bytes, users, b"users") {
            return None;
        }
        let name = skip_separators(bytes, users + 5)?;
        return name_end(line, name);
    }
    if matches!(bytes[start], b'/' | b'\\') {
        // The root of an absolute path, not a segment of a relative one (`docs/home/x/`).
        if before.is_some_and(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | '~')) {
            return None;
        }
        if let Some(end) = wsl_network_home_end(line, start) {
            return Some(end);
        }
    }
    if bytes[start] == b'/' {
        if let Some(end) = wsl_mount_home_end(line, start) {
            return Some(end);
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

/// `/mnt/<letter>/Users/<name>`: a Windows home seen from WSL.
fn wsl_mount_home_end(line: &str, start: usize) -> Option<usize> {
    let bytes = line.as_bytes();
    if !line[start..].starts_with("/mnt/") {
        return None;
    }
    let drive = start + 5;
    if !bytes.get(drive)?.is_ascii_alphabetic() || bytes.get(drive + 1) != Some(&b'/') {
        return None;
    }
    let users = skip_separators(bytes, drive + 1)?;
    if !starts_with_ignore_case(bytes, users, b"users") {
        return None;
    }
    let name = skip_separators(bytes, users + 5)?;
    name_end(line, name)
}

/// `\\wsl.localhost\<distro>\home\<name>` or `\\wsl$\<distro>\home\<name>`, in any slash style.
fn wsl_network_home_end(line: &str, start: usize) -> Option<usize> {
    let bytes = line.as_bytes();
    let host = skip_separators(bytes, start)?;
    let host_end = [&b"wsl.localhost"[..], b"wsl$"]
        .iter()
        .find(|host_name| starts_with_ignore_case(bytes, host, host_name))
        .map(|host_name| host + host_name.len())?;
    let distro = skip_separators(bytes, host_end)?;
    let distro_end = distro
        + line[distro..]
            .find(|c: char| matches!(c, '/' | '\\') || c.is_whitespace() || "\"'`)".contains(c))
            .unwrap_or(line.len() - distro);
    if distro_end == distro {
        return None;
    }
    let home = skip_separators(bytes, distro_end)?;
    if !starts_with_ignore_case(bytes, home, b"home") {
        return None;
    }
    let name = skip_separators(bytes, home + 4)?;
    name_end(line, name)
}

fn starts_with_ignore_case(bytes: &[u8], at: usize, prefix: &[u8]) -> bool {
    bytes
        .get(at..at + prefix.len())
        .is_some_and(|slice| slice.eq_ignore_ascii_case(prefix))
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
