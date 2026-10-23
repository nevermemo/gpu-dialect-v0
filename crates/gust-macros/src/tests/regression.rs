use std::{env, fs, path::Path};

use crate::{expand::kernel_options, slang::emit_kernel, validate::validate_module};

/// Set to `1` to rewrite a reviewed Slang golden with the emitted text. A bless prints a line
/// diff and still fails the test, so it cannot pass silently; review the diff, then rerun
/// without the variable. `cargo xtask bless` runs the golden tests this way.
const BLESS_VARIABLE: &str = "BLESS_GOLDEN";

/// Compares emitted Slang with the reviewed fixture `tests/fixtures/<fixture>`. Both texts are
/// already normalised to `\n` by the caller. Without `BLESS_GOLDEN=1` this is `assert_eq!`.
fn assert_golden(actual: &str, fixture: &str, expected: &str) {
    if actual == expected {
        return;
    }
    if env::var_os(BLESS_VARIABLE).is_some_and(|value| value == "1") {
        bless_golden(actual, fixture, expected);
    }
    assert_eq!(actual, expected);
}

fn bless_golden(actual: &str, fixture: &str, expected: &str) -> ! {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(fixture);
    // Keep the line endings the working tree already uses, so a bless diffs only real changes.
    let crlf = fs::read_to_string(&path).is_ok_and(|old| old.contains("\r\n"));
    let text = if crlf {
        actual.replace('\n', "\r\n")
    } else {
        actual.to_owned()
    };
    fs::write(&path, text).unwrap_or_else(|error| panic!("could not write {fixture}: {error}"));
    eprintln!("{}", line_diff(fixture, expected, actual));
    panic!("golden blessed; review the diff and rerun without BLESS_GOLDEN ({fixture})");
}

/// A unified line diff (old -> new) with three lines of context.
fn line_diff(name: &str, old: &str, new: &str) -> String {
    const CONTEXT: usize = 3;
    let old: Vec<&str> = old.lines().collect();
    let new: Vec<&str> = new.lines().collect();
    // Longest common subsequence table, filled from the ends.
    let mut common = vec![vec![0usize; new.len() + 1]; old.len() + 1];
    for i in (0..old.len()).rev() {
        for j in (0..new.len()).rev() {
            common[i][j] = if old[i] == new[j] {
                common[i + 1][j + 1] + 1
            } else {
                common[i + 1][j].max(common[i][j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut edits: Vec<(char, &str)> = Vec::new();
    while i < old.len() || j < new.len() {
        if i < old.len() && j < new.len() && old[i] == new[j] {
            edits.push((' ', old[i]));
            i += 1;
            j += 1;
        } else if i < old.len() && (j == new.len() || common[i + 1][j] >= common[i][j + 1]) {
            // Deletions come before insertions, as in `diff -u`.
            edits.push(('-', old[i]));
            i += 1;
        } else {
            edits.push(('+', new[j]));
            j += 1;
        }
    }
    let changed: Vec<usize> = (0..edits.len()).filter(|&k| edits[k].0 != ' ').collect();
    let mut out = format!("--- a/tests/fixtures/{name}\n+++ b/tests/fixtures/{name}\n");
    let mut next = 0;
    while next < changed.len() {
        let start = changed[next].saturating_sub(CONTEXT);
        let mut end = changed[next];
        while next < changed.len() && changed[next] <= end + 2 * CONTEXT + 1 {
            end = changed[next];
            next += 1;
        }
        let end = (end + CONTEXT + 1).min(edits.len());
        let before = |kind: char| {
            edits[..start]
                .iter()
                .filter(|edit| edit.0 == ' ' || edit.0 == kind)
                .count()
        };
        let span = |kind: char| {
            edits[start..end]
                .iter()
                .filter(|edit| edit.0 == ' ' || edit.0 == kind)
                .count()
        };
        out.push_str(&format!(
            "@@ -{},{} +{},{} @@\n",
            before('-') + 1,
            span('-'),
            before('+') + 1,
            span('+')
        ));
        for (kind, line) in &edits[start..end] {
            out.push_str(&format!("{kind}{line}\n"));
        }
    }
    out
}

#[test]
fn line_diff_reports_changed_lines_with_context() {
    let old = "a\nb\nc\nd\ne\nf\ng\nh\n";
    let new = "a\nb\nc\nX\ne\nf\ng\nh\n";
    assert_eq!(
        line_diff("t.slang", old, new),
        "--- a/tests/fixtures/t.slang\n+++ b/tests/fixtures/t.slang\n@@ -1,7 +1,7 @@\n a\n b\n c\n-d\n+X\n e\n f\n g\n"
    );
    assert_eq!(
        line_diff("t.slang", "a\n", "a\n"),
        "--- a/tests/fixtures/t.slang\n+++ b/tests/fixtures/t.slang\n"
    );
}

fn translate(source: &str) -> syn::Result<String> {
    let module: syn::ItemMod = syn::parse_str(source)?;
    validate_module(&module)?;
    let kernel = module
        .content
        .as_ref()
        .unwrap()
        .1
        .iter()
        .find_map(|item| match item {
            syn::Item::Fn(function) if function.sig.ident == "run" => Some(function),
            _ => None,
        })
        .unwrap();
    let options = kernel_options(kernel)?.expect("kernel has #[kernel] attribute");
    emit_kernel(&module, kernel, options)
}

#[test]
fn semantics_golden() {
    let source = translate(include_str!("../../../../tests/fixtures/semantics.rs")).unwrap();
    assert_golden(
        &source.replace("\r\n", "\n"),
        "semantics.slang",
        &include_str!("../../../../tests/fixtures/semantics.slang").replace("\r\n", "\n"),
    );
}

#[test]
fn numeric_golden() {
    let source = translate(include_str!("../../../../tests/fixtures/numeric.rs")).unwrap();
    assert_golden(
        &source.replace("\r\n", "\n"),
        "numeric.slang",
        &include_str!("../../../../tests/fixtures/numeric.slang").replace("\r\n", "\n"),
    );
}

#[test]
fn option_golden() {
    let source = translate(include_str!("../../../../tests/fixtures/option.rs")).unwrap();
    assert_golden(
        &source.replace("\r\n", "\n"),
        "option.slang",
        &include_str!("../../../../tests/fixtures/option.slang").replace("\r\n", "\n"),
    );
}

#[test]
fn result_lowering_emits_tagged_representation() {
    let source = translate(include_str!("../../../../tests/fixtures/result.rs")).unwrap();
    for expected in [
        "struct __GustResult<T> { bool isOk; T value; }",
        "__gust_ok((value / uint(2)))",
        "__gust_err(value)",
        "result.isOk",
        "(!result.isOk)",
        "__gust_unwrap_or(value, fallback)",
        "var __gust_match_",
    ] {
        assert!(source.contains(expected), "missing `{expected}`:\n{source}");
    }
}

#[test]
fn result_golden() {
    let source = translate(include_str!("../../../../tests/fixtures/result.rs")).unwrap();
    assert_golden(
        &source.replace("\r\n", "\n"),
        "result.slang",
        &format!(
            "{}\n",
            include_str!("../../../../tests/fixtures/result.slang")
                .replace("\r\n", "\n")
                .trim_end()
        ),
    );
}

#[test]
fn result_rejects_unproven_forms() {
    for (source, message) in [
        (
            "mod bad { fn f() -> Result<uint, int> { Ok(1u32) } #[kernel] fn run(id: SV_DispatchThreadID) {} }",
            "Result<T, T>",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let value: Result<Option<uint>, Option<uint>> = Ok(Some(1u32)); } }",
            "nested",
        ),
        (
            "mod bad { struct State { value: Result<uint, uint> } #[kernel] fn run(id: SV_DispatchThreadID) {} }",
            "struct fields",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID, out: RWStructuredBuffer<Result<uint, uint>>) {} }",
            "resource element",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let value: Result<uint, uint> = Ok(1u32); let x = value.unwrap(); } }",
            "unwrap",
        ),
    ] {
        let error = translate(source).unwrap_err();
        assert!(error.to_string().contains(message), "{error}: {source}");
    }
}

