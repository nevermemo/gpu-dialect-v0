use super::{redact, redaction_prefixes};

const ROOT: &str = r"C:\Users\someone\Desktop\gust";
const HOME: &str = r"C:\Users\someone";

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

#[test]
fn verbatim_root_and_empty_home_are_handled() {
    let prefixes = redaction_prefixes(r"\\?\C:\Users\someone\Desktop\gust", "");
    assert_eq!(
        redact(r"C:\Users\someone\Desktop\gust\src", &prefixes),
        r"<workspace>\src"
    );
    assert_eq!(redact("no paths here", &prefixes), "no paths here");
}
