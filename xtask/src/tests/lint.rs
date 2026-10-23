use super::{find_machine_path, is_forbidden_script, machine_path_problems};

#[test]
fn shell_script_extensions_are_forbidden_in_any_directory() {
    for path in [
        "build.ps1",
        "tools/Setup.psm1",
        "tools/Setup.psd1",
        "run.bat",
        "scripts/run.cmd",
        "ci/check.sh",
        ".agents/skills/slang-language/scripts/Compile-Slang.ps1",
    ] {
        assert!(is_forbidden_script(path), "{path} should be forbidden");
    }
}

#[test]
fn script_extensions_match_case_insensitively_and_on_hidden_names() {
    assert!(is_forbidden_script("Build.PS1"));
    assert!(is_forbidden_script("scripts/RUN.Sh"));
    assert!(is_forbidden_script(".sh"));
    assert!(is_forbidden_script(r"scripts\run.bat"));
}

#[test]
fn other_files_and_lookalike_names_are_allowed() {
    for path in [
        "build.rs",
        "README.md",
        "scripts/probes/slang-layout.cpp",
        "docs/shell.md",
        "xtask/src/shell.rs",
        "script.sh.md",
        "notes/ps1",
        "tools/cmd",
        "scripts/run.sh/README.md",
        ".github/workflows/ci.yml",
    ] {
        assert!(!is_forbidden_script(path), "{path} should be allowed");
    }
}

#[test]
fn windows_home_paths_match_in_every_slash_style() {
    for (line, expected) in [
        (r"C:\Users\someone\Desktop\gust", r"C:\Users\someone"),
        ("C:/Users/someone/Desktop/gust", "C:/Users/someone"),
        (r"c:\users\someone", r"c:\users\someone"),
        (r"C:\\Users\\someone\\gust", r"C:\\Users\\someone"),
        (r"\\?\D:\Users\some-one\x", r"D:\Users\some-one"),
        (
            r#"error at "C:\Users\bob.smith\x.rs""#,
            r"C:\Users\bob.smith",
        ),
    ] {
        assert_eq!(find_machine_path(line), Some(expected), "{line}");
    }
}

#[test]
fn unix_home_paths_match_with_a_trailing_slash() {
    assert_eq!(
        find_machine_path("cd /home/someone/project"),
        Some("/home/someone")
    );
    assert_eq!(
        find_machine_path("at /Users/someone/project/src"),
        Some("/Users/someone")
    );
    assert_eq!(find_machine_path("(/home/a_b-c/x)"), Some("/home/a_b-c"));
}

#[test]
fn placeholders_and_relative_paths_are_not_machine_paths() {
    for line in [
        r"<workspace>\crates\gust",
        "<workspace>/crates/gust",
        "~/.cargo/bin/slangc",
        r"~\.cargo\bin\slangc.exe",
        r"X:\Users\<name>",
        r"X:/Users/<name>/",
        "/home/<name>/",
        "/Users/<name>/",
        r"%USERPROFILE%\.cargo",
        "$HOME/.cargo/bin",
        "docs/home/foo/bar",
        "./home/foo/bar",
        "https://example.com/home/page/",
        "https://example.com/Users/page/",
        r"PATH:\Users\someone",
        r"C:\Users",
        r"C:\Users\",
        "/home/",
        "/home/someone",
        "no paths here",
        "",
    ] {
        assert_eq!(find_machine_path(line), None, "{line}");
    }
}

#[test]
fn multibyte_text_around_a_path_is_handled() {
    assert_eq!(
        find_machine_path("日本語 /home/someone/x"),
        Some("/home/someone")
    );
    assert_eq!(find_machine_path("日本語/home/someone/x"), None);
    assert_eq!(
        find_machine_path(r"é C:\Users\José\x"),
        Some(r"C:\Users\José")
    );
}

#[test]
fn problems_report_file_and_line() {
    let text = "clean line\nroot = C:\\Users\\someone\\gust\nalso /home/someone/x/y\n";
    let problems = machine_path_problems("docs/notes.md", text);
    assert_eq!(problems.len(), 2);
    assert!(
        problems[0].starts_with(r"docs/notes.md:2: machine-specific path `C:\Users\someone`"),
        "{}",
        problems[0]
    );
    assert!(
        problems[1].starts_with("docs/notes.md:3: machine-specific path `/home/someone`"),
        "{}",
        problems[1]
    );
}

#[test]
fn only_the_listed_fixtures_may_spell_placeholder_machine_paths() {
    let text = r"C:\Users\someone\gust";
    assert!(machine_path_problems("xtask/src/tests/lint.rs", text).is_empty());
    assert!(machine_path_problems("xtask/src/tests/validation_redaction.rs", text).is_empty());
    assert_eq!(machine_path_problems("xtask/src/lint.rs", text).len(), 1);
    assert_eq!(machine_path_problems("docs/other.md", text).len(), 1);
}

