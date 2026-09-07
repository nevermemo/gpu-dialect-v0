# Current status

## `Option<T>` deterministic lowering to Slang `Optional<T>` — COMPLETE (2026-09-07)

Ownership released. Owner was GitHub Copilot (VS Code agent) at the user's request.
The owner chose to represent `Option<T>` in Slang instead of banning std-prelude type
names (D14; supersedes the allowlist suggestion in NEXT_TASKS T03). This is the
VISION "deterministic lowering" row for `Option`; `Result`, `match`, `?`, and payload
enums stay out of scope. `f64`/`u64`/8-bit primitives are NOT emulated: WGSL has no
such scalars and the storage ABI stays four-byte (D08/D09).

Files: `crates/gpu-dialect-macros/src/{validate,slang,regression_tests}.rs`, new
`tests/fixtures/option.{rs,slang}` (reviewed golden), new
`crates/gpu-dialect-wgpu/tests/option.rs`, `docs/{ARCHITECTURE,DECISIONS}.md`,
`.ai/` records. No runtime, ABI, dependency, or signing changes.

Evidence before editing (slangc 2026.13.1, WGSL + SPIR-V + `spirv-val`, all exit 0):
Slang `Optional<T>` supports `none`, implicit `T -> Optional<T>` returns, generic
`Optional<T> __gust_some<T>(T)` and `T __gust_unwrap_or<T>(Optional<T>, T)` helpers,
reassignment, `Optional<bool|int|uint|struct>`, `.hasValue` on call results, and a
`var __gust_opt_N = e; if (__gust_opt_N.hasValue) { var x = __gust_opt_N.value; ... }`
shape for `if let`. WGSL lowers it to a `{ value, hasValue }` struct.

Implemented (failing-first: `option_golden` and `option_unsafe_forms_are_rejected`
both failed against `f3ec044`): validator accepts `Option<T>` (T = supported scalar or
module struct) in locals and helper params/returns, `Some(x)`, `None`, `.is_some()`,
`.is_none()`, `.unwrap_or(d)`, `if let Some(x) = e { } else { }`; rejects Option in
struct fields and resource element types, nested/resource payloads, `.unwrap()`/
`.expect()`, non-`Some(ident)` patterns, let chains (11 rejection cases). Emitter maps
`Option`→`Optional`, emits the two generic helpers when any Option feature is used,
and lowers `if let` through a per-statement `__gust_opt_{index}` temporary (an
`else if let` gets its own scope).

Verification (Rust 1.98.0, Slang 2026.13.1, RTX 5090/Vulkan): `cargo fmt --check`
exit 0; `cargo clippy --workspace --all-targets -D warnings` exit 0; `cargo test
--workspace` **90 passed, 0 failed, 0 ignored** (85 unit/integration + 5 doctests;
baseline 86). Existing goldens unchanged. GPU test compiles both targets and matches
an independent host `Option` reference at 1/63/64/65/257 elements across presence
tests, `unwrap_or`, `if let` on a local and on a call result, `Option<Pair>`, and
helpers returning/accepting Options. `vector-add` and `typed-pipeline` exit 0; no
derived-artifact drift. Independent read-only review: **PASS**, no findings; its
hand-traced reference values for x = 0, 3, 7, 8, 1001 match.

Next command: T07 (first explicit staged graph proof) per NEXT_TASKS and
`docs/EXECUTION_GRAPH.md` "First bounded proof"; claim it here before editing.

## Validator frontier — cast targets, primitive types, const blocks (2026-09-07)

Ownership released. Owner was GitHub Copilot (VS Code agent) at the user's request.
Bounded scope, as claimed: the T03 remaining frontier only. Files changed:
`crates/gpu-dialect-macros/src/validate.rs`, `crates/gpu-dialect-macros/src/regression_tests.rs`,
one `compile_fail` doctest in `crates/gpu-dialect/src/lib.rs`, a limits paragraph in
`docs/ARCHITECTURE.md`, and the `.ai/` records. No emitter, runtime, ABI, dependency,
or signing changes. Committed on top of `3fd0aac` in two scopes (below) and pushed
to `origin/main` at the user's request; unsigned like `3fd0aac` (no signing configured).

What changed (failing-first; all three new tests failed against `3fd0aac`):
- `visit_expr_cast`: `as` targets must be a single identifier in
  `f32`/`float`, `i32`/`int`, `u32`/`uint`. Previously `id.x as f64` translated to
  `f64(id.x)`, passed rustc (`uint` is a plain `u32` alias), and failed only in slangc.
- `visit_type_path`: Rust primitives outside the four-byte subset (`i8`..`i128`,
  `isize`, `u8`..`u128`, `usize`, `f16`, `f64`, `f128`, `char`, `str`) are rejected in
  every type position (let annotations, helper params/returns, struct fields, buffer
  element types). `let v: f64 = 1.0;` previously became `f64 v = 1.0;` — accepted by
  rustc and the macro, failing only at pipeline creation.
