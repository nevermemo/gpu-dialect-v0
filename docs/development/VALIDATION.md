# Validation strategy

Validation is layered. Choose the cheapest command that can falsify the current claim, then widen only when the touched boundary requires it.

## Command tiers

| Situation | First check | Wider check | Release check |
| --- | --- | --- | --- |
| Validator/lowering/golden change | `cargo xtask check-feature macro` | `cargo xtask check-workspace` | `cargo xtask check-full` |
| Reflection/layout change | `cargo xtask check-feature reflection` | `cargo xtask check-feature gpu-runtime` | `cargo xtask check-full` |
| Loop dialect change | `cargo xtask check-feature loops` | `cargo xtask check-feature gpu-semantics` | `cargo xtask check-full` |
| Result lowering/match | `cargo xtask check-feature result` | `cargo xtask check-feature gpu-semantics` | `cargo xtask check-full` |
| Diagnostic sidecar maps | `cargo xtask check-feature core` | `cargo xtask check-workspace` | `cargo xtask check-full` |
| wgpu runtime/cache/buffer change | `cargo xtask check-feature gpu-smoke` or `gpu-runtime` | `cargo xtask check-feature wgpu` | `cargo xtask check-full` |
| GPU semantic feature | focused macro/golden test + one GPU feature test | `cargo xtask check-feature gpu-semantics` | `cargo xtask check-full` |
| Example-only change | `cargo test -p <example> -- --ignored` | `cargo xtask check-examples` | `cargo xtask check-full` |
| Artifact/export change | relevant example binary | `cargo xtask check-artifacts` | `cargo xtask check-full` |
| Unknown dirty tree | `cargo xtask check-changed` | command it selects | `cargo xtask check-full` if release evidence is needed |

## Evidence tiers

Every status line, task report, and decision that cites evidence names the highest
tier it reached. A tier is claimed only with a run record (command, result, and for
Gpu the device).

| Tier | Claim | Typical command |
| --- | --- | --- |
| Unit | Macro validation, translation, and xtask logic pass without external tools; the declared minimum Rust version compiles every workspace target | `cargo test -p gust-macros`, `cargo test -p xtask`, `cargo xtask lint` (CI), CI job `msrv` |
| Slangc | The generated Slang compiles to the target | WGSL compile smoke tests, `cargo xtask check-fast` |
| Spirv | Exported SPIR-V passes `spirv-val --target-env vulkan1.2` | `cargo xtask check-artifacts`, `cargo xtask spirv-val <file.spv> --target-env vulkan1.2` |
| Reflect | The D17 layout gate matches the descriptor | `cargo xtask check-feature reflection` |
| Gpu | Readback on a device equals an independent host reference | GPU tests, `cargo xtask check-full` |

Each tier is one command: `cargo xtask test --suite <Tier>`. Unit runs `cargo test --locked -p gust-macros`, `cargo test --locked -p xtask`, lint, docs-check and check-arch. Slangc is Unit plus `cargo test --locked -p gust --lib` and `cargo test --locked -p gust --test diagnostics` (the Rust-line annotation of real Slang failures). Spirv is `check-artifacts`, Reflect is `check-feature reflection`, Gpu is `cargo test --locked -p gust-wgpu` (a missing adapter fails), and Full is `check-full`, which also runs `cargo test --locked --release -p gust --lib slang` so that the release-only (`cfg(not(debug_assertions))`) tests such as the source-map fallback run. A suite prints what it does not cover, and a pass of one tier says nothing about the tiers above it.

Label format: `done (Gpu: NVIDIA GeForce RTX 5090, Vulkan, slangc 2026.13.1, <commit>)`.
A software Vulkan driver such as lavapipe counts as `Gpu (software)`, never as
hardware evidence. Expected values come from an independent host reference, never from
the shadow bodies (D04). A missing tool or adapter is a failure for its tier, not a skip.

## Single-stage commands

