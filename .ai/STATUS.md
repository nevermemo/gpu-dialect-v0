# Current status

Updated: 2026-09-07. Owner: Kilo (co-maintainer), alongside the human owner; Codex
may collaborate for continuity. Completed scope: documentation, translator
stability/tests, runtime cache/compiler bridge, validation tooling (incl. verify.ps1
stderr capture), T04 (numeric semantics + field-aware struct construction), and T05
(capability probe records + kernel-level Slang→Rust diagnostic mapping). P1 is
complete except T06, which the owner has explicitly deferred. No agent process has
been launched or contacted; claim the next bounded task here before editing.

## This pass (both items complete)

- Bounded tooling fix: `scripts/verify.ps1` `Invoke-Checked` now captures **both**
  stdout and stderr (via temp files) and records them in `VALIDATION.json` as `stdout`
  and a new `stderr` field. Exit-code/throw behavior is unchanged. Why: a failing
  command's diagnostic (cargo/clippy emit on stderr) was previously absent from the
  JSON record, leaving only the generic `"... failed (exit N)"` message. File scope:
  `scripts/verify.ps1` only; no Rust/ABI/translator changes.
  - **Verified end-to-end (2026-09-07, PowerShell 7.6.5):** full `-Full` run passed
    (`VALIDATION.json` `passed: true`, 19 checks exit 0). `slangc -version` now lands
    in `stderr` (record line 33); `cargo clippy`/`cargo test` runner lines captured in
    `stderr` (lines 59, 69); the `µs` timings render correctly (UTF-8 fix confirmed —
    no more `┬╡`). 57 tests, all 5 examples on RTX 5090, 8 SPIR-V artifacts validated.
  - **Environment note:** run the script under **PowerShell 7** (`pwsh`, 7.6.5), not
    Windows PowerShell 5.1. `pwsh` is on PATH; `$PSHOME` confirms 7.6.5.
- **T05 (P1) complete — both slices.** Slice 1: `TargetProbe` record + `probe` in
  `crates/gpu-dialect/src/slang.rs` compiles a known-good minimal kernel to a target
  and records `supported`/`detail` (failing-first tests, WGSL + SPIR-V);
  PORTABLE_SLANG_CORE.md documents the record format. Slice 2: `emit_kernel` writes a
  `// @rust kernel: {module}::{kernel}` marker per kernel; on `slangc` failure the
  bridge parses the reported line and appends the nearest marker's kernel to the
  `CompilationFailed` diagnostic (kernel-level mapping; stable-Rust `proc_macro`
  spans do not expose line numbers). Goldens updated; two mapping tests added.
  Evidence (2026-09-07): core 12 passed, macros 20 passed, wgpu semantics 2 passed
  (RTX 5090). A full `-Full` re-run after the slice 2 commit was started but aborted
  before completion; the last full pass in VALIDATION.json (2026-09-07 13:54 UTC)
  predates the slice 2 commit.

## Verified baseline

- Checkout: `C:\Users\micro\Desktop\gpu-dialect-v0`. Git repository now exists
  (owner-initialized 2026-09-07; branch `main`, 6 commits as of 17:30 local; working
  tree clean except untracked `.agents/`). A Kilo worktree
  `.kilo/worktrees/enchanted-farmhouse` sits on the same commit.
- 34 workspace tests passed before code changes, including actual GPU tests.
- Rust 1.98.0; Slang 2026.13.1-1-g84792eb15; SPIRV-Tools installed.
- Source/artifact hashes before edits: `BASELINE.sha256` (not a content backup).

## Implemented and verified during this pass

- GUST documentation retains the working GPU Dialect subsystem and records future
  semantic frontend, execution graph, portability tiers, and ECS destination.
- Tail helper returns, numeric literal lowering, binary grouping, and boolean/integer
  negation; unsafe positional struct literals now fail explicitly.
- Reviewed complete Slang golden, diagnostic unit tests, real-GPU semantics fixture.
- Explicit typed locals and fail-closed signature/attribute/builtin diagnostics.
- Entrypoint-aware cache identity, proven with two entrypoints sharing one source
  and different GPU results; owned temporary directories, cleanup failure tests,
  compiler diagnostic capture and SPIR-V structure validation.
- Final full script: formatting and strict Clippy passed; **57 tests passed**
  (53 unit/integration + 4 compile-fail documentation tests), zero failed/ignored.
- All five examples ran on NVIDIA GeForce RTX 5090 via native Vulkan wgpu; all
  eight exported SPIR-V kernels passed `spirv-val --target-env vulkan1.2`.
- All eight artifact sets regenerated. `VALIDATION.json` records 19 successful
  commands and 8 validated SPIR-V hashes. Its scope is local debug verification,
  not release benchmarking or browser/Metal/DXIL certification.
- Deliberate missing-tool test confirmed the verification script records failure
  and stops; the final successful full run replaced that failure report.
- T04 (this pass): numeric semantics proven on a real GPU (signed/unsigned div/mod,
  float→int casts, overflow wrap) against an independent host reference; integer
  div/rem by a literal zero diagnosed at the Rust boundary; field-aware struct
  construction lowered to construct-then-assign (field identity + source order),
  supported as a `let` initializer and assignment RHS and rejected elsewhere. New
  `numeric` fixture (golden + GPU test) and macro regression tests.
- T05 (this pass): capability probe records (`TargetProbe` + `probe`, failing-first
  WGSL/SPIR-V tests, PORTABLE_SLANG_CORE.md record format) and kernel-level
  Slang→Rust diagnostic mapping (`// @rust kernel:` markers + nearest-marker lookup
  appended to `CompilationFailed`); goldens updated, two mapping tests.
- `scripts/verify.ps1`: `Invoke-Checked` captures stderr in addition to stdout and
  records both in `VALIDATION.json` (`stdout` + new `stderr` field). Failure records
  now carry the actual compiler/test diagnostic, not just the exit code. Behavior
  (stop on first failure, always write the JSON record) is unchanged.

## Current executable scope

Direct syn → Slang; Rust private shadow checking; WGSL → Vulkan wgpu; SPIR-V export.
Five runnable examples; scalar/nested padding-free typed storage; shader/layout/
pipeline cache; persistent buffers; dependent batches; async jobs; optional timestamps.
Bindings/layouts are manual macro metadata, not Slang reflection.

No rustc semantic frontend, graph compiler, ECS, renderer, indirect dispatch, dynamic
capacity growth, browser validation, or DXIL/Metal execution. Reflection is deferred.
Known correctness debt: implicit inference versus Rust-resolved types, evaluation
order/alias analysis, full name hygiene, and line-level Slang-to-Rust source maps
(kernel-level mapping exists; line-level needs non-stable spans). Numeric edge
semantics and field-aware struct construction are proven in bounded contexts (T04).
Resource helper parameters and shader struct literals are explicitly rejected. Concurrent cache misses may duplicate
compilation. See `NEXT_TASKS.md` for bounded follow-up.

## Allowance and handoff

The owner requested stopping new work near 10% remaining in either allowance window.
Reliable starting readings were 88% five-hour / 29% weekly. No reset credit is
authorized. Final handoff readings are recorded in `HANDOFF.md`; never infer live
account state from this historical note.
