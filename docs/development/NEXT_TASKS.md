# Next tasks

This is the active queue only. Completed narrative is archived in
[history/NEXT_TASKS-2026-09-09.md](history/NEXT_TASKS-2026-09-09.md); feature
contracts live in [tasks/](tasks/README.md).

Claim one bounded task in [STATUS.md](STATUS.md) before editing. A task is not
active until its owner, scope, and focused check appear there.

## Deferred

### T13: GPU active count to indirect dispatch

Deferred by owner. Preserve existing uncommitted T13 work; do not continue it
unless explicitly requested. Contract: [tasks/T13-active-count.md](tasks/T13-active-count.md).

## Claimable DX

### D19: Sidecar diagnostic maps — ACTIVE

Owner: GitHub Copilot. Replace kernel-only Slang diagnostic attribution with a generated sidecar map
that relates Slang line ranges to stable Rust construct labels. Keep direct syn
AST to Slang emission; do not introduce a custom shader IR. First contract must
specify the artifact schema, stable labels, failure mapping, fixture coverage,
and public/private artifact policy. Contract:
[tasks/D19-sidecar-diagnostic-maps.md](tasks/D19-sidecar-diagnostic-maps.md).

### D20: Internal ownership splits

Split the largest modules only where a named responsibility can move without
changing public APIs: macro validation by concern, macro regressions by feature,
or wgpu runtime internals by ownership. Use a no-behavior-change contract and
one focused check per move. Start with extracting the Result/Result-match
regressions from `crates/gust-macros/src/tests/regression.rs` into a feature-owned
test module; do not mix that mechanical move with behavior changes.

## Later engine proofs

- GPU active count to indirect dispatch (T13), when the owner reactivates it.
- Culling -> compacted visibility -> indirect arguments -> rendering.
- Entity identity/lifecycle and storage choices, guided by measured workloads.

## Deferred language/runtime breadth

Vectors/matrices, uniforms, textures, samplers, additional binding groups, and
specialized layouts each require their own reflected ABI evidence. `while`,
inclusive ranges, general match expressions, `?`, and broader Result APIs need
explicit semantic contracts before implementation.
