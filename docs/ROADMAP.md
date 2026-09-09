# Roadmap

This is a dependency-ordered direction, not a schedule or a claim of completed v1.
Runnable next-task acceptance criteria live in [NEXT_TASKS](development/NEXT_TASKS.md).

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

## S2 — Portable contract and language breadth (partially implemented)

Target probes, kernel-level diagnostic attribution, bounded field-aware struct
creation, local/helper `Option<T>` lowering, and exclusive-range loops are implemented
(T04–T06, T09–T11). Reflection gates every StorageV1 pipeline through the native
helper (D17, T06). Still pending: fine-grained source maps and small vector/math
families. The
broad uniforms/textures/samplers, parameter groups, and specialized layouts must
extend that reflected evidence before entering the runtime contract. Any artifact
disk cache needs source, compiler identity, options, and dependencies in its key.
Additional native targets need their own compilation and execution evidence.

## S3 — Explicit execution model (first proof implemented)

T07 supplies `StagedGraph` and `examples/staged-graph`: host settings upload, two
ordered GPU stages, summary readback, checked explicit dependencies and transfer/
residency reports. T08 adds validated indirect-dispatch resources. These are explicit
host APIs, not compiler-inferred graphs or CPU execution nodes. Still pending:
continuations, transfer planning, automatic dependency/effect inference and rustc
semantic integration. Slang still owns shader codegen.

## S4 — Engine-shaped proofs (pool and indirect compute implemented)

T08 supplies a typed vector-like component pool with GPU-copy growth, logical length
versus capacity, retirement, and GPU-derived indirect compute arguments. The count
starts from a host-mirrored length; this is not yet GPU compaction or an ECS.
Still pending: atomic GPU count generation, culling/compaction, indirect rendering,
entity IDs/generations and native capability alternatives. Choose ECS storage based
on measured workloads rather than treating this pool as the final engine layout.

## Long term — ECS game engine (aspiration)

GPU-resident physics/animation/hierarchy/visibility/geometry/material systems,
intentional CPU input/gameplay/assets, and portable plus native-optimized rendering.
No claim of a complete game engine until an actual user can run an agreed game or
scene without developer help.
