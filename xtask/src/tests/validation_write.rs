use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use super::{
    ArtifactRecord, GitStamp, RunInfo, compact_validation_summary, json_string,
    validation_redaction_tests::HOME, write_validation_with_home,
};
use crate::{process::CheckRecord, verify::VerifyMode};

fn run(mode: VerifyMode, git: &GitStamp) -> RunInfo<'_> {
    RunInfo {
        mode,
        started: "2026-10-23T00:00:00Z",
        git,
    }
}

/// A fresh root under the cargo target directory (`<target>/<profile>/deps/<test binary>`).
fn temp_root() -> PathBuf {
    let base = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.ancestors().nth(3).map(Path::to_path_buf))
        .unwrap_or_else(std::env::temp_dir);
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let root = base
        .join("xtask-tests")
        .join(format!("validation-{}-{stamp}", std::process::id()));
    fs::create_dir_all(root.join(".ai")).unwrap();
    root
}

/// The spellings command output uses for a path: as given, backslashes, doubled
/// backslashes, the verbatim prefix, and forward slashes.
fn spellings(path: &str) -> Vec<String> {
    let backslash = path.replace('/', r"\");
    vec![
        path.to_owned(),
        backslash.replace('\\', r"\\"),
        format!(r"\\?\{backslash}"),
        backslash.replace('\\', "/"),
        backslash,
    ]
}

/// The text as the JSON file holds it: raw, and with JSON string escaping.
fn json_forms(text: &str) -> [String; 2] {
    let escaped = json_string(text);
    [
        text.to_ascii_lowercase(),
        escaped[1..escaped.len() - 1].to_ascii_lowercase(),
    ]
}

#[test]
fn written_record_contains_neither_the_root_nor_the_home() {
    let root = temp_root();
    let root_text = root.display().to_string();
    let leaky = |tail: &str| {
        let mut parts = spellings(&root_text);
        parts.extend(spellings(HOME));
        format!("{} {tail}", parts.join(" | "))
    };
    let checks = [CheckRecord {
        program: format!(r"{root_text}\tool.exe"),
        arguments: vec![leaky("arg one"), leaky("arg two")],
        exit_code: Some(1),
        stdout: leaky("stdout"),
        stderr: leaky("stderr"),
    }];
    let artifacts = [ArtifactRecord {
        file: leaky("artifact.spv"),
        sha256: "0".repeat(64),
    }];
    let failure = leaky("failed");

    write_validation_with_home(
        &root,
        HOME,
        &run(VerifyMode::Fast, &GitStamp::UNKNOWN),
        Some(&failure),
        &checks,
        &artifacts,
    )
    .unwrap();
    let json = fs::read_to_string(root.join(".ai/VALIDATION.json")).unwrap();
    let _ = fs::remove_dir_all(&root);

    let lowered = json.to_ascii_lowercase();
    for secret in spellings(&root_text).iter().chain(&spellings(HOME)) {
        for form in json_forms(secret) {
            assert!(
                !lowered.contains(&form),
                "VALIDATION.json leaks `{secret}`:\n{json}"
            );
        }
    }
    // The records survive, with placeholders where the paths were.
    assert!(json.contains(r#""program": "<workspace>"#), "{json}");
    assert!(json.contains(r#""passed": false"#), "{json}");
    assert!(json.contains(r#""exit_code": 1"#), "{json}");
    assert!(json.contains("<workspace>"), "{json}");
    assert!(json.contains("~"), "{json}");
    assert!(json.contains("artifact.spv"), "{json}");
    assert!(json.contains("failed"), "{json}");
}

const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

fn write_record(git: &GitStamp) -> String {
    let root = temp_root();
    write_validation_with_home(&root, HOME, &run(VerifyMode::Fast, git), None, &[], &[]).unwrap();
    let json = fs::read_to_string(root.join(".ai/VALIDATION.json")).unwrap();
    let _ = fs::remove_dir_all(&root);
    json
}

#[test]
fn the_record_names_the_commit_and_whether_the_tree_was_dirty() {
    for dirty in [true, false] {
        let json = write_record(&GitStamp {
            commit: Some(COMMIT.to_owned()),
            dirty: Some(dirty),
        });
        assert!(
            json.contains(&format!("\"git_commit\": \"{COMMIT}\",\n")),
            "{json}"
        );
        assert!(
            json.contains(&format!("\"git_dirty\": {dirty},\n")),
            "{json}"
        );
        let summary = compact_validation_summary(&json);
        assert!(summary.starts_with("finished=20"), "{summary}");
        assert!(
            summary.ends_with(&format!(
                " mode=fast passed=true commit={COMMIT} dirty={dirty}"
            )),
            "{summary}"
        );
    }
}

#[test]
fn the_record_says_null_when_git_is_unavailable() {
    let json = write_record(&GitStamp::UNKNOWN);
    assert!(json.contains("\"git_commit\": null,\n"), "{json}");
    assert!(json.contains("\"git_dirty\": null,\n"), "{json}");
    assert!(
        compact_validation_summary(&json).ends_with("commit=null dirty=null"),
        "{json}"
    );
    // A record written before the stamp existed reads as unknown, not as clean.
    let old = "{\n  \"mode\": \"full\",\n  \"passed\": true\n}\n";
    assert!(compact_validation_summary(old).ends_with("commit=unknown dirty=unknown"));
}

#[test]
fn a_missing_git_program_gives_an_unknown_stamp() {
    let root = temp_root();
    let stamp = GitStamp::read_with(&root, "gust-test-no-such-git-program");
    let _ = fs::remove_dir_all(&root);
    assert_eq!(stamp, GitStamp::UNKNOWN);
}

fn git(root: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
        ])
        .args(["-c", "commit.gpgsign=false", "-c", "core.autocrlf=false"])
        .args(args)
        .current_dir(root)
        .output()
        .expect("git must be on PATH (lint needs it too)");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

#[test]
fn the_stamp_of_a_real_repository_follows_head_and_the_working_tree() {
    // A repository of its own, nested under the target directory.
    let root = temp_root();
    git(&root, &["init", "--quiet"]);
    // Before the first commit there is no HEAD to name.
    assert_eq!(GitStamp::read(&root).commit, None);
    fs::write(root.join("tracked.txt"), "one\n").unwrap();
    git(&root, &["add", "tracked.txt"]);
    git(&root, &["commit", "--quiet", "-m", "first"]);
    let head = git(&root, &["rev-parse", "HEAD"]);

    let clean = GitStamp::read(&root);
    assert_eq!(clean.commit.as_deref(), Some(head.as_str()));
    assert_eq!(clean.dirty, Some(false));

    // An untracked file, a modified tracked file and a staged change each make it dirty.
    fs::write(root.join("new.txt"), "x\n").unwrap();
    assert_eq!(GitStamp::read(&root).dirty, Some(true));
    fs::remove_file(root.join("new.txt")).unwrap();
    assert_eq!(GitStamp::read(&root).dirty, Some(false));
    fs::write(root.join("tracked.txt"), "two\n").unwrap();
    assert_eq!(GitStamp::read(&root).dirty, Some(true));
    git(&root, &["add", "tracked.txt"]);
    assert_eq!(GitStamp::read(&root).dirty, Some(true));
    git(&root, &["commit", "--quiet", "-m", "second"]);
    let second = GitStamp::read(&root);
    let _ = fs::remove_dir_all(&root);
    assert_ne!(second.commit.as_deref(), Some(head.as_str()));
    assert_eq!(second.dirty, Some(false));
}
