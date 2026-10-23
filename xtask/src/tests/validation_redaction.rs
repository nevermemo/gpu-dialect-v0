use super::{redact, redact_with, redaction_prefixes};

const ROOT: &str = r"C:\Users\someone\Desktop\gust";
pub(super) const HOME: &str = r"C:\Users\someone";

fn redacted(text: &str) -> String {
    redact(text, &redaction_prefixes(ROOT, HOME))
}

#[test]
fn workspace_root_spellings_become_a_placeholder() {
    assert_eq!(
        redacted(r"Compiling gust (C:\Users\someone\Desktop\gust\crates\gust)"),
        r"Compiling gust (<workspace>\crates\gust)"
    );
    assert_eq!(
        redacted(r"generated artifacts: \\?\C:\Users\someone\Desktop\gust\generated-wgpu"),
        r"generated artifacts: <workspace>\generated-wgpu"
    );
    assert_eq!(
        redacted("C:/Users/someone/Desktop/gust/target"),
        "<workspace>/target"
    );
    assert_eq!(
        redacted(r"C:\\Users\\someone\\Desktop\\gust\\crates"),
        r"<workspace>\\crates"
    );
}

#[test]
fn home_paths_outside_the_workspace_become_tilde() {
    assert_eq!(
        redacted(r"C:\Users\someone\.cargo\bin\slangc.exe"),
        r"~\.cargo\bin\slangc.exe"
    );
}

fn redacted_ignoring_case(text: &str) -> String {
    redact_with(text, &redaction_prefixes(ROOT, HOME), true)
}

#[test]
fn mismatched_casing_is_redacted_when_case_is_ignored() {
    // `current_dir()` spelled the root `gust`; an example printed the on-disk `GUST`.
    assert_eq!(
        redacted_ignoring_case(r"\\?\C:\Users\someone\Desktop\GUST\generated-wgpu"),
        r"<workspace>\generated-wgpu"
    );
    assert_eq!(
        redacted_ignoring_case(r"c:\USERS\Someone\Desktop\Gust\crates and C:/users/SOMEONE/.cargo"),
        r"<workspace>\crates and ~/.cargo"
    );
    assert_eq!(
        redacted_ignoring_case("nothing to see; Desktop\\GUST alone stays"),
        "nothing to see; Desktop\\GUST alone stays"
    );
}

#[test]
fn casing_is_respected_when_case_is_not_ignored() {
    let prefixes = redaction_prefixes(ROOT, HOME);
    assert_eq!(
        redact_with(
            r"C:\Users\someone\Desktop\GUST\generated-wgpu",
            &prefixes,
            false
        ),
        r"~\Desktop\GUST\generated-wgpu"
    );
}

#[test]
fn trailing_separators_on_the_home_are_dropped() {
    let backslash = redaction_prefixes(ROOT, r"C:\Users\someone\");
    assert_eq!(
        redact_with(r"C:\Users\someone\.cargo\bin", &backslash, false),
        r"~\.cargo\bin"
    );
    let forward = redaction_prefixes(ROOT, "C:/Users/someone//");
    assert_eq!(
        redact_with("C:/Users/someone/.cargo", &forward, false),
        "~/.cargo"
    );
    // A value made only of separators redacts nothing instead of everything.
    let degenerate = redaction_prefixes(ROOT, r"\");
    assert_eq!(redact_with(r"a\b/c", &degenerate, false), r"a\b/c");
}

#[test]
fn verbatim_unc_roots_are_redacted_in_every_spelling() {
    let prefixes = redaction_prefixes(r"\\?\UNC\server\share\gust", HOME);
    for (text, expected) in [
        (r"\\?\UNC\server\share\gust\src", r"<workspace>\src"),
        (r"\\server\share\gust\src", r"<workspace>\src"),
        (r"\\\\server\\share\\gust\\src", r"<workspace>\\src"),
        ("//server/share/gust/src", "<workspace>/src"),
    ] {
        assert_eq!(redact_with(text, &prefixes, false), expected, "{text}");
    }
    assert_eq!(
        redact_with(r"\\?\UNC\SERVER\Share\GUST\src", &prefixes, true),
        r"<workspace>\src"
    );
    // The same share written without the verbatim prefix gives the same spellings.
    assert_eq!(prefixes, redaction_prefixes(r"\\server\share\gust\", HOME));
}

#[test]
fn verbatim_root_and_empty_home_are_handled() {
    let prefixes = redaction_prefixes(r"\\?\C:\Users\someone\Desktop\gust", "");
    assert_eq!(
        redact(r"C:\Users\someone\Desktop\gust\src", &prefixes),
        r"<workspace>\src"
    );
    assert_eq!(redact("no paths here", &prefixes), "no paths here");
}