#[test]
fn result_match_rejects_unproven_shapes() {
    for (source, message) in [
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let value: Result<uint, uint> = Ok(1u32); match value { Ok(x) => { let y = x; } } } }",
            "exactly one `Ok` arm and one `Err` arm",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let value: Result<uint, uint> = Ok(1u32); match value { Ok(x) => { } Ok(y) => { } Err(e) => { } }; } }",
            "exactly one `Ok` arm and one `Err` arm",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let value: Result<uint, uint> = Ok(1u32); match value { Ok(x) if x > 0u32 => { } Err(e) => { } } } }",
            "guards",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let value: Result<uint, uint> = Ok(1u32); match value { #[allow(unused)] Ok(x) => { } Err(e) => { } }; } }",
            "attributes",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let value: Result<uint, uint> = Ok(1u32); match value { _ => { } Err(e) => { } }; } }",
            "only `Ok(identifier)` and `Err(identifier)`",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let value: Result<uint, uint> = Ok(1u32); match value { Ok((x, y)) => { } Err(e) => { } }; } }",
            "only `Ok(identifier)` and `Err(identifier)`",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let value: Result<uint, uint> = Ok(1u32); match value { Ok(mut x) => { } Err(e) => { } }; } }",
            "plain identifiers",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let value: Result<uint, uint> = Ok(1u32); match value { Ok(ref x) => { } Err(e) => { } }; } }",
            "plain identifiers",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let value: Result<uint, uint> = Ok(1u32); match value { Ok(x @ _) => { } Err(e) => { } }; } }",
            "plain identifiers",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID, mut output: RWStructuredBuffer<uint>) { let value: Result<uint, uint> = Ok(1u32); match value { Ok(output) => { } Err(error) => { } }; } }",
            "cannot shadow",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let value: Result<uint, uint> = Ok(1u32); match value { Ok(id) => { } Err(error) => { } }; } }",
            "cannot shadow",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let value: Result<uint, uint> = Ok(1u32); match value { Ok(x) => x, Err(e) => e } } }",
            "block arms",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let value: Result<uint, uint> = Ok(1u32); match value { Ok(x) => { let y = x; } Err(e) => { let y = e; } } } }",
            "terminate it with `;`",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let value: Result<uint, uint> = Ok(1u32); let output = match value { Ok(x) => { x } Err(e) => { e } }; } }",
            "statement",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { match 1u32 { Ok(x) => { } Err(e) => { } }; } }",
            "only for a known `Result<T, T>`",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let value: Option<uint> = Some(1u32); match value { Ok(x) => { } Err(e) => { } }; } }",
            "only for a known `Result<T, T>`",
        ),
    ] {
        let error = translate(source).unwrap_err();
        assert!(error.to_string().contains(message), "{error}: {source}");
    }
}