- `visit_expr_const`: inline `const { .. }` blocks are rejected by name. They were
  already stopped by the emitter's generic "not in the subset yet" fallback; the
  validator now owns the diagnostic, consistent with the other constructs.
- `bool` stays a supported type but is not a cast target (rustc rejects numeric→bool;
  bool→integer casts are unproven on the GPU).

Verification (Rust 1.98.0, Slang 2026.13.1, SPIRV-Tools v2026.3, RTX 5090/Vulkan):
`cargo fmt --all -- --check` exit 0; `cargo clippy --workspace --all-targets -D warnings`
exit 0; `cargo test --workspace` **86 passed, 0 failed, 0 ignored** (81 unit/integration
+ 5 doctests; baseline 82). Both golden fixtures unchanged. `cargo run -p vector-add`
and `cargo run -p typed-pipeline` exit 0; no further derived-artifact drift.
Independent read-only review of this change: **PASS** — traversal reaches all type
positions, no escape to `emit_type`, no test passes for the wrong reason. Its design
note: the primitive rule is a denylist (exhaustive over Rust's finite primitive set),
but std-prelude type names such as `Option<uint>` still pass the validator and fail
only in slangc. That sibling gap is pre-existing and recorded in NEXT_TASKS T03.

Queue state found at pickup: the working tree was clean and the formerly pending
T06 slice 1 + quality patch is committed as `3fd0aac` ("T06 Continued", unsigned,
author = owner) and pushed to `origin/main`. The signing-key blocker recorded below
no longer gates that work; HEAD `3fd0aac`, 82 workspace tests passing at pickup.

Independent review of the quality patch (`d99d176..3fd0aac`, macro `slang.rs`,
`validate.rs`, core `slang.rs` diagnostics): **PASS**. A separate read-only reviewer
traced both `emit_struct_value` call sites (one per statement, fresh `enumerate`
per block), found no `Expr::Struct` reaching emission unrejected, and found no
diagnostic input that maps to a *wrong* kernel (only `None`). A hand-written Slang
unit re-declaring `__gust_struct_0` in nested blocks compiled on WGSL and SPIR-V
and passed `spirv-val`, so per-block temporary indices are valid Slang. One
low-severity note: nested struct literals (`Pair { a: Inner { .. } }`) are rejected
by the emitter, not the validator — correct behavior, pre-existing, not a regression.
The "do not label these patches independently approved" caveat below is now lifted.

`scripts/verify.ps1 -Full` re-run under pwsh 7.6.5 after `3fd0aac` (before this
validator change): **GUST verification passed**, 19 checks exit 0, five examples on
NVIDIA GeForce RTX 5090, all eight SPIR-V exports validated. `.ai/VALIDATION.json`
refreshed. The run regenerated four stale derived exports (`particles__snapshot`,
`particles__step`, `polynomial__evaluate`, `signal_pipeline__transform` `.slang`):
each differs only by the T05 `// @rust kernel:` marker line; WGSL/SPIR-V unchanged.

Commits: `chore(artifacts): refresh VALIDATION.json and stale kernel-marker exports`
(VALIDATION.json + four `generated-wgpu/*.slang`) and `feat(validate): fail closed on
cast targets, non-32-bit primitives, and const blocks` (validate.rs,
regression_tests.rs, lib.rs doctest, ARCHITECTURE.md, .ai records).

Next command: claim one bounded task in this file before editing — either the
std-prelude type-name allowlist frontier (NEXT_TASKS T03) or T06 slice 2
(authoritative aggregate size/alignment/stride evidence via `scripts/probes/`).

## Checkpoint and T06 investigation (2026-09-07)

Owner: Codex at the user's request. Preserve the existing dirty checkpoint at
`d99d176`. Scope: full validation (VALIDATION.json and derived example exports),
independent review if available, bounded Slang layout probes under `target/`,
and durable investigation/status/roadmap notes. No runtime ABI expansion or new
dependencies. Signed commits still await the owner's existing public-key path.
Independent read-only checkpoint review: PASS, no blocking findings. Probe Builder
owns reproducible native Slang API sources under `scripts/probes/`; build outputs
remain under `target/t06-layout/`. This is investigation tooling, not a Rust runtime
dependency or broader supported GPU type contract.

## Quality review completed (2026-09-07)

Ownership released. Codex coordinated Scout/Builder work at the user's request. Baseline
is the existing uncommitted T06 slice on `main` at `d99d176`; 74 workspace tests
passed before this review. Preserve those edits. Bounded review scope: recent
T03/T04 macro changes and T05 compiler diagnostics; targeted regression-backed
fixes plus status/handoff/architecture corrections. No dependencies, ABI expansion,
signing changes, or commit in this review. Core diagnostics Builder owns
`crates/gpu-dialect/src/slang.rs`; Scout is read-only in macro sources.
Scout found struct self-assignment and dropped `..base` correctness gaps. Workers
then hit an allowance limit; Codex finished locally. Expanded bounded fix scope:
macro `slang.rs`, `validate.rs`, `regression_tests.rs`, numeric golden, and new
wgpu struct-assignment regression. Preserve all pre-existing T06 changes.

Findings fixed:
- T04 wrote struct fields into the destination while still reading the RHS. Real
  GPU regression failed: `(11, 101)` swapped to `(101, 101)`, not `(101, 11)`.
  Build a fresh reserved-name temporary in source field order, then assign once.
  This also preserves outer-variable reads in shadowing initializers. Reviewed
  numeric golden changed only to temporary construction and whole-value assignment.
- T04 accepted `Pair { first: 9, ..old }` but silently omitted copied fields.
  Failing-first validator regression now rejects record updates explicitly.
- T05 mapping missed actual Slang 2026.13.1 multiline `error[E...]` / arrow locations,
  accepted lookalike filenames, and could attribute a warning instead of an error.
  Legacy and current format tests plus actual failing compilation on both targets
  now cover correct kernel mapping and invalid/unrelated locations.

Verification: baseline 74 tests; final `cargo test --workspace` **82 passed,
0 failed, 0 ignored** (78 unit/integration + 4 doctests). Formatting check and
strict workspace/all-target Clippy exit 0. New struct-assignment tests compile
WGSL/SPIR-V and execute WGSL on NVIDIA RTX 5090/Vulkan at 1/63/64/65/257 elements.
`cargo run -p vector-add` and `cargo run -p typed-pipeline` exit 0; derived exports
unchanged. No full `verify.ps1 -Full` rerun; existing VALIDATION.json is historical.

Review limitation: Scout identified the frontend bugs and Builder contributed
diagnostic regressions, but worker allowance failures prevented an independent
review of the final patches. Codex performed final diff review and deterministic
checks; do not label these patches independently approved. No commit or signing
configuration change. Next step: independent review of this quality delta, then
resolve the existing signing-key-path blocker before the pending commits.

Updated: 2026-09-07. T06 slice 1 implementation ownership released after verification.
Files: core `reflect.rs`, `lib.rs`, reflection tests, and T06 status/architecture/
decision/handoff notes. Delegated Builder and independent Verifier are finished;
Codex integrated and ran workspace checks. Signed commit is pending the owner's
existing signing public-key path. No local model endpoints were contacted.
Prior completed work owner: Kilo (co-maintainer); Codex
may collaborate for continuity. Completed scope: documentation, translator
stability/tests, runtime cache/compiler bridge, validation tooling (incl. verify.ps1
stderr capture), T04 (numeric semantics + field-aware struct construction), T05
(capability probe records + kernel-level Slang→Rust diagnostic mapping), and the T03
second slice (broader name resolution + effect analysis at unsupported frontend
boundaries). P1 is complete except T06. The owner has now **lifted the T06 deferral**
(2026-09-07); T06 slice 1 adds compiler JSON inspection and partial layout checks.
This continuation implements that bounded slice; no wider ABI expansion.

## This pass (T06 slice 1 — compiler-authoritative layout reflection)

T06 was deferred; the owner authorized this bounded first slice. Added
`crates/gpu-dialect/src/reflect.rs`: dependency-free strict JSON parser,
`Reflection::from_json`, target-specific `reflect`, recursive field/resource records,
and `cross_check_pod::<T>`. Added the public module/re-exports in `lib.rs` and
`crates/gpu-dialect/tests/reflection.rs`. The existing public `TemporaryDirectory`
from baseline commit `d99d176` is reused; no changes to `slang.rs` or dependencies.

Actual Slang 2026.13.1 WGSL and SPIR-V JSON reports field offsets/sizes and resource
binding indices/access, but omits aggregate size, alignment, and storage-buffer
element stride. These remain `None`/unverified in `PodCrossCheck`; `Ok` does not
mean complete ABI proof. A field's `elementStride: 0` is not buffer stride.
Macro descriptors, wgpu uploads, and the four-byte storage ABI remain unchanged.
The wider T06 milestone is not complete.

Baseline: clean `main` at `d99d176`; `cargo test --workspace` 68 passed. Tools:
Rust 1.98.0 (88d9e12ae), Slang 2026.13.1-1-g84792eb15; GPU NVIDIA GeForce RTX 5090.
New API test failed first with expected unresolved imports. Existing vector-add
and typed-pipeline programs both exit 0 on RTX 5090 after integration; generated
artifacts unchanged.

Final verification (2026-09-07): `cargo fmt --all -- --check` exit 0;
`cargo clippy --workspace --all-targets -- -D warnings` exit 0;
`cargo test --workspace` **74 passed, 0 failed, 0 ignored** (70 unit/integration,
4 doctests). New evidence: strict JSON grammar/Unicode/depth tests; actual nested
WGSL/SPIR-V reflection; negative offset/size/name/count/scalar/stride comparisons;
binding space/access; compiler failures; failing-first rejection of unsupported
entry-point resource parameters. Independent Verifier: **PASS**, including a
separate run of all 5 reflection integration tests. No full `verify.ps1 -Full`
run this slice; `.ai/VALIDATION.json` remains the prior full-run record.

Commit blocker: the owner chose signing setup before commit, then chose an existing
key but has not supplied its `.pub` path. No signing configuration was changed and
no unsigned commit was made. HEAD remains `d99d176483a0c9bb9be42ae7f31879eb5238f54c`;
all task edits are uncommitted. Next step: obtain the signing public-key path,
configure repository-local SSH signing, verify signer availability, then commit
the scoped files with `feat(reflect): add Slang JSON reflection and POD cross-checks`.

## This pass (T03 second slice — name resolution + effect analysis)

Fail-closed the gap between what the validator accepted and what the translator can
lower. Two classes of diagnostic added in `crates/gpu-dialect-macros/src/validate.rs`:

- **Name resolution.** A bare path *value* must be a single identifier (a local or
  parameter). Multi-segment paths are associated constants or foreign items (`f32::
  INFINITY`, `u32::MAX`, `core::f32::consts::PI`) that the translator would emit as
  invalid Slang (`f32.INFINITY`). New `visit_expr_path` rejects them; callee paths are
  unaffected because `visit_expr_call` only descends for single-identifier helpers.
  `core` added to `BANNED_NAMES` so `core::` in path *types* is also rejected.
- **Effect analysis.** New `visit_expr_try` rejects the `?` operator and
  `visit_expr_unsafe` rejects `unsafe` blocks — both previously accepted by the
  validator and only caught later by the translator's generic "not in the subset"
  error. `asm!` needs no new rule: syn 2.0 parses it as `Expr::Macro`, already
  rejected by the existing macro rule.

Failing-first tests added in `regression_tests.rs`: `unsupported_name_paths_are_rejected`
(4 cases) and `unsupported_effects_are_rejected` (3 cases). Verified 2026-09-07:
`cargo fmt --check` clean; `cargo clippy --workspace --all-targets -- -D warnings`
clean; `cargo test --workspace` **68 passed, 0 failed** (66 prior + 2 new, incl. 5
example binaries on RTX 5090 + 4 compile-fail doc tests); `cargo run -p vector-add`
and `cargo run -p typed-pipeline` both exit 0 on the RTX 5090. Known remaining
frontier (not claimed here): cast target types and `const` blocks are still accepted
by the validator and only fail at Slang compile time.

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
   (RTX 5090). Full `cargo test --workspace` re-run after the slice 2 commit:
   **66 passed, 0 failed** (62 unit/integration incl. 5 example binaries on RTX 5090,
   + 4 compile-fail doc tests). A full `verify.ps1 -Full` (fmt/clippy/examples/SPIR-V)
   re-run after the slice 2 commit was not completed; the last full pass in
   VALIDATION.json (2026-09-07 13:54 UTC) predates the slice 2 commit.

## Verified baseline

- Checkout: `C:\Users\micro\Desktop\gpu-dialect-v0`. Git repository exists
  (owner-initialized 2026-09-07; branch `main`, 9 commits as of this update; working
  tree clean, `.agents/skills/` fully tracked). A Kilo worktree
  (`.kilo/worktrees/enchanted-farmhouse`) sits on the same commit.
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
- T03 second slice (this pass): fail-closed name resolution and effect analysis —
  `visit_expr_path` rejects multi-segment bare path values (associated constants /
  foreign items like `f32::INFINITY`, `u32::MAX`), `core` added to `BANNED_NAMES`,
  `visit_expr_try` rejects `?`, `visit_expr_unsafe` rejects `unsafe` blocks. Two
  failing-first regression tests (7 cases total). 68 tests pass, fmt/clippy clean,
  both examples run on RTX 5090.
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
Resource helper parameters, shader struct literals, multi-segment path values
(associated constants / foreign items), the `?` operator, and `unsafe` blocks are
explicitly rejected. Cast target types and `const` blocks are still accepted by the
validator and only fail at Slang compile time. Concurrent cache misses may duplicate
compilation. See `NEXT_TASKS.md` for bounded follow-up.

## Allowance and handoff

The owner requested stopping new work near 10% remaining in either allowance window.
Reliable starting readings were 88% five-hour / 29% weekly. No reset credit is
authorized. Final handoff readings are recorded in `HANDOFF.md`; never infer live
account state from this historical note.
