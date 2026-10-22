# GUST handoff

## Current state

- Branch: `main`, HEAD `1c7a81c`
- Active: none claimed; see [STATUS.md](STATUS.md).
- D19 sidecar diagnostic maps are implemented and committed. Full validation and
  independent review are still pending per
  [tasks/D19-sidecar-diagnostic-maps.md](tasks/D19-sidecar-diagnostic-maps.md).
- T13 GPU active-count proof is deferred by owner. Its work in progress is the git
  stash `WIP T13 active count deferred`, not the working tree. Do not apply,
  continue, validate, commit, or drop it without explicit direction.
- Last committed compiler feature: T12 statement Result match. Latest commit: D19
  diagnostic sidecars and task history.

## Pickup

1. Read `.ai/AGENT_CONTEXT.json`.
2. Read the active section of [STATUS.md](STATUS.md).
3. Read [NEXT_TASKS.md](NEXT_TASKS.md) and the matching task contract.
4. Run `git status --short --branch`, `git stash list`, and the contract's focused
   check.

## Evidence

- Last release-level project record: `cargo xtask check-full` passed at
  2026-09-09T12:10:13Z on NVIDIA GeForce RTX 5090 / Vulkan, with 13 SPIR-V
  artifacts validated. It predates the D19 commit.
- D19 focused bridge evidence: `cargo test -p gust --lib slang` and
  `cargo test -p xtask` pass.
- `cargo xtask check-fast` exited 0 on 2026-10-22 at `1c7a81c` with no tracked
  changes (untracked `.kilo/` only). Routine modes write no `.ai/VALIDATION.json`.
- Known full-check blocker: ignored `generated-wgpu/` holds T13-derived
  `pool__classify.*` exports, so it has 14 `.spv` files and `check-artifacts` /
  `check-full` will fail their exact-13 count. See [STATUS.md](STATUS.md).

Historical handoff records are in
[history/HANDOFF-2026-09-09.md](history/HANDOFF-2026-09-09.md).
