use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use super::{
    ArtifactRecord, json_string, validation_redaction_tests::HOME, write_validation_with_home,
};
use crate::{process::CheckRecord, verify::VerifyMode};

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
        VerifyMode::Fast,
        "2026-10-23T00:00:00Z",
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
