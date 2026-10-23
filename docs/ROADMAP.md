# Roadmap

This is the ordered plan the project follows. It was agreed with the owner on
2026-10-23. Milestones run in order; work inside a milestone may run in parallel
lanes. Decisions behind it are D21–D27 in [DECISIONS](DECISIONS.md). Claimable task
detail lives in [NEXT_TASKS](development/NEXT_TASKS.md) and task contracts in
[tasks/](development/tasks/README.md). Current state and evidence live in
[STATUS](development/STATUS.md).

## How to use this file

- Read **Now and next** first, including after a context compaction.
- Status words are facts: `planned`, `ready`, `in progress`, `done (<tier>, <commit>)`.
  A status never moves to `done` without a run record at the stated evidence tier
  (see [VALIDATION](development/VALIDATION.md)).
- Change the order or content of milestones only with an owner decision recorded in
  DECISIONS.
- No kill rules are attached for now (owner, 2026-10-23); scope is reviewed at
  milestone boundaries instead.

## Direction

GUST is a published, compute-only crate for writing GPU code in Rust: "Slang with Rust
syntax and rustc checking". It covers as much Rust syntax as can be lowered with a
written contract, and brings GPU concepts (vector types, SIMT dispatch, workgroup
memory, capability-specific code) into Rust. The target hardware and platform are
chosen at build time; GPU and CPU builds follow one semantics contract (D21, D22).
GUST is the foundation for two later projects: Breeze, a rendering and interaction
framework, and Hurricane, the game engine (today's
[ENGINE_NORTH_STAR](ENGINE_NORTH_STAR.md)). Their scope is defined when GUST is ready.

## Now and next

**Now: M0, close-out and housekeeping.**

| Item | Work | Status |
| --- | --- | --- |
| M0.1 | DX19 close-out: `cargo xtask check-full` and an independent review | done (Gpu: check-full RTX 5090 at `71f8d71`; review fixes `027eef7` with check-fast; two Fable reviews) |
| M0.2 | Replace the six PowerShell skill scripts with xtask commands (`slangc`, `spirv-val`, `doctor --smoke`, `test-exact`; `--locked` on existing checks); delete `Test-VerticalSlice.ps1`; fix the stale `.ps1` reference in `crates/gust/src/reflect/mod.rs` | done (Unit, 3ae37e2; check-fast Gpu RTX 5090) |
| M0.3 | Lint that fails when a `.ps1`, `.bat`, `.cmd` or `.sh` file is tracked (D27) | done (Unit, 3ae37e2; check-fast Gpu RTX 5090) |
| M0.4 | `cargo xtask doctor` exits non-zero when a required tool is missing | done (Unit, 3ae37e2; check-fast Gpu RTX 5090) |
| M0.5 | Artifact check compares expected file names, not a count | done (Unit, 3ae37e2; check-fast Gpu RTX 5090) |
| M0.6 | `cargo xtask docs-check`: relative links, conflict markers, decision ids | done (Unit, efc4c2a; check-fast Gpu RTX 5090) |
| M0.7 | Machine-path lint for tracked files | done (Unit, 3ae37e2; check-fast Gpu RTX 5090) |
| M0.8 | Explicit `BLESS_GOLDEN=1` for reviewed goldens, never automatic | done (Unit, efc4c2a; check-fast Gpu RTX 5090) |
| M0.9 | `cargo xtask check-arch`: no custom shader IR (dependency allowlist, no IR modules outside a `future/` path) | done (Unit, efc4c2a; check-fast Gpu RTX 5090) |
| M0.10 | Suites named by evidence tier: `cargo xtask test --suite Unit\|Slangc\|Spirv\|Reflect\|Gpu\|Full` over existing commands | done (Unit, efc4c2a; check-fast Gpu RTX 5090) |
| M0.11 | Thin `CLAUDE.md`: read AGENTS.md first, role mapping, compaction instructions, skills list | done (docs) |
| M0.12 | Skill fixes: GUST does not use trybuild; "host reference" instead of "CPU oracle"; vertical-slice skill points at `check-artifacts`/`check-full` | done (Unit, 3ae37e2) |
| M0.13 | DECISIONS tidy: header with next id, trailing prose moved under a heading | done (docs) |
| M0.14 | Independent review of the 2026-10-23 `xtask` redaction and CI changes | in progress |
| M0.16 | Bounded retry for a transient Vulkan `NoAdapter` in `HeadlessDevice::new`; `NoAdapter` stays a failure, never a skip (seen twice in parallel atomics tests, 2026-10-23) | done (Gpu: check-fast RTX 5090; review keep) |
| M0.17 | DX19 scanner follow-ups: `else` prefix matching, balanced braces in conditions, `match` label numbering, contract note on the heuristic | done (Gpu: check-fast RTX 5090; review keep) |
| M0.18 | Slice B review fixes: check-arch reads root `[workspace.dependencies]` renames; `--locked` wording in VALIDATION.md or flags on Reflect/Spirv suites; xtask `rust-version`; nits | done (Unit, 5503e5a; Codex review: keep after fixes -> M0.20) |
| M0.19 | No silent GPU skips: examples fail on `NoAdapter`; `else if` header braces; `match` ordinal scope in the DX19 contract; retry visibility | ready |
| M0.20 | check-arch: quoted TOML keys and basic-string escapes (Codex review of M0.18) | ready |
| M0.15 | Make the CI `unit` job a required check after a week of green runs (from 2026-10-30) | planned |