Workflow commands live in `cargo xtask` (D27); no shell or PowerShell scripts are tracked.

| Command | Use |
| --- | --- |
| `cargo xtask lint` | No tracked `.ps1`, `.psm1`, `.psd1`, `.bat`, `.cmd` or `.sh` file, and no machine-specific path in tracked text. Machine paths include Windows, Unix and WSL home spellings (a Unix home name must be followed by `/`, a quote or the end of the line); two test files may spell placeholder names only. Non-UTF-8 and binary files are skipped, and the summary counts them. Scans tracked files only (`git ls-files`): a new file is not linted until `git add` has staged it (see "What each scan sees" below). Also runs in `verify --mode smoke`, `check-fast`, `check-full` and CI. |
| `cargo xtask docs-check` | Relative Markdown links in project `.md` files (any case of the extension; tracked files plus untracked files that are not ignored; footnote definitions are not links) resolve to project files (anchors, query strings and external links are not checked), no merge-conflict markers in text files, and `docs/DECISIONS.md` ids are unique (also when the repeat is not adjacent) and increasing with the right "Next id". Sees untracked files that are not ignored, unlike `lint`. Same runs as `lint`. |
| `cargo xtask check-arch` | D02 guardrails: per-crate dependency allow-list (`gust-macros` = proc-macro2, quote, syn; `gust` = gust-macros; `gust-wgpu` = gust, pollster, wgpu; `xtask` = none), read from `cargo metadata --format-version 1 --locked --no-deps` so that cargo itself resolves `workspace = true`, `package` renames, target tables and every dependency kind (normal, dev, build). Each dependency is judged by its real package name; an allow-listed one must not be renamed with `package`, use a git source or an alternative registry, or be a path dependency outside `crates/<name>` (a workspace crate must be exactly `crates/<name>`). A manifest cargo rejects, for example an inherited dependency with no `[workspace.dependencies]` entry, fails the check with cargo's own message. The metadata is parsed by a small std-only JSON reader in xtask; no dependency is added. Not checked: `[patch]` and `[replace]` sections, and `[workspace.dependencies]` entries that no checked crate inherits. Also: no `ir`, `*_ir` or `ir/` module or file under `crates/` outside a `future/` path, no SPIR-V library in the compiler crates. A change is a DECISIONS entry plus an edit of `xtask/src/arch.rs`. Needs git and cargo. Sees untracked files that are not ignored, unlike `lint`. Same runs as `lint`. |
| `cargo xtask bless` | Reruns the Slang golden tests (`cargo test --locked -p gust-macros golden`) with `BLESS_GOLDEN=1`: a golden whose emitted text differs is rewritten, its line diff printed and its test failed with "golden blessed; review the diff and rerun without BLESS_GOLDEN". Then lists changed fixtures with `git diff --stat`. Exits 0 only when no fixture changed. Review every diff before committing. |
| `cargo xtask test --suite Unit\|Slangc\|Spirv\|Reflect\|Gpu\|Full` | One evidence tier as one command (below). Each suite prints its tier and what it does not cover; a missing tool or adapter fails the suite. |
| `cargo xtask doctor` | Tool readiness. Exits non-zero when cargo, rustc, slangc or the reflection helper is missing; `spirv-val` is reported and required only for artifact and full modes. |
| `cargo xtask doctor --smoke` | Doctor, then the hand-written `vector-add.slang` toolchain fixture compiled to WGSL and SPIR-V in a fresh directory under the cargo target directory, and the SPIR-V validated. It says that the Rust frontend and GPU execution were not run; it is not Slangc or Spirv evidence for generated Slang. |
| `cargo xtask slangc <file.slang> --entry <name> --target wgsl\|spirv --out <path> [-- <slangc args>]` | One slangc compile. Refuses an existing output, fails on a missing or empty artifact, writes `<path>.command.json` (version, arguments, exit code, output SHA-256) and `<path>.compile.log`. |
| `cargo xtask spirv-val <file.spv> --target-env <env>` | SPIR-V length and magic check, then `spirv-val`; prints the SHA-256 and validator version. |
| `cargo xtask test-exact -p <package> --test <target> <name>` | Lists the tests, requires `<name>` exactly once, runs it with `--exact --include-ignored`, and fails unless libtest reports one executed, passing test. Certifies execution only. |

