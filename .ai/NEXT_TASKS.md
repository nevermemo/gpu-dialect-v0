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

## T03 — P0: Fail closed at unsupported frontend boundaries

Status: first slice complete. Goal: actionable errors for invalid signatures, options, attributes,
and resource helpers. Why: syntactic acceptance currently exceeds proven lowering.
Files: macro validate/expand/slang and negative fixtures. Dependencies: T01.
Done: reject duplicate/missing invocation, non-unit kernels, zero/duplicate workgroups,
semantic attributes that are dropped, malformed builtin receivers/arguments; include
compile-fail evidence (implemented). Broader name resolution and effect analysis
remain separate work. Verify: macro unit tests and `cargo test --workspace`.

## T04 — P1: Resolved numeric semantics and struct construction

Status: typed locals implemented; numeric/effect semantics and struct construction are the recommended next task.
Goal: explicit typed locals plus field-aware construction with preserved
evaluation order, or clear diagnostics. Why: syn inference is not rustc inference;
integer division/overflow/casts and effect order remain correctness risks.
Files: macro translator/shadows plus fixtures. Dependencies: T01/T03.
Done: signed/unsigned edge tests, inference counterexamples, reordered struct fields,
and side-effect-order checks; no public claim beyond proven cases.
Verify: golden, compile-fail, Slang WGSL/SPV, GPU readback tests.

## T05 — P1: Capability/ABI evidence and diagnostic source mapping

Status: local validation JSON implemented; cross-target probes/source mapping planned.
Goal: machine-readable probe records and useful Rust locations for
Slang errors. Why: target claims and layouts must follow evidence. Files: proposed
probe utility, core bridge, macro source metadata, docs/PORTABLE_SLANG_CORE.md.
Dependencies: T02/T03. Done: versions/options/adapter captured, unsupported targets
explicit, scalar/nested ABI matrix, known Slang failure mapped to originating Rust.
Verify: repeat probes and intentional failing input; no silent skips.

## T06 — P1, deferred by owner: Slang reflection / broader shadows

Goal: compiler-authoritative layouts before uniforms/textures/samplers/vectors expand
the runtime ABI. Why: current hand-classified metadata is only a bootstrap contract.
Files: core reflection/ABI, macro descriptors, wgpu bindings. Dependencies: stability
milestones and explicit reconsideration of owner's deferral. Done: positive and
negative target layout comparisons, no blanket POD assumption. Verify: reflection
fixtures and actual GPU upload/readback. Do not start merely because it is listed.

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
