# Next tasks

This is the active queue only. Completed narrative is archived in
[history/NEXT_TASKS-2026-09-09.md](history/NEXT_TASKS-2026-09-09.md); feature
contracts live in [tasks/](tasks/README.md).

Claim one bounded task in [STATUS.md](STATUS.md) before editing. A task is not
active until its owner, scope, and focused check appear there.

The ordered plan is [ROADMAP.md](../ROADMAP.md). Its "Now and next" table lists the
claimable items of the current milestone; this file keeps deferred work and task detail
that does not fit there.

## Deferred

### T13: GPU active count to indirect dispatch

Deferred by owner. The T13 work in progress is not in the working tree: it is the
git stash `WIP T13 active count deferred`, based on `59a5c3c`. Its code changes
(`examples/component-pool/src/app.rs`, `xtask/src/{checks,verify}.rs`) applied
cleanly to `63769fb` in a 2026-10-22 dry run; its doc renames are superseded by
that commit and conflict. Do not apply, continue, or drop the stash unless the owner
explicitly requests it. Contract: [tasks/T13-active-count.md](tasks/T13-active-count.md).

## Claimable DX

### DX20: Internal ownership splits

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