#[test]
fn result_match_emits_single_scrutinee_evaluation() {
    let source = translate(
        "mod results {
            fn classify(value: uint) -> Result<uint, uint> { if value > 0u32 { Ok(value) } else { Err(value) } }
            #[kernel] fn run(id: SV_DispatchThreadID, mut output: RWStructuredBuffer<uint>) {
                match classify(id.x) {
                    Err(error) => { output[id.x] = error + 1u32; }
                    Ok(value) => { output[id.x] = value + 2u32; }
                };
            }
        }",
    )
    .unwrap();
    assert!(
        source.contains("var __gust_match_0 = classify(id.x);"),
        "{source}"
    );
    assert!(source.contains("if (__gust_match_0.isOk)"), "{source}");
    assert!(
        source.contains("var value = __gust_match_0.value;"),
        "{source}"
    );
    assert!(
        source.contains("var error = __gust_match_0.value;"),
        "{source}"
    );
}

#[test]
fn result_match_accepts_a_result_parameter() {
    let source = translate(
        "mod results {
            fn select(value: Result<uint, uint>) -> uint {
                let mut output = 0u32;
                match value {
                    Ok(ok) => { output = ok; }
                    Err(error) => { output = error + 1u32; }
                };
                output
            }
            #[kernel] fn run(id: SV_DispatchThreadID, mut output: RWStructuredBuffer<uint>) {
                output[id.x] = select(Ok(id.x));
            }
        }",
    )
    .unwrap();
    assert!(
        source.contains("uint select(__GustResult<uint> value)"),
        "{source}"
    );
    assert!(source.contains("var __gust_match_1 = value;"), "{source}");
}

#[test]
fn loops_golden() {
    let source = translate(include_str!("../../../../tests/fixtures/loops.rs")).unwrap();
    assert_golden(
        &source.replace("\r\n", "\n"),
        "loops.slang",
        &include_str!("../../../../tests/fixtures/loops.slang").replace("\r\n", "\n"),
    );
}

#[test]
fn atomics_golden() {
    let source = translate(include_str!("../../../../tests/fixtures/atomics.rs")).unwrap();
    assert_golden(
        &source.replace("\r\n", "\n"),
        "atomics.slang",
        &include_str!("../../../../tests/fixtures/atomics.slang").replace("\r\n", "\n"),
    );
}

#[test]
fn unbounded_or_unproven_loop_forms_are_rejected() {
    for (source, message) in [
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let mut i = 0u32; while i < 4u32 { i = i + 1u32; } } }",
            "`while`",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { loop { break; } } }",
            "`loop`",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { for i in 0u32..=4u32 { } } }",
            "inclusive",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { for i in 0u32.. { } } }",
            "both bounds",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { for i in ..4u32 { } } }",
            "both bounds",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID, input: StructuredBuffer<uint>) { for v in input { } } }",
            "exclusive integer range",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { for (a, b) in 0u32..4u32 { } } }",
            "single identifier",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { for mut i in 0u32..4u32 { i = i + 1u32; } } }",
            "immutable",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { 'outer: for i in 0u32..4u32 { break 'outer; } } }",
            "labels",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { for i in 0u32..4u32 { break 'x; } } }",
            "labels",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { if id.x > 1u32 { break; } } }",
            "outside a loop",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { continue; } }",
            "outside a loop",
        ),
        (
            "mod bad { fn f() -> uint { for i in 0u32..4u32 { fn g() {} } 0u32 } #[kernel] fn run(id: SV_DispatchThreadID) { } }",
            "items inside",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let r = 0u32..4u32; } }",
            "only as the iterable",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { for i in 0..4u32 { } } }",
            "suffix",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { for i in 0u32..(-4) { } } }",
            "suffix",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID, out: RWStructuredBuffer<uint>) { for out in 0u32..4u32 { } } }",
            "cannot shadow",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { for i in 0u32..4u32 { let v = break; } } }",
            "statement",
        ),
    ] {
        let error = translate(source).unwrap_err();
        assert!(error.to_string().contains(message), "{error}: {source}");
    }
}

#[test]
fn for_loop_end_bound_is_evaluated_once_and_body_control_flow_maps_directly() {
    let source = translate(
        "mod shapes {
        fn count(limit: uint) -> uint {
            let mut n = 0u32;
            for i in 2u32..limit { if i == 3u32 { continue; } if i > 6u32 { break; } n = n + i; }
            n
        }
        #[kernel] fn run(id: SV_DispatchThreadID, mut out: RWStructuredBuffer<uint>) {
            if id.x < out.len() { for _ in 0u32..2u32 { out[id.x] = count(out[id.x]); } }
        }
    }",
    )
    .unwrap();
    assert!(source.contains("var __gust_end_1 = limit;"), "{source}");
    assert!(
        source.contains("for (var i = uint(2); i < __gust_end_1; i++)"),
        "{source}"
    );
    assert!(source.contains("continue;"), "{source}");
    assert!(source.contains("break;"), "{source}");
    assert!(
        source.contains(
            "for (var __gust_iter_0 = uint(0); __gust_iter_0 < __gust_end_0; __gust_iter_0++)"
        ),
        "{source}"
    );
}

