# Test layers and fixtures

Use the existing test layout; GUST has no trybuild harness and does not need one.

- Translation goldens and rejection tests live in `crates/gust-macros/src/tests/regression.rs`. A golden test translates a fixture from `tests/fixtures/` and compares the Slang text with the reviewed `.slang` file. A rejection test calls the same `translate` helper on a small `mod bad { ... }` source and asserts that the error message contains the intended text.
- Source that rustc must reject even though the macro accepts it is a `compile_fail` doctest in `crates/gust/src/lib.rs`.
- GPU behavior tests are integration tests in `crates/gust-wgpu/tests/`, and ignored example tests run under `cargo xtask check-examples`.

Each rejection case should fail for the intended dialect error, not an unresolved name or an unrelated Rust error: assert on the message, not only on `is_err()`. Put the unsupported syntax nested inside otherwise accepted constructs as well as at the top level. A `compile_fail` doctest passes for any compile error, so keep it minimal and check what it rejects.

Source assertions should check semantic structure (operations, operands, branch, bounds guard, bindings) without overfitting whitespace. A snapshot proves stability, not shader validity. Compile the emitted artifact, not a separately maintained golden shader. Negative tests should exercise unsupported syntax nested inside otherwise accepted constructs, mutation of read-only buffers and name shadowing. A golden changes only with a reviewed semantic diff and a passing GPU test; never bless it from the failure output.

For arithmetic/control-flow changes choose distinct inputs whose expected outputs cannot equal zero initialization. Include length 1, workgroup size-1, workgroup size, workgroup size+1 and multiple groups plus a remainder. Test empty input and mismatched lengths under the documented host contract. Use deterministic seeds. Compare exact integers and justified float error bounds; define NaN/infinity handling explicitly. Expected values come from an independent host reference, never from the shadow bodies.

`cargo xtask check-format` runs `cargo fmt --all -- --check`, `cargo xtask check-lints` runs Clippy over the workspace and all targets with warnings denied, and `cargo xtask check-workspace` runs the workspace tests with the example packages excluded, including doctests. Clippy and the tests pass `--locked`, so a stale `Cargo.lock` fails instead of being rewritten. None of them selects `--all-features`, because features may be mutually exclusive. A successful workspace run can still contain ignored GPU tests; run a single one with `cargo xtask test-exact -p <package> --test <target> <name>`.

Source checked 2026-09-07: [cargo test](https://doc.rust-lang.org/cargo/commands/cargo-test.html).
