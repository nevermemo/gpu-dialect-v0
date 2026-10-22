# GUST handoff

## Current state

- Branch: `main`
- Active: none claimed; see [STATUS.md](STATUS.md).
- DX19 sidecar diagnostic maps are implemented and committed. Full validation and
  independent review are still pending per
  [tasks/DX19-sidecar-diagnostic-maps.md](tasks/DX19-sidecar-diagnostic-maps.md).
- T13 GPU active-count proof is deferred by owner. Its work in progress is the git
  stash `WIP T13 active count deferred`, not the working tree. Do not apply,
  continue, validate, commit, or drop it without explicit direction.
- Last committed compiler feature: T12 statement Result match. Latest work
  (2026-10-23): owner approvals, VALIDATION.json redaction and untracking, history
  path purge, repository renamed to `nevermemo/gust`, Unit-tier CI.

## Pickup

1. Read `.ai/AGENT_CONTEXT.json`.
2. Read the active section of [STATUS.md](STATUS.md).
3. Read [NEXT_TASKS.md](NEXT_TASKS.md) and the matching task contract.
4. Run `git status --short --branch`, `git stash list`, and the contract's focused
   check.

## Evidence

- Last release-level project record: `cargo xtask check-full` passed at
  2026-09-09T12:10:13Z on NVIDIA GeForce RTX 5090 / Vulkan, with 13 SPIR-V
  artifacts validated. It predates the DX19 commit.
- DX19 focused bridge evidence: `cargo test -p gust --lib slang` and
  `cargo test -p xtask` pass.
- `cargo xtask check-fast` exited 0 on 2026-10-22 at `63769fb` with no tracked
  changes (untracked `.kilo/` only). Routine modes write no `.ai/VALIDATION.json`.
- The stale T13-derived `generated-wgpu/pool__classify.*` exports were moved out of
  the tree on 2026-10-23, so `check-full` sees its expected 13 `.spv` files again.

Historical handoff records are in
[history/HANDOFF-2026-09-09.md](history/HANDOFF-2026-09-09.md).