#[test]
fn option_unsafe_forms_are_rejected() {
    for (source, message) in [
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let o: Option<uint> = Some(1u32); let v = o.unwrap(); } }",
            "panics",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let o: Option<uint> = Some(1u32); let v = o.expect(\"x\"); } }",
            "panics",
        ),
        (
            "mod bad { struct Holder { slot: Option<uint> } #[kernel] fn run(id: SV_DispatchThreadID) {} }",
            "struct fields",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID, out: RWStructuredBuffer<Option<uint>>) {} }",
            "resource element",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let o: Option<Option<uint>> = None; } }",
            "payload",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let o: Option<uint> = Some(1u32); if let Some(_) = o { } } }",
            "`Some(identifier)`",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let o: Option<uint> = Some(1u32); if let None = o { } } }",
            "`Some(identifier)`",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let o: Option<uint> = Some(1u32); if let Some(v) = o && v > 0u32 { } } }",
            "`if let`",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID, out: RWStructuredBuffer<uint>) { let o: Option<uint> = Some(1u32); if let Some(out) = o { } } }",
            "cannot shadow",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let o: Option<uint> = Some(1u32, 2u32); } }",
            "one argument",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let o: Option<uint> = Some(1u32); let v = o.unwrap_or(1u32, 2u32); } }",
            "one argument",
        ),
    ] {
        let error = translate(source).unwrap_err();
        assert!(error.to_string().contains(message), "{error}: {source}");
    }
}

#[test]
fn generated_symbol_prefixes_are_reserved() {
    for source in [
        "mod bad { #[kernel] fn run(__gpu_len_out: SV_DispatchThreadID) {} }",
        "mod bad { fn __gust_not(x: uint) -> uint { x } #[kernel] fn run(id: SV_DispatchThreadID) {} }",
    ] {
        let error = translate(source).unwrap_err();
        assert!(error.to_string().contains("reserved"));
    }
}

#[test]
fn invalid_kernel_contracts_are_rejected_early() {
    for (source, message) in [
        (
            "mod bad { #[kernel] fn run(a: SV_DispatchThreadID, b: SV_DispatchThreadID) {} }",
            "exactly one",
        ),
        ("mod bad { #[kernel] fn run() {} }", "exactly one"),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) -> uint { 1u32 } }",
            "return unit",
        ),
        (
            "mod bad { #[kernel(workgroup_size(0, 1, 1))] fn run(id: SV_DispatchThreadID) {} }",
            "nonzero",
        ),
        (
            "mod bad { #[kernel] #[kernel] fn run(id: SV_DispatchThreadID) {} }",
            "duplicate",
        ),
        (
            "mod bad { #[kernel(workgroup_size(64, 1, 1), workgroup_size(32, 1, 1))] fn run(id: SV_DispatchThreadID) {} }",
            "duplicate",
        ),
    ] {
        let module: syn::ItemMod = syn::parse_str(source).unwrap();
        let error = crate::expand::expand(module).unwrap_err();
        assert!(error.to_string().contains(message), "{error}: {source}");
    }
}

#[test]
fn unsupported_attributes_and_builtin_receivers_are_diagnosed() {
    for (source, message) in [
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID, out: RWStructuredBuffer<uint>) { let out = 1u32; let n = out.len(); } }",
            "cannot shadow",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { #[cfg(any())] let ignored = 1u32; } }",
            "attribute",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let n = id.len(); } }",
            "storage parameter",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID, out: RWStructuredBuffer<uint>) { let n = out.len(1u32); } }",
            "arguments",
        ),
        (
            "mod bad { fn helper(b: StructuredBuffer<uint>) -> uint { b.len() } #[kernel] fn run(id: SV_DispatchThreadID) {} }",
            "value parameters",
        ),
        (
            "mod bad { struct Generic<T> { field: T } #[kernel] fn run(id: SV_DispatchThreadID) {} }",
            "generic GPU structs",
        ),
    ] {
        let error = translate(source).unwrap_err();
        assert!(error.to_string().contains(message), "{error}: {source}");
    }
}

#[test]
fn helper_tail_branches_return_values() {
    let source = translate(
        "mod tails {
        fn choose(x: float) -> float { if x > 0.0 { x * 2.0 } else { { -x } } }
        #[kernel] fn run(id: SV_DispatchThreadID, mut out: RWStructuredBuffer<float>) {
            if id.x < out.len() { out[id.x] = choose(2.0); }
        }
    }",
    )
    .unwrap();
    assert!(source.contains("return (x * 2.0);"), "{source}");
    assert!(source.contains("return (-x);"), "{source}");
    assert!(!source.contains("return out["));
}

#[test]
fn typed_locals_preserve_numeric_type() {
    let source = translate("mod typed {
        fn quotient(x: uint) -> uint { let divisor: uint = 2; let mut answer: uint = x / divisor; answer += 1u32; answer }
        #[kernel] fn run(id: SV_DispatchThreadID) { let result: uint = quotient(id.x); }
    }").unwrap();
    assert!(source.contains("uint divisor = 2;"), "{source}");
    assert!(source.contains("uint answer = (x / divisor);"), "{source}");
    assert!(source.contains("uint result = quotient(id.x);"), "{source}");
}

