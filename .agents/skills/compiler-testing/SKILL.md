---
name: compiler-testing
description: "Add or diagnose compiler regression tests, trybuild compile-pass/compile-fail fixtures, span diagnostics, emitted Slang assertions, or CPU/GPU differential tests. Use when validating compiler changes; not as a mandatory full test sweep for documentation edits."
---

# Compiler testing

Choose the smallest test that can disprove the change, then expand only across affected compiler boundaries. Keep the locked toolchain and dependency versions.

1. Validator: accepted fixture plus single-cause compile-fail fixture with a meaningful source span.
2. Emitter: source -> expected Slang semantics, then actual slangc compilation for supported targets.
3. Runtime: independent CPU expected values against GPU readback, including partial workgroups.
4. Run focused tests before workspace checks. Never regenerate expected diagnostics automatically to make a test pass.

Read [test layers](references/test-layers.md) for trybuild and regression selection. Run [Test-Compiler.ps1](scripts/Test-Compiler.ps1) for locked formatting, Clippy and workspace tests. It reports Cargo checks only; it does not certify a GPU run or that every intended fixture exists.
