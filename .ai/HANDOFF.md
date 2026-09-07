# GUST handoff — 2026-09-07

Ownership released. The repository is ready for a local AI to resume manually.
No local model endpoint was configured, no other agent was started, and no account
reset was used. Read AGENTS, STATUS, NEXT_TASKS, then DECISIONS before claiming work.

## What changed and why

1. Re-anchored README and docs around GUST while retaining the working GPU Dialect
   crate APIs. The vision, compiler evolution, execution graph/CPU domains,
   portability tiers, and ECS destination are clearly separate from implemented code.
   Added decisions, ranked roadmap, agent agreement, and durable status/tasks.
2. Reproduced and fixed implicit helper-return loss, Rust numeric-suffix emission,
   binary precedence mismatch, and boolean versus integer `!`. Added typed local
   declarations. The typed-pipeline helpers now use ordinary Rust tail returns.
3. Replaced unproven positional shader struct construction with an explicit error.
   Whole-struct buffer copies and field updates remain supported. Added early
   diagnostics for kernel contracts, duplicate/zero workgroups, dropped attributes,
   malformed builtin receivers, resource-shadowing, generic structs, and ref patterns.
4. Corrected cache identity to include entrypoint. A GPU test alternates two
   entrypoints sharing one compilation unit and verifies outputs 11/22 plus two
   cache misses and two hits.
5. Hardened the compiler bridge with exclusive temp-directory creation and owned
   cleanup on all return paths (best effort if deletion is OS-blocked), both diagnostic
   streams, and SPIR-V structure checks. Tests cover collisions, compiler failure,
   early I/O failure, and malformed binary output.
6. Added reviewed full Slang golden, shared shader/GPU semantics fixture, four
   compile-fail documentation tests, and `scripts/verify.ps1`. The script stops on
   errors and emits `.ai/VALIDATION.json`; a deliberate missing-tool check was tested.
7. Hardened `scripts/verify.ps1` `Invoke-Checked` to capture **stderr** as well as
   stdout (redirected to temp files) and record both in `VALIDATION.json` (`stdout`
   plus a new `stderr` field). Previously a failing command's diagnostic — which
   cargo/clippy emit on stderr — was missing from the JSON record, leaving only the
   generic `"... failed (exit N)"` message. Exit-code/throw behavior and the
   stop-on-first-failure / always-write-record contract are unchanged.

## Important files

- Frontend: `crates/gpu-dialect-macros/src/{slang,validate,expand,regression_tests}.rs`.
- Bridge/contract: `crates/gpu-dialect/src/{slang,descriptor,abi,lib}.rs`.
- Cache/tests: `crates/gpu-dialect-wgpu/src/lib.rs`, `tests/semantics.rs` under that crate.
- Shared expectations: `tests/fixtures/semantics.{rs,slang}` and `numeric.{rs,slang}`.
- Validation: `scripts/verify.ps1`, `.ai/VALIDATION.json`.
- Documentation: README, AGENTS, `docs/`, and `.ai/`.
- Derived output: all 8 kernels in `generated-wgpu/` regenerated via the 5 examples;
  shader grouping and typed-pipeline source changed. Some binary/WGSL outputs remain
  byte-identical because Slang optimizes equivalent expressions.
- `.ai/CHANGED_FILES.md` inventories added/modified files against `BASELINE.sha256`.
  No Git repository, commits, backups, or ZIP were created; hashes are not rollback data.

## Verification evidence

Baseline before edits: 34 passing tests. Final: **57 passing tests**, zero failed
or ignored, formatting check and strict Clippy clean. All five runnable programs
passed on NVIDIA GeForce RTX 5090 (native Vulkan wgpu). All eight exported SPIR-V
modules passed external Vulkan 1.2 validation. WGSL is the execution route; SPIR-V
validation is separate. The generated `.rs` files are readable host snippets, not
independently compiled standalone packages.

Tool versions: Rust 1.98.0; Slang 2026.13.1-1-g84792eb15;
SPIRV-Tools v2026.3.rc1-0-gb707790a. Final local evidence is timestamped in
VALIDATION.json (successful command records, 8 validated artifact hashes). Each check
record now carries `stdout` and `stderr` so a failing command's diagnostic is
preserved in the record. Debug example timings are smoke evidence, not a CPU/GPU
performance conclusion. Browser WebGPU, DXIL, Metal, other vendors, and the
manifest's older Rust minimum are unverified.

The stderr-capture patch was then **verified end-to-end** on 2026-09-07 under
**PowerShell 7.6.5** (`pwsh`; not Windows PowerShell 5.1): a full `-Full` run passed
(`VALIDATION.json` `passed: true`, 19 checks exit 0). `slangc -version` now lands in
`stderr` (record line 33); `cargo clippy`/`cargo test` runner lines are captured in
`stderr` (lines 59, 69); the `µs` timings render correctly (UTF-8 fix confirmed — no
more `┬╡`). **Environment note for future runs: use `pwsh` (PowerShell 7), not 5.1.**

## Deferred and known risks

- Reflection remains deferred by the owner. Metadata is macro-assigned, not Slang
  reflection. No wide vectors/matrices/uniform/texture ABI was introduced.
- No rustc semantic integration, execution-graph compiler, ECS/renderer, dynamic
  pool growth, indirect execution, CPU fallback, or custom shader instruction IR.
- Explicit local types help but do not provide full Rust inference. Numeric edge
  behavior, effect/evaluation order, aliases, and complete identifier hygiene remain
  open. Shader struct literals and resource-valued helpers fail explicitly until
  their lowering is proven. Source maps still point at generated Slang, not Rust.
- Concurrent first cache misses can compile twice; this is not a single-flight cache.
- Full future capability probing and graph/engine APIs remain planned, not completed.

## Recommended pickup

T04 (numeric semantics + struct construction) is complete: the `numeric` fixture
locks the emitted Slang, a real-GPU test proves signed/unsigned div/mod, float→int
casts, and struct field identity + source order, integer div/rem by a literal zero is
diagnosed at the Rust boundary, and struct literals lower to construct-then-assign.
See the T04 evidence block in NEXT_TASKS.md and `docs/ARCHITECTURE.md`.

**T05 (P1) is now claimed** — first slice is capability probe records (a `TargetProbe`
+ `probe` in `crates/gpu-dialect/src/slang.rs` with a failing-first test); diagnostic
source mapping (Slang→Rust spans) is the next T05 slice. The remaining S1 work is the
T05 source-mapping slice and the negative-diagnostics coverage (T03). Do not bundle
reflection (T06, deferred by the owner) or an ECS framework (T08) into that work.

Start from the workspace root:

```powershell
.\scripts\verify.ps1 -Full
cargo test -p gpu-dialect-macros regression_tests
cargo test -p gpu-dialect-wgpu --test semantics
```

Do not blindly regenerate golden expectations. Review emitted Slang and actual GPU
results; retain the independent host expectations. Record task/file ownership in
STATUS and exact verification results before the next handoff.

## Allowance policy

The owner asked Codex to reserve the final portion of either allowance window for
stabilization and handoff near 10% remaining. Reliable starting readings were 88%
five-hour and 29% weekly. No reset credit was used. The final account snapshot is
added below; it is historical and does not constrain a future local AI's runtime.

Final handoff checkpoint: **12% five-hour / 17% weekly remaining**. New implementation
stopped at this natural milestone near the requested 10% reserve; only inventory,
documentation checks, and the final response followed. No reset credit consumed.
