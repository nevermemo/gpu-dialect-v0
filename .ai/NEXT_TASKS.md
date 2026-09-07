# Next tasks

Claim one bounded task in STATUS before editing. Verify repository state; this list
is not authority to contact external agents or change account settings.

## T01 — P0: Translation regression foundation

Status: first slice complete. Goal: preserve helper returns, literals, grouping, and
negation. Why: reproduced wrong/invalid Slang from accepted Rust. Files:
`crates/gpu-dialect-macros/src/{slang,regression_tests,validate}.rs`,
`tests/fixtures/semantics.*`, `crates/gpu-dialect-wgpu/tests/semantics.rs`.
Dependencies: installed Slang and Vulkan adapter for integration.
Done: failing-before tests, reviewed golden, both target compilers, real GPU results
at 1/63/64/65/257 elements. Verify: `cargo test --workspace`.

## T02 — P0: Runtime cache and compiler bridge hardening

Status: complete. Goal: correct entrypoint identity and owned temporary-file cleanup.
Why: inspection found cache key omits entrypoint; predictable directory reuse and
early error paths weaken compiler isolation. Files: wgpu `src/lib.rs`, core
`src/slang.rs`, targeted tests. Dependencies: none beyond baseline tools.
Done: regression proves two entries in one source cannot share the wrong pipeline;
exclusive temp creation, cleanup on failures, bounded artifact validation.
Verify: targeted core/wgpu tests, then `scripts/verify.ps1 -Full`.

## T03 — P0: Fail closed at unsupported frontend boundaries — COMPLETE

Status: complete (both slices, 2026-09-07). Goal: actionable errors for invalid
signatures, options, attributes, and resource helpers. Why: syntactic acceptance
currently exceeds proven lowering.
Files: macro validate/expand/slang and negative fixtures. Dependencies: T01.
Slice 1 (done): reject duplicate/missing invocation, non-unit kernels, zero/duplicate
workgroups, semantic attributes that are dropped, malformed builtin receivers/arguments;
compile-fail evidence.
Slice 2 (done this pass): broader name resolution and effect analysis.
`visit_expr_path` rejects multi-segment bare path values (associated constants / foreign
items like `f32::INFINITY`, `u32::MAX`, `core::f32::consts::PI`); `core` added to
`BANNED_NAMES`; `visit_expr_try` rejects `?`; `visit_expr_unsafe` rejects `unsafe`
blocks. `asm!` already covered (syn 2.0 parses it as `Expr::Macro`). Failing-first
tests: `unsupported_name_paths_are_rejected` (4 cases) and
`unsupported_effects_are_rejected` (3 cases).
Slice 3 (done 2026-09-07): type-name frontier. `visit_expr_cast` limits
`as` targets to `f32`/`float`, `i32`/`int`, `u32`/`uint`; `visit_type_path` rejects
Rust primitives outside the four-byte subset in every type position (`let v: f64`,
`fn helper(x: u64)`, `-> usize`, struct fields, `RWStructuredBuffer<i64>`);
`visit_expr_const` rejects inline `const { .. }` blocks by name. Failing-first tests:
`casts_to_unproven_targets_are_rejected`, `non_32_bit_primitive_types_are_rejected_before_slang`,
`const_blocks_are_rejected`; one `compile_fail` doctest in core `lib.rs`.
Remaining frontier (not claimed): std-prelude type names are not banned, so
`fn maybe() -> Option<uint> { None }` passes the validator and rustc and emits
`Option<uint> maybe()`, failing only in slangc (verified by probe 2026-09-07). A
fix needs an allowlist of type names (prelude + module structs) per the validator
contract, plus a test-order update for `unsupported_effects_are_rejected`.
Verify: `cargo test -p gpu-dialect-macros` and `cargo test --workspace`.

## T04 — P1: Resolved numeric semantics and struct construction — COMPLETE

Status: complete. Numeric semantics proven and a literal-zero div/rem diagnostic
added; field-aware struct construction implemented in bounded contexts.
Goal: explicit typed locals plus field-aware construction with preserved
evaluation order, or clear diagnostics. Why: syn inference is not rustc inference;
integer division/overflow/casts and effect order remain correctness risks.
Files: macro translator/shadows plus fixtures. Dependencies: T01/T03.
Done: signed/unsigned edge tests, inference counterexamples, reordered struct fields,
and side-effect-order checks; no public claim beyond proven cases.
Verify: golden, compile-fail, Slang WGSL/SPV, GPU readback tests.

