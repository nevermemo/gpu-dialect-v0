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
fn only_the_listed_fixtures_may_spell_machine_paths() {
    let text = r"C:\Users\someone\gust";
    assert!(machine_path_problems("xtask/src/tests/lint.rs", text).is_empty());
    assert!(machine_path_problems("xtask/src/tests/validation_redaction.rs", text).is_empty());
    assert_eq!(machine_path_problems("xtask/src/lint.rs", text).len(), 1);
    assert_eq!(machine_path_problems("docs/other.md", text).len(), 1);
}