#[test]
fn fixture_files_accept_only_the_placeholder_user_names() {
    let fixture = "xtask/src/tests/lint.rs";
    for name in ["someone", "bob.smith", "José", "a_b-c", "some-one"] {
        for text in [
            format!(r"C:\Users\{name}\gust"),
            format!("/home/{name}/gust"),
            format!("/mnt/c/Users/{name}/gust"),
            format!(r"\\wsl.localhost\Ubuntu\home\{name}\gust"),
        ] {
            assert!(machine_path_problems(fixture, &text).is_empty(), "{text}");
        }
    }
    // Case variants of a placeholder are placeholders too (the redaction tests need them).
    assert!(machine_path_problems(fixture, &format!(r"c:\USERS\{}", "SOMEONE")).is_empty());
    // The names below are built at run time so that this file stays clean itself.
    let real = ["alice", "carol-d"];
    for name in real {
        for text in [
            format!(r"C:\Users\{name}\gust"),
            format!("/home/{name}/gust"),
            format!("/mnt/c/Users/{name}"),
            format!(r"\\wsl.localhost\Ubuntu\home\{name}\gust"),
        ] {
            let problems = machine_path_problems(fixture, &text);
            assert_eq!(problems.len(), 1, "{text}");
            assert!(
                problems[0].contains("fixtures may only use"),
                "{}",
                problems[0]
            );
        }
    }
}

#[test]
fn every_path_on_a_line_is_checked_not_only_the_first() {
    let mixed = format!(r"C:\Users\someone\a and /home/{}/x", "alice");
    let problems = machine_path_problems("xtask/src/tests/lint.rs", &mixed);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("/home/alice"), "{}", problems[0]);
    let two = r"C:\Users\someone\a and D:\Users\bob.smith\b";
    assert_eq!(machine_path_problems("docs/x.md", two).len(), 2);
    assert!(machine_path_problems("xtask/src/tests/lint.rs", two).is_empty());
}

#[test]
fn wsl_mount_and_network_home_paths_match() {
    for (line, expected) in [
        ("cd /mnt/c/Users/someone/Desktop", "/mnt/c/Users/someone"),
        ("/mnt/d/users/bob.smith", "/mnt/d/users/bob.smith"),
        ("(/mnt/c/Users/a_b-c/x)", "/mnt/c/Users/a_b-c"),
        (
            r"\\wsl.localhost\Ubuntu\home\someone\gust",
            r"\\wsl.localhost\Ubuntu\home\someone",
        ),
        (
            r"\wsl.localhost\Ubuntu-24.04\home\some-one\",
            r"\wsl.localhost\Ubuntu-24.04\home\some-one",
        ),
        (r"\\WSL$\Ubuntu\home\José\x", r"\\WSL$\Ubuntu\home\José"),
        (
            r"\\wsl.localhost\\Ubuntu\\home\\someone\\x",
            r"\\wsl.localhost\\Ubuntu\\home\\someone",
        ),
        (
            r"//wsl.localhost/Ubuntu/home/someone/x",
            r"//wsl.localhost/Ubuntu/home/someone",
        ),
        (
            r"\\wsl.localhost\<distro>\home\someone\",
            r"\\wsl.localhost\<distro>\home\someone",
        ),
        (
            r#"open "\\wsl$\Debian\home\someone""#,
            r"\\wsl$\Debian\home\someone",
        ),
    ] {
        assert_eq!(find_machine_path(line), Some(expected), "{line}");
    }
}

#[test]
fn wsl_placeholders_and_non_home_paths_are_not_machine_paths() {
    for line in [
        "/mnt/c/Users/<name>",
        "/mnt/c/Users/%USERNAME%/x",
        "/mnt/c/Users",
        "/mnt/c/Users/",
        "/mnt/cc/Users/someone",
        "/mnt/c/Windows/someone",
        "docs/mnt/c/Users/someone",
        r"\\wsl.localhost\<distro>\home\<name>\",
        r"\\wsl.localhost\Ubuntu\home",
        r"\\wsl.localhost\Ubuntu\opt\someone",
        r"\\wsl.localhost\Ubuntu",
        r"\\wsl.localhost",
        r"\\wsl.localhost\\home\someone",
        r"\\wsl.example\Ubuntu\home\someone",
        r"x\wsl.localhost\Ubuntu\home\someone",
    ] {
        assert_eq!(find_machine_path(line), None, "{line}");
    }
}
