# GUST handoff

## Current state

- Branch: `main`
- Active: D19 sidecar diagnostic maps; see [STATUS.md](STATUS.md) and
  [tasks/D19-sidecar-diagnostic-maps.md](tasks/D19-sidecar-diagnostic-maps.md)
- T13 GPU active-count proof is deferred by owner. Preserve its existing worktree
  changes; do not continue, validate, commit, or alter that work without explicit
  direction.
- Last committed feature: T11/T12 Result lowering and statement Result match.

## Pickup

1. Read `.ai/AGENT_CONTEXT.json`.
2. Read the active section of [STATUS.md](STATUS.md).
3. Read [NEXT_TASKS.md](NEXT_TASKS.md) and the matching task contract.
4. Run `git status --short --branch` and the contract's focused check.

## Evidence

- Last release-level project record before current DX work:
  `cargo xtask check-full` passed at 2026-09-09T12:10:13Z on NVIDIA GeForce RTX
  5090 / Vulkan, with 13 SPIR-V artifacts validated.
- D19 focused bridge evidence: `cargo test -p gust --lib slang` and
  `cargo test -p xtask` pass.

Historical handoff records are in
[history/HANDOFF-2026-09-09.md](history/HANDOFF-2026-09-09.md).
