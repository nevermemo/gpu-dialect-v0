use std::{env, fs, path::Path};

use super::{find_in_path, parse_env_dump, vcvars_capture_command_line};

#[test]
fn capture_command_line_quotes_the_path_once_inside_slash_s() {
    let vcvars = Path::new(
        "C:\\Program Files (x86)\\Microsoft Visual Studio\\2022\\VC/Auxiliary/Build/vcvars64.bat",
    );
    assert_eq!(
        vcvars_capture_command_line(vcvars).unwrap(),
        "/S /C \"\"C:\\Program Files (x86)\\Microsoft Visual Studio\\2022\\VC\\Auxiliary\\Build\\vcvars64.bat\" >nul && set\""
    );
}

#[test]
fn capture_command_line_keeps_metacharacters_inside_the_quotes() {
    let line = vcvars_capture_command_line(Path::new("C:\\a&b (c)\\^d\\vcvars64.bat")).unwrap();
    assert_eq!(
        line,
        "/S /C \"\"C:\\a&b (c)\\^d\\vcvars64.bat\" >nul && set\""
    );
}

#[test]
fn capture_command_line_rejects_paths_that_cannot_be_quoted() {
    for bad in [
        "C:\\50%\\vcvars64.bat",
        "C:\\a\"b\\vcvars64.bat",
        "C:\\a\nb\\x.bat",
    ] {
        let error = vcvars_capture_command_line(Path::new(bad)).unwrap_err();
        assert!(error.contains("cannot quote safely"), "{bad}: {error}");
    }
}

#[test]
fn env_dump_parses_crlf_values_with_equals_and_skips_drive_entries() {
    let dump =
        "Path=C:\\a;C:\\b\r\n=C:=C:\\work\r\nEMPTY=\r\nFLAGS=/a=b\r\nbanner without equals\r\n";
    assert_eq!(
        parse_env_dump(dump),
        [
            ("Path".to_owned(), "C:\\a;C:\\b".to_owned()),
            ("EMPTY".to_owned(), String::new()),
            ("FLAGS".to_owned(), "/a=b".to_owned()),
        ]
    );
    assert!(parse_env_dump("").is_empty());
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
