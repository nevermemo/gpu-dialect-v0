# Current status

## Active: roadmap M1 (2026-10-23)

The roadmap agreed with the owner is in [ROADMAP.md](../ROADMAP.md). M0 closed on
2026-10-23 after a two-reviewer sanity check (D28): every M0 item is done except M0.15,
which waits for a week of green CI (from 2026-10-30). M1 (diagnostics):
M1.0 to M1.5 are done (per-item evidence in the roadmap); the M1 sanity check is
next, then M2 (typed host API).

- M1.3: the compile-time copy carries `#line` directives on `slang::compile*` and both
  reflection paths; exported `.slang` never does. The native helper honours them, and
  the sidecar records `line_directives`. 10 of 10 deliberate failures land on the right
  Rust line on WGSL and SPIR-V; WGSL/SPIR-V bytes are identical with and without
  directives for three fixtures. Columns stay Slang columns; only the first error is
  attributed (`8fcf944`, `7007827`).
- M1.4: a rejected `#[gpu]` module is returned beside `compile_error!` with `#[kernel]`
  removed, kernel fns renamed and private as on the success path, and the prelude
  imported; `mod m;` gets the error alone. Measured on `vector-add` with one rejection
  inside the referenced kernel (rustc 1.98): one error before and after, the same
  message; the editor benefit is not measured (`e01b82c`, `f507ab4`).
- M1.5: every `#[gpu]` rejection carries a stable code (`[GUST-E0401] message`, D31);
  111 codes, one `docs/DIAGNOSTICS.md` section each with its reachability. Every code is
  asserted by a test or listed as unreachable through `#[gpu]` (seven codes). Guards:
  unique codes, doc and enum agree, and a source scan for uncoded `syn` errors whose
  limits the doc states (`df07122`, `6e195bc`, `f20d313`).
- M1 carry-overs: check-ui matches each expectation within one rustc diagnostic; the
  `on_unimplemented` text no longer claims exclusivity; the vcvars capture refuses
  inherited multi-line values that could forge a variable (`fb257c9`).
- Bug fix: the `typed-pipeline` and `staged-graph` release runs dispatched 65 536
  workgroups against wgpu's default limit of 65 535 and panicked; release sizes are now
  `1 << 21` with a test per example, and both release runs complete (`5c34e22`).

T13 remains deferred by owner; its work in progress is the git stash
`WIP T13 active count deferred`, not the working tree. Do not apply, continue,
validate, commit, or drop it without explicit direction.

```text
owner: Claude Code coordinator (monitored autonomy)
claim: roadmap M1, slice by slice (see the roadmap's Now and next)
next_focused_check: cargo xtask check-fast
full_check_needed_before_commit: no
```

## Current baseline

- Branch: `main`
- Last complete compiler feature: T12, statement-only `match Result<T, T>`
- Last full validation: `cargo xtask check-full` passed at 2026-10-23T13:24:07Z on
  NVIDIA GeForce RTX 5090 / Vulkan at `8200431`, through the shared cargo slot
  wrapper: workspace tests, release-mode slang tests, all 8 examples, and 13 SPIR-V
  artifacts through `spirv-val`. The local `.ai/VALIDATION.json` record contains no
  machine paths; it does not yet record the commit (fixed in M1 slice m1.0).
- DX19 is closed: full validation, two independent reviews, and fixes (`027eef7`,
  `08a4843`, `3296885`).
- CI (`unit` job: fmt, clippy, Unit suite) has passed on every push to main on
  2026-10-23.

## Current contracts

- Task acceptance criteria: [tasks/README.md](tasks/README.md)
- T10 atomics: [tasks/T10-atomics.md](tasks/T10-atomics.md)
- T11 Result lowering: [tasks/T11-result.md](tasks/T11-result.md)
- T12 Result match: [tasks/T12-result-match.md](tasks/T12-result-match.md)
- T13 active count (deferred): [tasks/T13-active-count.md](tasks/T13-active-count.md)
- DX19 sidecar maps: [tasks/DX19-sidecar-diagnostic-maps.md](tasks/DX19-sidecar-diagnostic-maps.md)

## Latest DX evidence

- 2026-10-23 Unit-tier CI (`.github/workflows/ci.yml`, Rust pinned to 1.98.0): fmt,
  workspace clippy with warnings denied, `cargo test -p gust-macros` (54 passed) and
  `cargo test -p xtask` (6 passed) passed locally before the first push. Evidence tiers
  (Unit, Slangc, Spirv, Reflect, Gpu) are defined in `VALIDATION.md`. The DX tasks
  were renamed DX19/DX20 so decision ids continue at D21. History was rewritten to
  replace machine paths and the repository is now `nevermemo/gust`.
- 2026-10-23 approval and privacy policy: AGENTS.md gained an Approvals section
  (all actions pre-authorized by the owner) and a parallel-lane branch rule;
  `.ai/VALIDATION.json` is now untracked and xtask redacts the workspace root and home
  directory when writing it. `cargo fmt --all -- --check`, `cargo test -p xtask`
  (6 tests), `cargo clippy -p xtask -- -D warnings`, and `cargo xtask check-fast`
  passed through the shared cargo slot wrapper.
- 2026-10-23 cleanup: the stale T13-derived `generated-wgpu/pool__classify.*` exports
  were moved out of the tree, so `generated-wgpu/` again holds the 13 `.spv` files
  `check-full` expects. The T13 stash is untouched.
- 2026-10-22 documentation sync, no code changes: README, ARCHITECTURE,
  ENGINE_NORTH_STAR, NEXT_TASKS, HANDOFF, and the Copilot builder agent profile now
  record DX19 as committed with release evidence pending, point T13 at its stash, and
  list atomics (T10), Option/Result (D14), and bounded struct literals (D07) as
  supported. An independent read-only review checked each added claim against the
  validator, emitter, regressions, fixtures, GPU tests, and xtask (PASS);
  `git diff --check` passed.
- `cargo xtask check-feature core` passed (19 core tests), including sidecar-map
  schema/line-range coverage and construct-enriched compiler diagnostics.
- `cargo test -p xtask` passed (3 routing tests).
- `cargo xtask check-feature result` passed (6 macro plus 4 GPU/target tests).
- `cargo fmt --all -- --check` and `git diff --check` passed.
- DX19 writes temporary `kernel.map.json` files with stable Slang line/construct
  segments and appends mapped kernel/construct labels to Slang failures. Focused
  `cargo test -p gust --lib slang` passed (14).
- Full validation was not refreshed after DX19 because T13 changes were then in the
  working tree. They are now stashed; the `generated-wgpu/` blocker above remains.

## Archive

Completed task reports and superseded ownership records are preserved in
[history/STATUS-2026-09-09.md](history/STATUS-2026-09-09.md).
