//! Slang failures that rustc and the GUST validator accept must still point at the
//! Rust line the faulty statement was written on (DX19 line table).
//!
//! Both fixtures below are valid Rust and pass validation, so only `slangc` can
//! reject them: a local named `new` is a Slang syntax error where it is used, and
//! a self-recursive helper is "recursion not allowed". The expected Rust line is
//! found by searching
//! this very file for a marker comment, so no line number is written down by hand.
//! Needs `slangc` on `PATH`.

use gust::{
    Kernel, KernelDescriptor,
    slang::{self, Error, Target},
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

/// The 1-based line of this file whose code ends with the `// FAILS: <what>` marker.
fn marked_line(what: &str) -> u32 {
    let marker = format!("// FAILS: {what}");
    let line = THIS_FILE
        .lines()
        .position(|line| line.trim_end().ends_with(&marker))
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
