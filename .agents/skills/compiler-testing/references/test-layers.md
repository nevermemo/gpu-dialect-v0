# Test layers and fixtures

Use the existing test layout. For a new trybuild harness the conventional pattern is:

```rust
#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/pass/*.rs");
    t.compile_fail("tests/ui/fail/*.rs");
}
```

Ensure both globs actually match fixtures. Each rejection case should fail for the intended dialect error, not an unresolved import or unrelated Rust error. Commit reviewed `.stderr` expectations beside failing fixtures. trybuild writes new diagnostics into `wip`; `TRYBUILD=overwrite` updates expectations in place. Use that only for an intentional diagnostic update, then review the diff and rerun without overwrite. Do not set it globally. Pin rustc when stable diagnostic snapshots matter.

Source assertions should check semantic structure (operations, operands, branch, bounds guard, bindings) without overfitting whitespace. A snapshot proves stability, not shader validity. Compile the emitted artifact, not a separately maintained golden shader. Negative tests should exercise unsupported syntax nested inside otherwise accepted constructs, mutation of read-only buffers and name shadowing.

For arithmetic/control-flow changes choose distinct inputs whose expected outputs cannot equal zero initialization. Include length 1, workgroup size-1, workgroup size, workgroup size+1 and multiple groups plus a remainder. Test empty input and mismatched lengths under the documented host contract. Use deterministic seeds. Compare exact integers and justified float error bounds; define NaN/infinity handling explicitly.

`Test-Compiler.ps1` requires Cargo.lock and runs `fmt --check`, Clippy with warnings denied, workspace/all-target tests and doctests sequentially. Use its package/features arguments where needed. It never runs `--all-features` implicitly because features may be mutually exclusive. It rejects inherited TRYBUILD=overwrite. A successful workspace run can still contain ignored GPU tests; use the exact headless-test helper for those.

Sources checked 2026-09-07: [trybuild](https://docs.rs/trybuild/latest/trybuild/), [cargo test](https://doc.rust-lang.org/cargo/commands/cargo-test.html).