#[test]
fn binding_modes_and_typed_resource_shadowing_fail_explicitly() {
    for (source, message) in [
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let ref x: uint = 1u32; } }",
            "binding modes",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let id: uint = 1u32; } }",
            "cannot shadow",
        ),
        (
            "mod bad { #[kernel] fn run(ref id: SV_DispatchThreadID) {} }",
            "binding modes",
        ),
    ] {
        let error = translate(source).unwrap_err();
        assert!(error.to_string().contains(message), "{error}");
    }
}

#[test]
fn rust_literals_are_lowered_not_copied() {
    let source = translate(
        "mod literals {
        #[kernel] fn run(id: SV_DispatchThreadID, mut out: RWStructuredBuffer<uint>) {
            if id.x < out.len() { out[id.x] = 0xff_u32 + 0b10u32 + 1_024u32 + (2.5f32 as uint); }
        }
    }",
    )
    .unwrap();
    assert!(source.contains("uint(255)"), "{source}");
    assert!(source.contains("uint(2)"), "{source}");
    assert!(source.contains("uint(1024)"), "{source}");
    assert!(source.contains("float(2.5)"), "{source}");
}

#[test]
fn binary_grouping_preserves_rust_precedence() {
    let source = translate(
        "mod grouping {
        fn masked(x: uint) -> bool { return x & 1u32 == 0u32; }
        #[kernel] fn run(id: SV_DispatchThreadID) { let even = masked(id.x); }
    }",
    )
    .unwrap();
    assert!(
        source.contains("return ((x & uint(1)) == uint(0));"),
        "{source}"
    );
}

#[test]
fn struct_constructor_preserves_field_identity_and_source_order() {
    let source = translate(
        "mod constructors {
        struct Pair { first: uint, second: uint }
        #[kernel] fn run(id: SV_DispatchThreadID) { let p = Pair { second: 1u32, first: 2u32 }; }
    }",
    )
    .unwrap();
    // Field identity is preserved by name even though `second` is written first.
    assert!(source.contains("Pair __gust_struct_0;"), "{source}");
    assert!(
        source.contains("__gust_struct_0.second = uint(1);"),
        "{source}"
    );
    assert!(
        source.contains("__gust_struct_0.first = uint(2);"),
        "{source}"
    );
    assert!(source.contains("Pair p = __gust_struct_0;"), "{source}");
    // Source evaluation order is preserved: `second` is assigned before `first`.
    assert!(
        source.find("__gust_struct_0.second = uint(1);").unwrap()
            < source.find("__gust_struct_0.first = uint(2);").unwrap(),
        "{source}"
    );
}

#[test]
fn struct_literal_in_expression_position_is_diagnosed() {
    let error = translate(
        "mod constructors {
        struct Pair { first: uint, second: uint }
        fn take(p: Pair) -> uint { p.first }
        #[kernel] fn run(id: SV_DispatchThreadID) { let v = take(Pair { first: 1u32, second: 2u32 }); }
    }",
    )
    .unwrap_err();
    assert!(error.to_string().contains("struct literals"), "{error}");
}

#[test]
fn struct_update_syntax_is_rejected_instead_of_dropping_fields() {
    for statement in [
        "let q = Pair { first: 9u32, ..p };",
        "p = Pair { first: 9u32, ..p };",
    ] {
        let source = format!(
            "mod update {{ struct Pair {{ first: uint, second: uint }} #[kernel] fn run(id: SV_DispatchThreadID) {{ let mut p = Pair {{ first: 1u32, second: 2u32 }}; {statement} }} }}"
        );
        let error = translate(&source).unwrap_err();
        assert!(error.to_string().contains("struct update"), "{error}");
    }
}

#[test]
fn unsupported_numeric_width_is_diagnosed() {
    let error = translate(
        "mod widths {
        #[kernel] fn run(id: SV_DispatchThreadID) { let wide = 1u64; }
    }",
    )
    .unwrap_err();
    assert!(error.to_string().contains("32-bit"), "{error}");
}

#[test]
fn casts_to_unproven_targets_are_rejected() {
    // `uint` is a plain `u32` alias, so rustc accepts every numeric target below in
    // the shadow body; only the macro can stop `f64(id.x)` reaching slangc.
    for target in [
        "f64",
        "u64",
        "i64",
        "usize",
        "isize",
        "u8",
        "u16",
        "i8",
        "i16",
        "bool",
        "char",
        "double",
        "Pair",
        "_",
        "*const uint",
        "gust::uint",
    ] {
        let source = format!(
            "mod bad {{ struct Pair {{ first: uint, second: uint }} #[kernel] fn run(id: SV_DispatchThreadID) {{ let v = id.x as {target}; }} }}"
        );
        let error = translate(&source).unwrap_err();
        assert!(error.to_string().contains("cast"), "{error}: {source}");
    }
    for (target, expected) in [
        ("f32", "float(id.x)"),
        ("float", "float(id.x)"),
        ("i32", "int(id.x)"),
        ("int", "int(id.x)"),
        ("u32", "uint(id.x)"),
        ("uint", "uint(id.x)"),
    ] {
        let source = format!(
            "mod ok {{ #[kernel] fn run(id: SV_DispatchThreadID) {{ let v = id.x as {target}; }} }}"
        );
        let ok = translate(&source).unwrap();
        assert!(ok.contains(expected), "{ok}");
    }
}