`check-lints`, `check-workspace`, the Clippy and workspace-test steps and the Fast and Gpu test steps of the verify modes pass `--locked` to cargo, as do `test-exact` and every cargo call of the Unit, Slangc, Spirv, Reflect and Gpu suites, including the two reflection test runs behind Reflect and the example runs behind Spirv (a unit test checks them; Reflect also builds the native helper, and only Spirv runs the examples). Not locked: `cargo fmt` (it has no such flag), the other `check-feature` areas, and the `--ignored` example tests of the Examples and Full modes, so the Full suite is not covered by this statement.

## What each scan sees

`cargo xtask lint` reads **tracked files only** (`git ls-files`). `cargo xtask docs-check` and `cargo xtask check-arch` read tracked files **plus untracked files that are not ignored** (the files the next commit would contain). A new script, or a new file with a machine-specific path, is therefore not caught by `lint` until it is staged with `git add`, while a broken link or a stray `ir` module is caught before that. Ignored files (`target/`, `generated-wgpu/`, `.ai/VALIDATION.json`) are scanned by none of them.

## Minimum Rust version

The workspace declares `rust-version = "1.90"` (D30, which supersedes the 1.88 in D29: the locked `ordered-float 5.5.0`, pulled in by wgpu-hal, declares 1.90). The CI job `msrv` measures the claim: it installs exactly Rust 1.90.0 and runs `cargo check --workspace --all-targets --locked` (all targets, so test code is covered). It needs no slangc and no GPU and runs no tests, so it is Unit-tier evidence of compilation only. It is not a required check yet. A local run needs a 1.90 toolchain: `rustup toolchain install 1.90.0`, then `RUSTUP_TOOLCHAIN=1.90.0 cargo check --workspace --all-targets --locked`. `cargo xtask check-full` uses the stable toolchain and does not measure the minimum version; `.ai/VALIDATION.json` lists it under `untested`.

## Record policy

Routine checks do not rewrite `.ai/VALIDATION.json`. Use `cargo xtask verify --mode <mode> --record` only when a durable record for a non-full mode is useful. `cargo xtask check-full` records by default and is the release-evidence path. The record carries `git_commit` (the output of `git rev-parse HEAD`, taken before the checks start) and `git_dirty` (true when `git status --porcelain` listed anything: modified, staged, or untracked files that are not ignored). Both are `null` when git is unavailable or refuses the repository. A record with `git_dirty: true` is evidence about that working tree, not about the commit. `cargo xtask status` prints them as `commit=` and `dirty=`; a record written before the stamp existed prints `unknown`.

## Required evidence by boundary

- Validator: accepted fixture or single-cause rejection test with a meaningful diagnostic.
- Lowering: reviewed Slang golden plus a source-structure assertion when useful.
- Shader target: WGSL compile smoke in routine tests; SPIR-V validation in artifact/full checks.
- Runtime: real GPU readback against an independent host reference, including partial and multiple workgroups.
- Engine proof: explicit contract first, exact transfer/residency/count assertions, no silent fallback.

## What not to do

- Do not run `cargo xtask check-full` as the first check for a local compiler edit.
- Do not auto-bless goldens without reviewing the semantic diff.
- Do not treat `spirv::validate_structure` as semantic SPIR-V validation; external `spirv-val` owns that layer.
- Do not introduce shared `HeadlessDevice` fixtures until `cargo xtask measure-tests` isolates adapter/device setup as the bottleneck.
- Do not commit regenerated `generated-wgpu/` outputs; full/artifact checks regenerate
	them locally and validate the SPIR-V files before leaving them ignored.