Evidence (2026-09-07, slangc 2026.13.1-1-g84792eb15, Vulkan adapter present):
- `numeric` fixture golden (`tests/fixtures/numeric.slang`) locks the emitted Slang.
- `numeric` GPU test proves signed/unsigned div/mod, float→int casts, and struct
  field identity + source order against an independent host reference.
- Integer div/rem by a literal zero is rejected at the Rust boundary (Slang makes it
  a compile error; rustc treats it as a runtime panic).
- Struct literals lower to construct-then-assign by name in source order; supported
  as a `let` initializer and assignment RHS, rejected elsewhere.

## T05 — P1: Capability/ABI evidence and diagnostic source mapping — COMPLETE

Quality-review follow-up (2026-09-07): real compiler errors exposed the current
multiline diagnostic format, now covered alongside legacy line/column locations.
T04 follow-up builds struct RHS values in fresh temporaries before writing the
destination and explicitly rejects record updates (`..base`). GPU regressions
cover local/buffer swaps and shadowing at workgroup boundaries. Final independent
review is outstanding after worker allowance failures; see STATUS.

Status: complete (2026-09-07). Both slices done: capability probe records and
kernel-level Slang→Rust diagnostic mapping.
Goal: machine-readable probe records and useful Rust locations for Slang errors.
Why: target claims and layouts must follow evidence. Files:
`crates/gpu-dialect/src/slang.rs`, `crates/gpu-dialect-macros/src/slang.rs`,
`tests/fixtures/{numeric,semantics}.slang`, docs/PORTABLE_SLANG_CORE.md.
Dependencies: T02/T03.
Slice 1: `TargetProbe` record + `probe` in `crates/gpu-dialect/src/slang.rs`
compiles a known-good minimal kernel to a target and records `supported`/`detail`;
failing-first tests (WGSL + SPIR-V); PORTABLE_SLANG_CORE.md documents the record
format.
Slice 2: `emit_kernel` writes a `// @rust kernel: {module}::{kernel}` marker before
each kernel; on `slangc` failure the core bridge parses the reported line and
appends `originating Rust kernel: {module}::{kernel}` (nearest marker at or before
that line) to the `CompilationFailed` diagnostic. Kernel-level, not line-level:
stable-Rust `proc_macro` spans do not expose line numbers. Golden fixtures updated
with the marker; two tests cover mapping with a marker and `None` without.
Evidence (2026-09-07): `cargo test -p gpu-dialect --lib` 12 passed (incl. both
mapping tests); `cargo test -p gpu-dialect-macros` 20 passed (marker goldens);
`cargo test -p gpu-dialect-wgpu --test semantics` 2 passed on RTX 5090.
Verify: repeat probes and intentional failing input; no silent skips.

## T06 — P1, authorized: Slang reflection / broader shadows

Goal: compiler-authoritative layouts before uniforms/textures/samplers/vectors expand
the runtime ABI. Why: current hand-classified metadata is only a bootstrap contract.
Owner lifted the deferral on 2026-09-07. Slice 1 adds the core JSON parser,
`reflect`, and explicit `cross_check_pod` API with positive/negative comparisons.
Compiler JSON supplies field offsets/sizes and resource bindings. The installed
compiler omits aggregate size/alignment/storage stride for the tested resources;
coverage reports must not claim those properties verified.

Remaining: obtain authoritative aggregate size, alignment, and stride evidence
before enforcing reflection at runtime or expanding shadows. Do not infer these
from host metadata. Files for later work: core reflection/ABI, macro descriptors,
wgpu bindings. Full T06 completion still requires complete target layout checks
and actual GPU upload/readback; slice 1 alone does not replace the bootstrap ABI.

## T07 — P2: First explicit staged graph proof

Status: planned. Goal: CPU settings → two GPU stages → small summary, with inspectable
dependencies and transfer sizes. Why: bridge current batches toward GUST execution.
Files: new bounded example plus minimal runtime API if needed. Dependencies: S1 stable.
Done: correct output, intermediate residency, invalid dependency failure, no automatic
inference claim. Verify: runnable example and ordering/transfer tests.

## T08 — P2: Engine component pool / indirect workload proofs

Status: planned. Goal: GPU data survives host-driven capacity growth without resize
readback, then active-count-driven indirect dispatch. Why: concrete ECS prerequisites.
Files: typed buffer runtime and a small component-pool example. Dependencies: T07 and
specified logical length/capacity/retirement contract. Done: zero/growth/stale binding/
in-flight cases and output parity; capability checks for indirect work.
Verify: full suite plus boundary tests and measured copy/transfer evidence.
