//! Slang failures that rustc and the GUST validator accept must still point at the
//! Rust line the faulty statement was written on (DX19 line table).
//!
//! The fixtures below are valid Rust and pass validation, so only `slangc` can
//! reject them: a local named `new` is a Slang syntax error where it is used, and
//! a self-recursive helper is "recursion not allowed". The expected Rust line is
//! found by searching
//! this very file for a marker comment, so no line number is written down by hand.
//!
//! Both compile paths are covered: `slang::compile` and the two failure sites of
//! `reflect` (`reflect::reflect`, which runs `slangc`, and
//! `reflect::compile_reflected_with_helper`). The helper site runs a scripted
//! stand-in for the native reflection helper that fails with a Slang-shaped error
//! line, so this file needs only `slangc` on `PATH`, not the helper; the real
//! helper is exercised by the reflection tests.

use gust::{
    Kernel, KernelDescriptor, SourceOrigin, reflect,
    slang::{self, Error, Target},
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

const THIS_FILE: &str = include_str!("diagnostics.rs");

#[gust::gpu]
mod keyword_local {
    #[kernel]
    pub fn run(id: SV_DispatchThreadID, mut out: RWStructuredBuffer<uint>) {
        if id.x < out.len() {
            let new = id.x;
            out[id.x] = new; // FAILS: keyword local
        }
    }
}

#[gust::gpu]
mod recursive_helper {
    fn count_down(n: uint) -> uint {
        if n == 0u32 {
            0u32
        } else {
            count_down(n - 1u32) // FAILS: recursion
        }
    }

    #[kernel]
    pub fn run(id: SV_DispatchThreadID, mut out: RWStructuredBuffer<uint>) {
        if id.x < out.len() {
            out[id.x] = count_down(id.x);
        }
    }
}

#[gust::gpu]
mod else_if_chain {
    #[kernel]
    pub fn run(id: SV_DispatchThreadID, mut out: RWStructuredBuffer<uint>) {
        if id.x < out.len() {
            let new = id.x;
            // OWNS: else-if chain
            if id.x == 0u32 {
                out[id.x] = 1u32;
            } else if new == 1u32 {
                // FAILS: else-if condition
                out[id.x] = 2u32;
            } else {
                out[id.x] = 3u32;
            }
        }
    }
}

/// The 1-based line of this file whose code ends with the `// FAILS: <what>` marker.
fn marked_line(what: &str) -> u32 {
    line_ending_with(&format!("// FAILS: {what}"))
}

/// The 1-based line right after the `// OWNS: <what>` comment line of this file.
fn owner_line(what: &str) -> u32 {
    line_ending_with(&format!("// OWNS: {what}")) + 1
}

fn line_ending_with(marker: &str) -> u32 {
    let line = THIS_FILE
        .lines()
        .position(|line| line.trim_end().ends_with(marker))
        .unwrap_or_else(|| panic!("no line ends with `{marker}`"));
    u32::try_from(line + 1).unwrap()
}

/// The diagnostics of a failed compile, which must be an actual compiler failure.
fn failure(kernel: &KernelDescriptor, target: Target) -> String {
    match slang::compile(kernel, target) {
        Err(Error::CompilationFailed { diagnostics, .. }) => diagnostics,
        other => panic!("{target:?}: expected a Slang compile failure, got {other:?}"),
    }
}

/// The `file` and `line` of the `originating Rust source: file:line` annotation.
fn rust_source(diagnostics: &str) -> (&str, u32) {
    let line = diagnostics
        .lines()
        .find_map(|line| line.strip_prefix("originating Rust source: "))
        .unwrap_or_else(|| panic!("no Rust source annotation in:\n{diagnostics}"));
    let (file, number) = line.rsplit_once(':').expect("file:line");
    (file, number.parse().expect("line number"))
}

fn assert_table_is_recorded(kernel: &KernelDescriptor) {
    let source = &kernel.source;
    assert!(
        source.file.ends_with("diagnostics.rs"),
        "file `{}`",
        source.file
    );
    assert!(
        !source.file.contains('\\'),
        "`{}` uses `/` only",
        source.file
    );
    // The path is never absolute and never leaks the build machine's layout.
    let manifest_dir = env!("CARGO_MANIFEST_DIR").replace('\\', "/");
    assert!(
        !std::path::Path::new(source.file).is_absolute()
            && !source.file.starts_with('/')
            && !source.file.contains(':'),
        "`{}` is relative",
        source.file
    );
    assert!(
        !source.file.contains(&manifest_dir),
        "`{}` does not embed the manifest directory",
        source.file
    );
    assert!(!source.lines.is_empty());
    for record in source.lines {
        assert!(record.slang_start >= 1 && record.slang_start <= record.slang_end);
        assert!(record.rust_line >= 1);
    }
    assert!(
        source
            .lines
            .windows(2)
            .all(|pair| pair[0].slang_start <= pair[1].slang_start),
        "records are in emission order"
    );
    assert!(
        source
            .lines
            .iter()
            .all(|record| (record.rust_line as usize) <= THIS_FILE.lines().count())
    );
}

#[test]
fn descriptors_record_the_defining_file_and_an_ordered_line_table() {
    assert_table_is_recorded(&keyword_local::run::DESCRIPTOR);
    assert_table_is_recorded(&recursive_helper::run::DESCRIPTOR);
    assert_eq!(
        <keyword_local::run as Kernel>::DESCRIPTOR.source,
        keyword_local::run::DESCRIPTOR.source
    );
}

#[test]
fn a_slang_only_syntax_error_in_a_kernel_names_the_rust_line() {
    let expected = marked_line("keyword local");
    for target in [Target::Wgsl, Target::Spirv] {
        let diagnostics = failure(&keyword_local::run::DESCRIPTOR, target);
        let (file, line) = rust_source(&diagnostics);
        assert!(
            file.ends_with("diagnostics.rs"),
            "{target:?}: {diagnostics}"
        );
        assert_eq!(line, expected, "{target:?}: {diagnostics}");
        assert!(
            diagnostics.contains("originating Rust kernel: keyword_local::run"),
            "{target:?}: {diagnostics}"
        );
        assert!(
            diagnostics.contains("originating Rust construct: assignment #"),
            "{target:?}: {diagnostics}"
        );
    }
}

#[test]
fn a_slang_only_recursion_error_in_a_helper_names_the_rust_line() {
    let expected = marked_line("recursion");
    for target in [Target::Wgsl, Target::Spirv] {
        let diagnostics = failure(&recursive_helper::run::DESCRIPTOR, target);
        assert!(
            diagnostics.contains("recursion"),
            "{target:?}: the failure is Slang's recursion rule: {diagnostics}"
        );
        let (file, line) = rust_source(&diagnostics);
        assert!(
            file.ends_with("diagnostics.rs"),
            "{target:?}: {diagnostics}"
        );
        assert_eq!(line, expected, "{target:?}: {diagnostics}");
    }
}

#[test]
fn a_descriptor_without_a_line_table_reports_no_rust_source() {
    let mut descriptor = keyword_local::run::DESCRIPTOR;
    descriptor.source = gust::SourceOrigin::UNKNOWN;
    let diagnostics = failure(&descriptor, Target::Wgsl);
    assert!(
        !diagnostics.contains("originating Rust source"),
        "{diagnostics}"
    );
    // The kernel and construct attribution of DX19 is unchanged.
    assert!(
        diagnostics.contains("originating Rust kernel: keyword_local::run"),
        "{diagnostics}"
    );
}

#[test]
fn an_error_in_an_else_if_condition_names_the_line_of_the_owning_if() {
    let owner = owner_line("else-if chain");
    // The `else if` header is the line above its marker comment.
    let else_if = marked_line("else-if condition") - 1;
    assert_eq!(
        else_if,
        owner + 2,
        "the fixture's else-if sits two lines down"
    );
    let table = else_if_chain::run::DESCRIPTOR.source;
    // The table has no record of its own for the `else if` header: the owning
    // `if` statement's record covers the whole chain, so no record names the
    // else-if's Rust line.
    assert!(
        table.lines.iter().all(|record| record.rust_line != else_if),
        "an `else if` header must not get its own record: {:?}",
        table.lines
    );
    let chain = table
        .lines
        .iter()
        .find(|record| record.rust_line == owner)
        .expect("a record for the owning if");
    assert!(chain.slang_end > chain.slang_start);
    for target in [Target::Wgsl, Target::Spirv] {
        let diagnostics = failure(&else_if_chain::run::DESCRIPTOR, target);
        let (file, line) = rust_source(&diagnostics);
        assert!(
            file.ends_with("diagnostics.rs"),
            "{target:?}: {diagnostics}"
        );
        assert_eq!(line, owner, "{target:?}: {diagnostics}");
        assert_ne!(line, else_if, "{target:?}: {diagnostics}");
        // The reflection path reports the same line.
        let diagnostics = reflect_failure(&else_if_chain::run::DESCRIPTOR, target);
        assert_eq!(
            rust_source(&diagnostics).1,
            owner,
            "{target:?}: {diagnostics}"
        );
    }
}

// ---------------------------------------------------------------------------
// The reflection path (DX19: both compile paths annotate alike).

/// The diagnostics of a failed `reflect::reflect` (it runs `slangc` itself).
fn reflect_failure(kernel: &KernelDescriptor, target: Target) -> String {
    match reflect::reflect(kernel, target) {
        Err(reflect::Error::Compiler(Error::CompilationFailed { diagnostics, .. })) => diagnostics,
        other => panic!("{target:?}: expected a reflect compile failure, got {other:?}"),
    }
}

/// The diagnostics of a failed `reflect::compile_reflected_with_helper` whose
/// helper is a script that fails with a `kernel.slang(<slang_line>)` error.
fn helper_failure(kernel: &KernelDescriptor, target: Target, slang_line: u32) -> String {
    let helper = scripted_failing_helper(slang_line);
    let result = reflect::compile_reflected_with_helper(kernel, target, &helper);
    let _ = fs::remove_file(&helper);
    match result {
        Err(reflect::Error::Compiler(Error::CompilationFailed { diagnostics, .. })) => diagnostics,
        other => panic!("{target:?}: expected a helper compile failure, got {other:?}"),
    }
}

/// An executable that prints a Slang-shaped error at `slang_line` of
/// `kernel.slang` to stderr and exits with status 1, like the real helper does
/// for a Slang compile error.
fn scripted_failing_helper(slang_line: u32) -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let directory = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    fs::create_dir_all(&directory).unwrap();
    let name = format!(
        "scripted-helper-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let message = format!("kernel.slang({slang_line}): error 1: scripted failure");
    #[cfg(windows)]
    {
        let path = directory.join(format!("{name}.bat"));
        fs::write(
            &path,
            format!("@echo off\r\necho {message} 1>&2\r\nexit /b 1\r\n"),
        )
        .unwrap();
        path
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        let path = directory.join(format!("{name}.sh"));
        fs::write(&path, format!("#!/bin/sh\necho '{message}' >&2\nexit 1\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }
}

/// The first Slang line generated from the given Rust line of this file.
fn slang_line_of(kernel: &KernelDescriptor, rust_line: u32) -> u32 {
    kernel
        .source
        .lines
        .iter()
        .rev()
        .find(|record| record.rust_line == rust_line)
        .unwrap_or_else(|| panic!("no record for Rust line {rust_line}"))
        .slang_start
}

fn without_source(kernel: &KernelDescriptor) -> KernelDescriptor {
    let mut descriptor = *kernel;
    descriptor.source = SourceOrigin::UNKNOWN;
    descriptor
}

/// A known origin: the kernel and the Rust source line, the kernel exactly once.
fn assert_known_origin(diagnostics: &str, kernel: &str, rust_line: u32, context: &str) {
    assert_eq!(
        diagnostics.matches("originating Rust kernel: ").count(),
        1,
        "{context}: {diagnostics}"
    );
    assert!(
        diagnostics.contains(&format!("originating Rust kernel: {kernel}")),
        "{context}: {diagnostics}"
    );
    let (file, line) = rust_source(diagnostics);
    assert!(file.ends_with("diagnostics.rs"), "{context}: {diagnostics}");
    assert_eq!(line, rust_line, "{context}: {diagnostics}");
}

/// An unknown origin gets exactly the text from before the line table: the
/// compiler output, then the kernel name as the last line, and no construct or
/// source line. `preceding` are the prefixes of the compiler's trailer lines
/// just before the kernel line, in order.
fn assert_unknown_origin(diagnostics: &str, kernel: &str, preceding: &[&str], context: &str) {
    let lines: Vec<&str> = diagnostics.lines().collect();
    assert_eq!(
        lines.last().copied(),
        Some(format!("originating Rust kernel: {kernel}").as_str()),
        "{context}: {diagnostics}"
    );
    let trailer = &lines[lines.len() - 1 - preceding.len()..lines.len() - 1];
    for (line, prefix) in trailer.iter().zip(preceding) {
        assert!(line.starts_with(prefix), "{context}: {diagnostics}");
    }
    assert_eq!(
        diagnostics.matches("originating Rust").count(),
        1,
        "no construct or source line: {context}: {diagnostics}"
    );
}

#[test]
fn the_reflect_path_names_the_kernel_construct_and_rust_line() {
    let cases = [
        (
            &keyword_local::run::DESCRIPTOR,
            "keyword_local::run",
            marked_line("keyword local"),
        ),
        // The error is in a helper above the first kernel marker, which has a
        // Rust line but no construct: the kernel is still named, once.
        (
            &recursive_helper::run::DESCRIPTOR,
            "recursive_helper::run",
            marked_line("recursion"),
        ),
    ];
    for target in [Target::Wgsl, Target::Spirv] {
        for (kernel, name, expected) in cases {
            let diagnostics = reflect_failure(kernel, target);
            assert_known_origin(&diagnostics, name, expected, &format!("{target:?} {name}"));
        }
        let diagnostics = reflect_failure(&keyword_local::run::DESCRIPTOR, target);
        assert!(
            diagnostics.contains("originating Rust construct: assignment #"),
            "{target:?}: {diagnostics}"
        );
    }
}

#[test]
fn the_reflect_path_without_a_line_table_keeps_the_text_from_before_the_table() {
    for target in [Target::Wgsl, Target::Spirv] {
        for (kernel, name) in [
            (&keyword_local::run::DESCRIPTOR, "keyword_local::run"),
            (&recursive_helper::run::DESCRIPTOR, "recursive_helper::run"),
        ] {
            let diagnostics = reflect_failure(&without_source(kernel), target);
            assert_unknown_origin(
                &diagnostics,
                name,
                &["compiler exit: "],
                &format!("{target:?} {name}"),
            );
        }
    }
}

#[test]
fn the_helper_path_names_the_kernel_construct_and_rust_line() {
    for target in [Target::Wgsl, Target::Spirv] {
        let kernel = &keyword_local::run::DESCRIPTOR;
        let expected = marked_line("keyword local");
        let diagnostics = helper_failure(kernel, target, slang_line_of(kernel, expected));
        assert_known_origin(
            &diagnostics,
            "keyword_local::run",
            expected,
            &format!("{target:?}"),
        );
        assert!(
            diagnostics.contains("originating Rust construct: assignment #"),
            "{target:?}: {diagnostics}"
        );
        assert!(diagnostics.contains("reflection helper: "), "{diagnostics}");

        // A helper above the first kernel marker: a Rust line, but no construct.
        let kernel = &recursive_helper::run::DESCRIPTOR;
        let expected = marked_line("recursion");
        let diagnostics = helper_failure(kernel, target, slang_line_of(kernel, expected));
        assert_known_origin(
            &diagnostics,
            "recursive_helper::run",
            expected,
            &format!("{target:?}"),
        );
    }
}

#[test]
fn the_helper_path_without_a_line_table_keeps_the_text_from_before_the_table() {
    for target in [Target::Wgsl, Target::Spirv] {
        let kernel = &keyword_local::run::DESCRIPTOR;
        // The Slang line comes from the table, which the descriptor then drops.
        let line = slang_line_of(kernel, marked_line("keyword local"));
        let diagnostics = helper_failure(&without_source(kernel), target, line);
        assert_unknown_origin(
            &diagnostics,
            "keyword_local::run",
            &["compiler exit: ", "reflection helper: "],
            &format!("{target:?}"),
        );
        assert!(
            diagnostics.contains("kernel.slang(") && diagnostics.contains("scripted failure"),
            "{target:?}: {diagnostics}"
        );
    }
}
