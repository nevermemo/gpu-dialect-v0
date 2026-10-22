# Current status

## Active: none claimed (2026-10-22)

The DX context reduction, D19 sidecar diagnostic maps, and the `slang/source_map.rs`
ownership split are committed (`1c7a81c`). D19 still needs the full validation and
independent review its contract requires. T13 remains deferred by owner; its work in
progress is the git stash `WIP T13 active count deferred`, not the working tree. Do
not apply, continue, validate, commit, or drop it without explicit direction.

```text
owner: none
claim: none
next_focused_check: cargo xtask check-fast
full_check_needed_before_commit: no
```

## Current baseline

- Branch: `main`, HEAD `1c7a81c`
- Last complete compiler feature: T12, statement-only `match Result<T, T>`
- Last full validation: `cargo xtask check-full` passed at 2026-09-09T12:10:13Z
  on NVIDIA GeForce RTX 5090 / Vulkan; 13 SPIR-V artifacts passed `spirv-val`.
  It predates `1c7a81c`.
- Latest routine check: `cargo xtask check-fast` exited 0 ("GUST verification
  passed") on 2026-10-22 at `1c7a81c` with no tracked changes (untracked `.kilo/`
  only). Routine modes write no `.ai/VALIDATION.json` record.
- Known full-check blocker: ignored `generated-wgpu/` still holds `pool__classify.*`
  exports produced by the T13 work, so it has 14 `.spv` files and
  `cargo xtask check-artifacts` / `check-full` will fail their exact-13 count. Those
  four derived files must be removed with owner approval, or T13 must land with a
  deliberate count update, before full validation can pass.
- Open task queue: select and claim a bounded task from
  [NEXT_TASKS.md](NEXT_TASKS.md)

## Current contracts

- Task acceptance criteria: [tasks/README.md](tasks/README.md)
- T10 atomics: [tasks/T10-atomics.md](tasks/T10-atomics.md)
- T11 Result lowering: [tasks/T11-result.md](tasks/T11-result.md)
- T12 Result match: [tasks/T12-result-match.md](tasks/T12-result-match.md)
- T13 active count (deferred): [tasks/T13-active-count.md](tasks/T13-active-count.md)
- D19 sidecar maps: [tasks/D19-sidecar-diagnostic-maps.md](tasks/D19-sidecar-diagnostic-maps.md)

## Latest DX evidence

- 2026-10-22 documentation sync, no code changes: README, ARCHITECTURE,
  ENGINE_NORTH_STAR, NEXT_TASKS, HANDOFF, and the Copilot builder agent profile now
  record D19 as committed with release evidence pending, point T13 at its stash, and
  list atomics (T10), Option/Result (D14), and bounded struct literals (D07) as
  supported. An independent read-only review checked each added claim against the
  validator, emitter, regressions, fixtures, GPU tests, and xtask (PASS);
  `git diff --check` passed.
- `cargo xtask check-feature core` passed (19 core tests), including sidecar-map
  schema/line-range coverage and construct-enriched compiler diagnostics.
- `cargo test -p xtask` passed (3 routing tests).
- `cargo xtask check-feature result` passed (6 macro plus 4 GPU/target tests).
- `cargo fmt --all -- --check` and `git diff --check` passed.
- D19 writes temporary `kernel.map.json` files with stable Slang line/construct
  segments and appends mapped kernel/construct labels to Slang failures. Focused
  `cargo test -p gust --lib slang` passed (14).
- Full validation was not refreshed after D19 because T13 changes were then in the
  working tree. They are now stashed; the `generated-wgpu/` blocker above remains.

## Archive

Completed task reports and superseded ownership records are preserved in
[history/STATUS-2026-09-09.md](history/STATUS-2026-09-09.md).