#[test]
fn non_32_bit_primitive_types_are_rejected_before_slang() {
    // rustc accepts each of these in the shadow; without a validator rule they
    // became `f64 v = 1.0;` and failed only inside slangc at pipeline creation.
    for (source, name) in [
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let v: f64 = 1.0; } }",
            "f64",
        ),
        (
            "mod bad { fn helper(x: u64) -> uint { 1u32 } #[kernel] fn run(id: SV_DispatchThreadID) {} }",
            "u64",
        ),
        (
            "mod bad { fn helper(x: uint) -> usize { 1 } #[kernel] fn run(id: SV_DispatchThreadID) {} }",
            "usize",
        ),
        (
            "mod bad { struct Wide { value: f64 } #[kernel] fn run(id: SV_DispatchThreadID) {} }",
            "f64",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID, out: RWStructuredBuffer<i64>) {} }",
            "i64",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let c: char = 'a'; } }",
            "char",
        ),
    ] {
        let error = translate(source).unwrap_err();
        let message = error.to_string();
        assert!(
            message.contains("not a supported GPU type") && message.contains(name),
            "{error}: {source}"
        );
    }
    let ok = translate(
        "mod ok { struct Mixed { a: float, b: int, c: uint, d: bool, e: f32, f: i32, g: u32 } #[kernel] fn run(id: SV_DispatchThreadID) { let flag: bool = true; } }",
    )
    .unwrap();
    assert!(ok.contains("bool flag = true;"), "{ok}");
}

#[test]
fn const_blocks_are_rejected() {
    for source in [
        "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let v = const { 1u32 }; } }",
        "mod bad { #[kernel] fn run(id: SV_DispatchThreadID, mut out: RWStructuredBuffer<uint>) { out[id.x] = const { 2u32 } + 1u32; } }",
        "mod bad { fn helper() -> uint { const { 3u32 } } #[kernel] fn run(id: SV_DispatchThreadID) {} }",
    ] {
        let error = translate(source).unwrap_err();
        assert!(
            error.to_string().contains("const blocks"),
            "{error}: {source}"
        );
    }
}

#[test]
fn unsupported_name_paths_are_rejected() {
    for (source, message) in [
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let v = f32::INFINITY; } }",
            "associated constants",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let v = u32::MAX; } }",
            "associated constants",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let v = core::f32::consts::PI; } }",
            "associated constants",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let v: core::f32 = 1.0; } }",
            "CPU-only",
        ),
    ] {
        let error = translate(source).unwrap_err();
        assert!(error.to_string().contains(message), "{error}: {source}");
    }
}

#[test]
fn unsupported_effects_are_rejected() {
    for (source, message) in [
        (
            "mod bad { fn maybe() -> Option<uint> { 1u32 } #[kernel] fn run(id: SV_DispatchThreadID) { let v = maybe()?; } }",
            "`?` operator",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { unsafe { let v = 1u32; } } }",
            "unsafe blocks",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { asm!(\"nop\"); } }",
            "macros",
        ),
    ] {
        let error = translate(source).unwrap_err();
        assert!(error.to_string().contains(message), "{error}: {source}");
    }
}

#[test]
fn integer_division_by_literal_zero_is_diagnosed() {
    for source in [
        "mod bad { #[kernel] fn run(id: SV_DispatchThreadID, mut out: RWStructuredBuffer<uint>) { out[id.x] = out[id.x] / 0u32; } }",
        "mod bad { #[kernel] fn run(id: SV_DispatchThreadID, mut out: RWStructuredBuffer<int>) { out[id.x] = out[id.x] % 0; } }",
        "mod bad { #[kernel] fn run(id: SV_DispatchThreadID, mut out: RWStructuredBuffer<uint>) { out[id.x] = out[id.x] / -0u32; } }",
    ] {
        let error = translate(source).unwrap_err();
        assert!(
            error.to_string().contains("literal zero"),
            "{error}: {source}"
        );
    }
    // Float division by zero is defined (it yields inf/NaN), so it must not be
    // rejected even though the divisor literal is zero.
    let ok = translate(
        "mod ok { #[kernel] fn run(id: SV_DispatchThreadID, mut out: RWStructuredBuffer<float>) { out[id.x] = out[id.x] / 0.0; } }",
    )
    .unwrap();
    assert!(ok.contains("(out[id.x] / 0.0)"), "{ok}");
}

// --- T10 atomics ---

#[test]
fn atomic_add_on_rw_u32_buffer_element_is_accepted() {
    let source = translate(
        "mod atomics {
        #[kernel] fn run(id: SV_DispatchThreadID, mut counter: RWStructuredBuffer<uint>) {
            atomic_add(&mut counter[0], 1u32);
        }
    }",
    )
    .unwrap();
    assert!(
        source.contains("RWStructuredBuffer<Atomic<uint>> counter;"),
        "buffer type must use Atomic<uint>: {source}"
    );
    assert!(
        source.contains("counter[0].add(uint(1));"),
        "atomic_add must lower to .add(): {source}"
    );
}

#[test]
fn atomic_add_with_dynamic_index_is_accepted() {
    let source = translate(
        "mod atomics {
        #[kernel] fn run(id: SV_DispatchThreadID, mut counter: RWStructuredBuffer<uint>) {
            atomic_add(&mut counter[id.x], 1u32);
        }
    }",
    )
    .unwrap();
    assert!(
        source.contains("RWStructuredBuffer<Atomic<uint>> counter;"),
        "buffer type must use Atomic<uint>: {source}"
    );
    assert!(
        source.contains("counter[id.x].add(uint(1));"),
        "atomic_add must lower to .add(): {source}"
    );
}

