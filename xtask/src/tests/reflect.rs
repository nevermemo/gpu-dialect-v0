use std::{env, fs, path::Path};

use super::{
    decode_diagnostic, decode_utf16le, diagnostic_tail, environment_path, find_in_path,
    parse_env_dump, vcvars_capture_command, vcvars_capture_command_line,
};

#[cfg(windows)]
use super::{capture_environment_from, capture_vcvars_environment};

fn utf16le(text: &str) -> Vec<u8> {
    text.encode_utf16()
        .flat_map(|unit| unit.to_le_bytes())
        .collect()
}

fn pairs(items: &[(&str, &str)]) -> Vec<(String, String)> {
    items
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect()
}

/// A scratch directory whose name needs every kind of cmd.exe care: spaces, parentheses, `&`,
/// `!`, `^` and a non-ASCII letter.
#[cfg(windows)]
fn odd_dir(tag: &str) -> std::path::PathBuf {
    let dir = env::temp_dir().join(format!(
        "gust reflect (x86) & !bang! ^ \u{e9}\u{e8} {tag} {}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn capture_command_line_quotes_the_path_once_inside_slash_s() {
    let vcvars = Path::new(
        "C:\\Program Files (x86)\\Microsoft Visual Studio\\2022\\VC/Auxiliary/Build/vcvars64.bat",
    );
    assert_eq!(
        vcvars_capture_command_line(vcvars).unwrap(),
        "/D /U /V:OFF /S /C \"\"C:\\Program Files (x86)\\Microsoft Visual Studio\\2022\\VC\\Auxiliary\\Build\\vcvars64.bat\" 1>&2 && set\""
    );
}

#[test]
fn capture_command_line_keeps_metacharacters_inside_the_quotes() {
    let line =
        vcvars_capture_command_line(Path::new("C:\\a&b (c)\\^d\\!bang!\\vcvars64.bat")).unwrap();
    assert_eq!(
        line,
        "/D /U /V:OFF /S /C \"\"C:\\a&b (c)\\^d\\!bang!\\vcvars64.bat\" 1>&2 && set\""
    );
}

#[test]
fn capture_command_line_rejects_paths_that_cannot_be_quoted() {
    for bad in [
        "C:\\50%\\vcvars64.bat",
        "C:\\a\"b\\vcvars64.bat",
        "C:\\a\nb\\x.bat",
        "C:\\a\rb\\x.bat",
    ] {
        let error = vcvars_capture_command_line(Path::new(bad)).unwrap_err();
        assert!(error.contains("cannot quote safely"), "{bad}: {error}");
        assert!(vcvars_capture_command(Path::new(bad)).is_err(), "{bad}");
    }
}

#[cfg(windows)]
#[test]
fn capture_command_hands_the_exact_line_to_cmd_as_one_raw_argument() {
    let vcvars = Path::new("C:\\a (b)\\vcvars64.bat");
    let command = vcvars_capture_command(vcvars).unwrap();
    assert_eq!(command.get_program(), "cmd");
    let args: Vec<_> = command.get_args().collect();
    assert_eq!(
        args,
        [vcvars_capture_command_line(vcvars).unwrap().as_str()]
    );
}

/// The real wiring: a fake `vcvars64.bat` in a directory full of cmd metacharacters and
/// non-ASCII letters runs through the real `cmd.exe`. With `Command::arg` (escaped inner
/// quotes) cmd cannot find the batch file; without `/V:OFF` the `!bang!` vanishes; with an
/// ANSI decode the accented letters are mangled.
#[cfg(windows)]
#[test]
fn capture_runs_a_batch_file_in_an_odd_directory_through_real_cmd() {
    let dir = odd_dir("run");
    let bat = dir.join("vcvars64.bat");
    fs::write(
        &bat,
        "@echo off\r\nset \"GUST_FAKE_HERE=%~dp0\"\r\nset \"GUST_FAKE_BANG=hi!there!\"\r\necho noise on stdout\r\n",
    )
    .unwrap();
    let environment = capture_vcvars_environment(&bat);
    fs::remove_dir_all(&dir).unwrap();
    let environment = environment.unwrap();
    let get = |name: &str| {
        environment
            .iter()
            .find(|(candidate, _)| candidate == name)
            .map(|(_, value)| value.clone())
    };
    assert_eq!(
        get("GUST_FAKE_HERE"),
        Some(format!("{}\\", dir.display())),
        "the directory (with !bang! and accents) must survive intact"
    );
    assert_eq!(get("GUST_FAKE_BANG"), Some("hi!there!".to_owned()));
    // The batch file's own stdout must not leak into the dump.
    assert!(environment.iter().all(|(name, _)| !name.contains("noise")));
    assert!(environment_path(&environment).is_some());
}

#[cfg(windows)]
#[test]
fn capture_failure_reports_the_exit_code_and_the_last_batch_output() {
    let dir = odd_dir("fail");
    let bat = dir.join("vcvars64.bat");
    fs::write(
        &bat,
        "@echo off\r\necho one\r\necho two\r\necho three\r\necho four\r\necho five\r\necho six\r\necho broke in \"%~dp0\"\r\nexit /b 7\r\n",
    )
    .unwrap();
    let error = capture_vcvars_environment(&bat).unwrap_err();
    fs::remove_dir_all(&dir).unwrap();
    assert!(error.contains("exit 7"), "{error}");
    assert!(error.contains("broke in "), "{error}");
    assert!(error.contains("\u{e9}\u{e8}"), "{error}");
    assert!(error.contains("six"), "{error}");
    assert!(error.contains("three"), "{error}");
    assert!(!error.contains("two"), "only the last five lines: {error}");
}

#[cfg(windows)]
#[test]
fn inherited_multiline_values_never_become_variables() {
    let dir = odd_dir("multi");
    let bat = dir.join("vcvars64.bat");
    fs::write(&bat, "@echo off\r\nset \"GUST_FAKE_OK=1\"\r\n").unwrap();

    // A continuation line is dropped together with its variable.
    let mut command = vcvars_capture_command(&bat).unwrap();
    command.env("GUST_MULTI", "first\r\nsecond line");
    let environment = capture_environment_from(command).unwrap();
    assert!(environment.iter().any(|(name, _)| name == "GUST_FAKE_OK"));
    assert!(environment.iter().all(|(name, _)| name != "GUST_MULTI"));
    assert!(environment.iter().all(|(_, value)| value != "second line"));

    // A continuation line that looks like `PATH=...` cannot be told from a variable, so the
    // duplicate is refused instead of overriding the real PATH.
    let mut command = vcvars_capture_command(&bat).unwrap();
    command.env("GUST_FORGE", "x\r\nPATH=C:\\evil");
    let error = capture_environment_from(command).unwrap_err();
    fs::remove_dir_all(&dir).unwrap();
    assert!(error.contains("more than once"), "{error}");
    assert!(error.contains("PATH") || error.contains("Path"), "{error}");
}

#[test]
fn env_dump_parses_crlf_values_with_equals_and_skips_drive_entries() {
    let dump =
        "=C:=C:\\work\r\nPath=C:\\a;C:\\b\r\nEMPTY=\r\nFLAGS=/a=b\r\nProgramFiles(x86)=C:\\pf\r\n";
    assert_eq!(
        parse_env_dump(dump).unwrap(),
        pairs(&[
            ("Path", "C:\\a;C:\\b"),
            ("EMPTY", ""),
            ("FLAGS", "/a=b"),
            ("ProgramFiles(x86)", "C:\\pf"),
        ])
    );
    assert!(parse_env_dump("").unwrap().is_empty());
}

#[test]
fn env_dump_drops_a_variable_whose_value_spans_lines() {
    let dump = "A=1\r\nMULTI=first\r\nsecond line\r\n\r\nthird\r\nB=2\r\n";
    assert_eq!(
        parse_env_dump(dump).unwrap(),
        pairs(&[("A", "1"), ("B", "2")])
    );
    // Lines before any variable, and a name with a space, are not variables.
    assert_eq!(
        parse_env_dump("banner\r\nbad name=1\r\nC=3\r\n").unwrap(),
        pairs(&[("C", "3")])
    );
}

#[test]
fn env_dump_refuses_a_forged_duplicate_in_any_casing() {
    for dump in [
        "Path=C:\\real\r\nX=1\r\nPATH=C:\\evil\r\n",
        "INCLUDE=a\r\ninclude=b\r\n",
        "LIB=a\r\nLIB=b\r\n",
    ] {
        let error = parse_env_dump(dump).unwrap_err();
        assert!(error.contains("more than once"), "{dump}: {error}");
    }
    // A forged line that follows a multi-line value is only reachable through a duplicate.
    let forged = "Path=C:\\real\r\nA=x\r\nPATH=C:\\evil\r\n";
    assert!(parse_env_dump(forged).is_err());
}

#[test]
fn utf16le_dump_keeps_non_ascii_directories() {
    let text =
        "Path=C:\\caf\u{e9}\\\u{6771}\u{4eac};C:\\bin\r\nTEMP=C:\\Users\\\u{fc}ser\\Temp\r\n";
    let decoded = decode_utf16le(&utf16le(text)).unwrap();
    assert_eq!(decoded, text);
    assert_eq!(
        parse_env_dump(&decoded).unwrap(),
        pairs(&[
            ("Path", "C:\\caf\u{e9}\\\u{6771}\u{4eac};C:\\bin"),
            ("TEMP", "C:\\Users\\\u{fc}ser\\Temp"),
        ])
    );
    // A byte order mark is dropped; an odd byte count is not UTF-16.
    let mut with_bom = utf16le("\u{feff}A=1\r\n");
    assert_eq!(decode_utf16le(&with_bom).unwrap(), "A=1\r\n");
    with_bom.push(0x41);
    assert!(
        decode_utf16le(&with_bom)
            .unwrap_err()
            .contains("odd length")
    );
    assert_eq!(decode_utf16le(&[]).unwrap(), "");
}

#[test]
fn diagnostics_decode_either_encoding_and_keep_the_last_lines() {
    assert_eq!(decode_diagnostic(b"plain ascii"), "plain ascii");
    assert_eq!(
        decode_diagnostic(&utf16le("caf\u{e9} wide")),
        "caf\u{e9} wide"
    );
    let wide = utf16le("a\r\n\r\nb\r\nc\r\nd\r\n");
    assert_eq!(diagnostic_tail(&wide, 2), "c | d");
    assert_eq!(diagnostic_tail(b"only\n", 5), "only");
    assert_eq!(diagnostic_tail(b"", 5), "");
}

#[test]
fn environment_path_ignores_casing_and_other_names() {
    for name in ["Path", "PATH", "path", "pAtH"] {
        let environment = pairs(&[("PATHEXT", ".EXE"), (name, "C:\\bin"), ("OTHER", "x")]);
        assert_eq!(environment_path(&environment), Some("C:\\bin"), "{name}");
    }
    assert_eq!(environment_path(&pairs(&[("PATHEXT", ".EXE")])), None);
    assert_eq!(environment_path(&[]), None);
}

#[test]
fn find_in_path_returns_the_first_directory_holding_a_file() {
    let root = env::temp_dir().join(format!("gust-reflect-path-{}", std::process::id()));
    let (first, second, third) = (root.join("one"), root.join("two"), root.join("three"));
    for dir in [&first, &second, &third] {
        fs::create_dir_all(dir).unwrap();
    }
    // A directory named like the program must not count; the first real file wins.
    fs::create_dir_all(first.join("cl.exe")).unwrap();
    fs::write(second.join("cl.exe"), b"").unwrap();
    fs::write(third.join("cl.exe"), b"").unwrap();
    let path = env::join_paths([&first, &second, &third]).unwrap();
    let found = find_in_path("cl.exe", path.to_str().unwrap());
    let none = find_in_path("link.exe", path.to_str().unwrap());
    fs::remove_dir_all(&root).unwrap();
    assert_eq!(found, Some(second.join("cl.exe")));
    assert_eq!(none, None);
    assert_eq!(find_in_path("cl.exe", ""), None);
}
