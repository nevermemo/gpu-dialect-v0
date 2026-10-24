# Current status

## Active: roadmap M2 (2026-10-23)

The roadmap agreed with the owner is in [ROADMAP.md](../ROADMAP.md). M0 closed on
2026-10-23 after a two-reviewer sanity check (D28): every M0 item is done except M0.15,
which waits for a week of green CI (from 2026-10-30). M1 (diagnostics) closed on
2026-10-23 (D33): M1.0 to M1.6 are done (per-item evidence in the roadmap), after a
sanity check by Fable and Opus (both "proceed with fixes") and the full validation
below. M2 (typed host API) is under way, one slice at a time.

- M2 m2.d2: const layout assertions. Every `#[gpu]` struct asserts, in one const block,
  each field at its padding-free offset (the message names the field), no tail padding,
  alignment 4 and `LAYOUT.is_storage_v1()`; every kernel asserts per storage parameter
  that the element layout is StorageV1 and matches its Rust size, alignment and stride.
  `cargo check` now reports what only the runtime asserts caught. The plan's premise that
  no accepted struct could fail was false: a `#[gpu]` struct with a hand-written outside
  `GpuPod` field of alignment 8 passes the validator; it is now E0080 at compile time
  (fixtures `layout-outside-field`, `layout-outside-element`). check-ui follows a primary
  span's macro expansion to the outermost site inside the case (const-eval panics point
  into core). Open for m2.f2: pool buffers (`gust-wgpu/src/pool.rs`) skip the runtime
  layout assert that `create_typed_buffer` has (`adeb7be`).
- M2 m2.d1: `#[gpu]` generates `call`, `submit` and `dispatch` on every kernel handle:
  one argument per storage parameter in declaration order, element and device pinned by
  `Resource` bounds (each spanned on its parameter), access from the parameter. A
  per-kernel const assert pins the storage count against `DESCRIPTOR`, so `cargo check`
  catches a mismatch. Kernels with a `Uniform` parameter get no typed call; kernels with
  no storage parameters take the device type (`call::<D>(..)`). Measured: component-pool's
  five-parameter kernel infers without turbofish (spike S3); a wrong element type is
  E0271 on the user's line; a device without `Run` shows `Run`'s `on_unimplemented` text.
  A non-GUST argument gives two E0277 errors, so it has no fixture. Known limits: the
  `__Gust` prefix of generated generics is not reserved; the `Into` and `Run` notes still
  point at `#[gust::gpu]` (`a83a96e`).
- M2 m2.d0: `check-ui` reads `cargo check --message-format json` and judges only the
  case's own diagnostics; a failing case must produce exactly one error. `expected.txt`
  pins it by keys: `code`, `file` and `line` (exact, on one primary span), `message`
  (substring of the message or a child note) and `label` (substring of that span's label,
  where `on_unimplemented`'s label lands, measured on real runs). Editing the label text in
  `gust` now fails check-ui. Open for m2.d1: spans inside macro-generated code may need
  `expansion` followed, and one error per case may be tight for two-error fixtures
  (`a8b074f`).
- M2 m2.c: `gust-wgpu` implements the typed surface. `HeadlessDevice` is a
  `TypedDevice` with a `Run<K, N>` for every kernel, and `GpuBuffer<T>` is a `Resource`;
  every typed call goes through the existing persistent-binding validation (layout,
  device, access, current length) before any wgpu object. One buffer bound to two
  parameters is now rejected as `Error::AliasedBinding` when either binding is
  read-write, on typed, untyped and graph paths (measured before: read-only plus
  read-write panicked in wgpu, read-write twice ran with the last writer winning; no
  example binds a buffer twice, and check-examples passes). Binding indices are checked
  against `max_bindings_per_bind_group` on every dispatch path. 19 GPU tests in
  `tests/typed.rs`. Open for m2.e: a `DispatchIndirect` node whose arguments buffer is
  also bound read-write in the same node is not checked; the `_` arm on the
  `#[non_exhaustive]` `LengthRule` would silently accept a new variant (`119ece3`).
- M2 m2.a: typed call surface in `gust` (`dispatch.rs`). `TypedDevice` extends
  `Device` (unchanged) with a `Binding<'r>` type; `Resource`, `Arg`, `Independent`,
  `LengthRule` and `Call` describe one typed launch, and `Run<K, N>` submits it.
  `Call::new` takes the descriptor from the kernel type and asserts the binding count
  at monomorphisation; order, access, element and device are checked by generated
  calls (types) and by each backend at submit (contract). Only test mocks implement it
  so far (a recording mock and a CPU-shaped `RefCell` mock); `gust-wgpu` follows in
  m2.c, the macro in m2.d1 (`3060072`).
- M2 m2.b: launch-limit checks. Every dispatch path checks the kernel and its bindings
  against the opened device's limits before any compile or cache lookup and returns
  `Error::DeviceLimit`; before, wgpu panicked (measured: workgroup size 512 at pipeline
  creation, 65 536 workgroups at dispatch). The opened RTX 5090 device carries wgpu's
  default limits, not the adapter's. `HeadlessDevice::with_limits` only lowers limits
  (five fields honoured). Known gaps: in a multi-node graph earlier nodes may compile
  before an over-limit node fails (nothing is submitted; m2.e moves the check into graph
  validation); the dispatch emitted by `render_wgpu_source` and buffers above
  `max_buffer_size` are not checked yet; binding indices are checked since m2.c
  (`151ef56`).

- M1.6: sanity-check fixes. Clippy `disallowed-methods`/`disallowed-macros` replace the
  hand-written uncoded-error scanner, and the exception is confined to `diagnostics.rs`
  by a test; a code counts as tested only when an assertion passed for it; `#line`
  paths follow the macro's path rule, including Unicode letters (slangc echoes UTF-8
  names); `resolver = "3"` with unchanged lockfiles. Unit, Slangc and Reflect passed on
  the merged tree (`1f961fc`).

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
claim: roadmap M2, slice by slice (see the roadmap's Now and next)
next_focused_check: cargo xtask check-fast
full_check_needed_before_commit: no
```

## Current baseline

- Branch: `main`
- Last complete compiler feature: T12, statement-only `match Result<T, T>`
- Last full validation: `cargo xtask check-full` passed at 2026-10-23T22:45:51Z on
  NVIDIA GeForce RTX 5090 / Vulkan at `a6cbb8f` (clean tree, slangc 2026.13.1), through
  the shared cargo slot wrapper: workspace tests, release-mode slang tests, all 8
  examples (debug builds), and 13 SPIR-V artifacts through `spirv-val`. The Slangc and
  Reflect suites passed at the same commit. The local `.ai/VALIDATION.json` record
  stamps the commit and dirty state and contains no machine paths.
- DX19 is closed: full validation, two independent reviews, and fixes (`027eef7`,
  `08a4843`, `3296885`).
- CI (`unit` job: fmt, clippy, Unit suite; `msrv` job on Rust 1.90) passes on main;
  one push on 2026-10-23 (`10b2e06`) was red and was fixed in `3cdc607`.

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