#[test]
fn atomic_add_on_rw_i32_buffer_element_is_accepted() {
    let source = translate(
        "mod atomics {
        #[kernel] fn run(id: SV_DispatchThreadID, mut counter: RWStructuredBuffer<int>) {
            atomic_add(&mut counter[0], 1i32);
        }
    }",
    )
    .unwrap();
    assert!(
        source.contains("RWStructuredBuffer<Atomic<int>> counter;"),
        "buffer type must use Atomic<int>: {source}"
    );
    assert!(
        source.contains("counter[0].add(int(1));"),
        "atomic_add on i32 must lower to .add(): {source}"
    );
}

#[test]
fn atomic_min_on_rw_u32_is_accepted() {
    let source = translate(
        "mod atomics {
        #[kernel] fn run(id: SV_DispatchThreadID, mut counter: RWStructuredBuffer<uint>) {
            atomic_min(&mut counter[0], 5u32);
        }
    }",
    )
    .unwrap();
    assert!(
        source.contains("RWStructuredBuffer<Atomic<uint>> counter;"),
        "buffer type must use Atomic<uint>: {source}"
    );
    assert!(
        source.contains("counter[0].min(uint(5));"),
        "atomic_min must lower to .min(): {source}"
    );
}

#[test]
fn atomic_max_on_rw_u32_is_accepted() {
    let source = translate(
        "mod atomics {
        #[kernel] fn run(id: SV_DispatchThreadID, mut counter: RWStructuredBuffer<uint>) {
            atomic_max(&mut counter[0], 5u32);
        }
    }",
    )
    .unwrap();
    assert!(
        source.contains("RWStructuredBuffer<Atomic<uint>> counter;"),
        "buffer type must use Atomic<uint>: {source}"
    );
    assert!(
        source.contains("counter[0].max(uint(5));"),
        "atomic_max must lower to .max(): {source}"
    );
}

#[test]
fn atomic_exchange_on_rw_u32_is_accepted() {
    let source = translate(
        "mod atomics {
        #[kernel] fn run(id: SV_DispatchThreadID, mut counter: RWStructuredBuffer<uint>) {
            atomic_exchange(&mut counter[0], 42u32);
        }
    }",
    )
    .unwrap();
    assert!(
        source.contains("RWStructuredBuffer<Atomic<uint>> counter;"),
        "buffer type must use Atomic<uint>: {source}"
    );
    assert!(
        source.contains("counter[0].exchange(uint(42));"),
        "atomic_exchange must lower to .exchange(): {source}"
    );
}

#[test]
fn atomic_exchange_on_rw_i32_is_accepted() {
    let source = translate(
        "mod atomics {
        #[kernel] fn run(id: SV_DispatchThreadID, mut counter: RWStructuredBuffer<int>) {
            atomic_exchange(&mut counter[0], 1i32);
        }
    }",
    )
    .unwrap();
    assert!(
        source.contains("RWStructuredBuffer<Atomic<int>> counter;"),
        "buffer type must use Atomic<int>: {source}"
    );
    assert!(
        source.contains("counter[0].exchange(int(1));"),
        "atomic_exchange on i32 must lower to .exchange(): {source}"
    );
}

#[test]
fn atomic_compare_exchange_on_rw_u32_is_accepted() {
    let source = translate(
        "mod atomics {
        #[kernel] fn run(id: SV_DispatchThreadID, mut counter: RWStructuredBuffer<uint>) {
            atomic_compare_exchange(&mut counter[0], 0u32, 1u32);
        }
    }",
    )
    .unwrap();
    assert!(
        source.contains("RWStructuredBuffer<Atomic<uint>> counter;"),
        "buffer type must use Atomic<uint>: {source}"
    );
    assert!(
        source.contains("counter[0].compareExchange(uint(0), uint(1));"),
        "atomic_compare_exchange must lower to .compareExchange(): {source}"
    );
}

#[test]
fn atomic_compare_exchange_on_rw_i32_is_accepted() {
    let source = translate(
        "mod atomics {
        #[kernel] fn run(id: SV_DispatchThreadID, mut counter: RWStructuredBuffer<int>) {
            atomic_compare_exchange(&mut counter[0], 0i32, 1i32);
        }
    }",
    )
    .unwrap();
    assert!(
        source.contains("RWStructuredBuffer<Atomic<int>> counter;"),
        "buffer type must use Atomic<int>: {source}"
    );
    assert!(
        source.contains("counter[0].compareExchange(int(0), int(1));"),
        "atomic_compare_exchange on i32 must lower to .compareExchange(): {source}"
    );
}

#[test]
fn non_atomic_buffer_keeps_plain_element_type() {
    let source = translate(
        "mod plain {
        #[kernel] fn run(id: SV_DispatchThreadID, mut data: RWStructuredBuffer<uint>) {
            data[id.x] = data[id.x] + 1u32;
        }
    }",
    )
    .unwrap();
    assert!(
        source.contains("RWStructuredBuffer<uint> data;"),
        "non-atomic buffer must keep plain element type: {source}"
    );
    assert!(
        !source.contains("Atomic<"),
        "non-atomic buffer must not use Atomic: {source}"
    );
}