**Next: M1, diagnostics.**

## Milestones

| Milestone | Content | Status |
| --- | --- | --- |
| M0 Close-out and housekeeping | See Now and next | in progress |
| M1 Diagnostics | `#line` markers so Slang errors point at Rust file and line (extends DX19); stable diagnostic codes; `#[diagnostic::on_unimplemented]` on shadow-type traits; original tokens kept beside `compile_error!` | planned |
| M2 Typed host API | Typed per-kernel dispatch, designed backend-neutral so the same call runs on the GPU or a CPU twin; const `offset_of!`/`size_of` layout assertions; StagedGraph deriving dependencies from typed reads and writes; launch-limit checks | planned |
| M3 Dual test builds | Executable shadow types under the semantics contract (D22); CPU dispatch; CPU twins emitted under `cfg(test)`; `--cfg gust_dual` for cross-crate tests and benchmarks; per-kernel opt-out (`#[gpu(cpu = "unsupported")]`); existing GPU tests gain CPU-versus-GPU checks while semantics fixtures keep independent host references (D26) | planned |
| M4 Build-time targets and distribution | Build profiles (target plus capability set) chosen at build time; hash-keyed artifact manifest; D17 at export time for WGSL and SPIR-V; runtime `slangc` behind a development feature; a pinned-Slang manager for crate users; first browser WebGPU run | planned |
| M5 CPU target and evidence breadth | Production `cpu` build target and runtime; lavapipe CI job labelled `Gpu (software)`; differential program generator with shrinking; known-mismatch file; scalar `coretests` cases on the GPU | planned |
| M6 Language breadth I | Vectors and matrices (vec3 and mat3 rejected in buffers at first; `m * v` lowers to `mul`); collision-only identifier renaming; uniforms (`ConstantBuffer<T>`) | planned |
| M7 GPU concepts in Rust | `par_for!` SIMT dispatch on both pipelines; workgroup shared memory and barriers, including a CPU strategy; reduce, scan and compaction primitives (T13 lands here) | planned |
| M8 Capability variants | Capability-keyed variants with platform aliases; exhaustiveness checked against the crate's declared target list, with CPU as a target; one rustc-checked signature; subgroup operations first (D23) | planned |
| M9 Language breadth II | Methods in `impl` blocks; user enums with `match`; tuples; fixed-size arrays; non-capturing closures; then traits mapped to Slang interfaces and generics | planned |
| M10 Publication | Benchmarks against rayon, hand-written WGSL with wgpu, CubeCL and rust-gpu; a "Rust program to GPU" example gallery; docs and a book; API stability pass; crate naming (D25); crates.io release | planned |

A first public release may be cut after M7 so that publishing does not wait for M8
and M9; that cut is an owner decision when M7 is done.

## Parking lot

- GPU builds with a CPU fallback (D21 leaves this open as a future option).
- WebGL2 (out of scope for GUST compute, D24; revisit for Breeze).
- Slang automatic differentiation.
- An optional nightly-pinned THIR checker behind a cargo subcommand.
- Per-API SPIR-V, MSL and DXIL builds for native-only capabilities.
- Breeze and Hurricane scope.

## Completed foundation (before 2026-10-23)

- **S0, preserve and re-anchor (2026-09-07):** baseline verified, scope documented,
  repeatable full verification.
- **S1, translation and runtime stability:** golden translation and GPU semantics
  fixtures; silent translation errors fixed; entrypoint cache identity and compiler
  process lifecycle hardened.
- **S2, portable contract and language breadth (partial):** target probes,
  kernel-level diagnostics, bounded struct construction, `Option`/`Result`, bounded
  loops, atomics; reflection gates every StorageV1 pipeline (D17).
- **S3, explicit execution model (first proof):** `StagedGraph` with checked explicit
  dependencies and transfer reports; validated indirect dispatch.
- **S4, engine-shaped proofs:** `GpuPool<T>` with GPU-copy growth and GPU-derived
  indirect arguments.

Details are in [history/COMPLETED_TASKS](development/history/COMPLETED_TASKS.md).
