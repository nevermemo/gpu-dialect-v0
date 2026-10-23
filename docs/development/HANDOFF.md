# GUST handoff

## Current state

- Branch: `main`
- Active: roadmap M1 (diagnostics), slice by slice; see the "Now and next" section of
  [ROADMAP.md](../ROADMAP.md) and [STATUS.md](STATUS.md).
- M0 is closed (D28). Its only open item is M0.15, making the CI `unit` job a required
  check after a week of green runs (from 2026-10-30).
- T13 GPU active-count proof is deferred by owner. Its work in progress is the git
  stash `WIP T13 active count deferred`, not the working tree. Do not apply,
  continue, validate, commit, or drop it without explicit direction.
- Last committed compiler feature: T12 statement Result match.

## Pickup

1. Read `.ai/AGENT_CONTEXT.json`, AGENTS.md, and (for Claude Code) CLAUDE.md.
2. Read the "Now and next" section of [ROADMAP.md](../ROADMAP.md) and the active
   section of [STATUS.md](STATUS.md).
3. Read the matching task contract or plan.
4. Run `git status --short --branch`, `git stash list`, and `cargo xtask check-fast`.

## Evidence

- Last release-level record: `cargo xtask check-full` passed at
  2026-10-23T13:24:07Z on NVIDIA GeForce RTX 5090 / Vulkan at `8200431` (workspace
  tests, release-mode slang tests, 8 examples, 13 SPIR-V artifacts validated).
- CI `unit` job green on every push to main on 2026-10-23.
- Evidence tiers and labels: [VALIDATION.md](VALIDATION.md).

Historical handoff records are in
[history/HANDOFF-2026-09-09.md](history/HANDOFF-2026-09-09.md).
