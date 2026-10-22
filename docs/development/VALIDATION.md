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
| Unit | Macro validation, translation, and xtask logic pass without external tools | `cargo test -p gust-macros`, `cargo test -p xtask` (CI) |
| Slangc | The generated Slang compiles to the target | WGSL compile smoke tests, `cargo xtask check-fast` |
| Spirv | Exported SPIR-V passes `spirv-val --target-env vulkan1.2` | `cargo xtask check-artifacts` |
| Reflect | The D17 layout gate matches the descriptor | `cargo xtask check-feature reflection` |
| Gpu | Readback on a device equals an independent host reference | GPU tests, `cargo xtask check-full` |

Label format: `done (Gpu: NVIDIA GeForce RTX 5090, Vulkan, slangc 2026.13.1, <commit>)`.
A software Vulkan driver such as lavapipe counts as `Gpu (software)`, never as
hardware evidence. Expected values come from an independent host reference, never from
the shadow bodies (D04). A missing tool or adapter is a failure for its tier, not a skip.

## Record policy

Routine checks do not rewrite `.ai/VALIDATION.json`. Use `cargo xtask verify --mode <mode> --record` only when a durable record for a non-full mode is useful. `cargo xtask check-full` records by default and is the release-evidence path.

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