#[test]
fn atomic_ops_reject_unsupported_receivers() {
    for (source, message) in [
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID, counter: StructuredBuffer<uint>) { atomic_add(&mut counter[0], 1u32); } }",
            "read-write buffer",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID, mut counter: RWStructuredBuffer<float>) { atomic_add(&mut counter[0], 1.0f32); } }",
            "u32",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID, mut counter: RWStructuredBuffer<uint>) { let mut local = 0u32; atomic_add(&mut local, 1u32); } }",
            "read-write buffer",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID, mut counter: RWStructuredBuffer<uint>) { atomic_add(&mut counter, 1u32); } }",
            "buffer element",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID, mut counter: RWStructuredBuffer<uint>) { atomic_add(&counter[0], 1u32); } }",
            "mutable reference",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID, mut counter: RWStructuredBuffer<uint>) { atomic_add(&mut counter[0]); } }",
            "2 arguments",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID, mut counter: RWStructuredBuffer<uint>) { atomic_add(&mut counter[0], 1u32, 2u32); } }",
            "2 arguments",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID, mut counter: RWStructuredBuffer<uint>) { atomic_min(&mut counter[0], 1u32, 2u32); } }",
            "2 arguments",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID, mut counter: RWStructuredBuffer<uint>) { atomic_compare_exchange(&mut counter[0], 1u32); } }",
            "3 arguments",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID, mut counter: RWStructuredBuffer<uint>) { atomic_compare_exchange(&mut counter[0], 1u32, 2u32, 3u32); } }",
            "3 arguments",
        ),
    ] {
        let error = translate(source).unwrap_err();
        assert!(error.to_string().contains(message), "{error}: {source}");
    }
}

#[test]
fn atomic_min_max_reject_signed_elements() {
    for (source, message) in [
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID, mut counter: RWStructuredBuffer<int>) { atomic_min(&mut counter[0], 1i32); } }",
            "unsigned",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID, mut counter: RWStructuredBuffer<int>) { atomic_max(&mut counter[0], 1i32); } }",
            "unsigned",
        ),
    ] {
        let error = translate(source).unwrap_err();
        assert!(error.to_string().contains(message), "{error}: {source}");
    }
}

#[test]
fn atomic_ops_reject_unsupported_element_types() {
    for (source, message) in [
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID, mut counter: RWStructuredBuffer<float>) { atomic_add(&mut counter[0], 1.0f32); } }",
            "u32",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID, mut counter: RWStructuredBuffer<float>) { atomic_exchange(&mut counter[0], 1.0f32); } }",
            "u32",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID, mut counter: RWStructuredBuffer<float>) { atomic_compare_exchange(&mut counter[0], 1.0f32, 2.0f32); } }",
            "u32",
        ),
    ] {
        let error = translate(source).unwrap_err();
        assert!(error.to_string().contains(message), "{error}: {source}");
    }
}

#[test]
fn atomic_ops_are_rejected_in_expression_position() {
    for source in [
        "mod bad { #[kernel] fn run(id: SV_DispatchThreadID, mut counter: RWStructuredBuffer<uint>) { let old = atomic_add(&mut counter[0], 1u32); } }",
        "mod bad { #[kernel] fn run(id: SV_DispatchThreadID, mut counter: RWStructuredBuffer<uint>) { if atomic_compare_exchange(&mut counter[0], 0u32, 1u32) { counter[0] = 2u32; } } }",
    ] {
        let error = translate(source).unwrap_err();
        assert!(
            error.to_string().contains("only as a statement"),
            "{error}: {source}"
        );
    }
}

#[test]
fn atomic_buffers_reject_ordinary_index_access() {
    for source in [
        "mod bad {
            #[kernel] fn run(id: SV_DispatchThreadID, mut counter: RWStructuredBuffer<uint>, mut output: RWStructuredBuffer<uint>) {
                atomic_add(&mut counter[0], 1u32);
                output[id.x] = counter[0];
            }
        }",
        "mod bad {
            #[kernel] fn run(_id: SV_DispatchThreadID, mut counter: RWStructuredBuffer<uint>) {
                atomic_add(&mut counter[0], 1u32);
                counter[0] = 0u32;
            }
        }",
    ] {
        let error = translate(source).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("atomic buffer elements cannot be read or written directly"),
            "{error}: {source}"
        );
    }
}

#[test]
fn atomic_usage_is_isolated_to_the_emitted_kernel() {
    let source = "mod atomics {
        #[kernel] fn run(_id: SV_DispatchThreadID, mut counter: RWStructuredBuffer<uint>) {
            atomic_add(&mut counter[0], 1u32);
        }
        #[kernel] fn reader(id: SV_DispatchThreadID, counter: StructuredBuffer<uint>, mut output: RWStructuredBuffer<uint>) {
            output[id.x] = counter[id.x];
        }
    }";
    let module: syn::ItemMod = syn::parse_str(source).unwrap();
    validate_module(&module).unwrap();
    let reader = module
        .content
        .as_ref()
        .unwrap()
        .1
        .iter()
        .find_map(|item| match item {
            syn::Item::Fn(function) if function.sig.ident == "reader" => Some(function),
            _ => None,
        })
        .unwrap();
    let options = kernel_options(reader).unwrap().unwrap();
    let emitted = emit_kernel(&module, reader, options).unwrap();
    assert!(
        emitted.contains("StructuredBuffer<uint> counter;"),
        "{emitted}"
    );
    assert!(!emitted.contains("Atomic<uint> counter"), "{emitted}");
}
