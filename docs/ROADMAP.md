# Roadmap

This is a dependency-ordered direction, not a schedule or a claim of completed v1.
Runnable next-task acceptance criteria live in [NEXT_TASKS](../.ai/NEXT_TASKS.md).

## S0 — Preserve and re-anchor (completed 2026-09-07)

Verify baseline; retain all five examples and GPU features; document actual GUST
scope, portability, execution domains, engine destination, and agent workflow.
Deliver status, handoff, baseline hashes, a repeatable full verification command,
and reviewed generated artifacts. No engine implementation claimed.

## S1 — Translation/runtime stability (active priority)

- Golden translation plus GPU semantics regression coverage: first fixture implemented.
- Fix reproduced silent translation errors; reject unsupported constructs with spans.
- Entrypoint cache identity and compiler process/artifact lifecycle: hardened.
- Close signature/attribute/receiver validation gaps; explicit typed locals before
  richer inference: initial diagnostic slice and typed locals implemented. Further
  inference/effect semantics remain open. Preserve short-circuit and evaluation-order contracts.
- Add negative compile tests and mapped diagnostic evidence; do not auto-bless goldens.

## S2 — Portable contract and language breadth (planned)

Machine-readable target probes and ABI regression matrix, then source mapping,
small vector/math families with operator coverage, and field-aware struct creation.
Reflection remains deferred but becomes a gate for broad uniforms/textures/samplers,
parameter groups, and specialized layouts. Any artifact disk cache needs source,
compiler identity, options, and dependencies in its key. Additional native targets
need their own compilation and execution evidence.

## S3 — Explicit execution model (planned)

Extract one staged CPU/GPU resource-flow proof from typed-pipeline. Declare accesses
and dependencies explicitly; measure transfers and residency. Add validated indirect
dispatch and bounded continuation work only with clear ordering/termination rules.
Then consider rustc semantic integration, richer Rust lowering, and automatic
dependency/effect inference. Slang still owns shader codegen.

## S4 — Engine-shaped proofs (planned)

Typed component-pool growth by GPU copy; two read/write-declared systems; active-count
indirect execution; culling/compaction into indirect rendering. Test logical length
versus allocation capacity, binding refresh, IDs/generations, safe retirement, and
native capability alternatives. Choose ECS storage based on measured workloads.

## Long term — ECS game engine (aspiration)

GPU-resident physics/animation/hierarchy/visibility/geometry/material systems,
intentional CPU input/gameplay/assets, and portable plus native-optimized rendering.
No claim of a complete game engine until an actual user can run an agreed game or
scene without developer help.
