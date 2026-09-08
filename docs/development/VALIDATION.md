# Validation strategy

Validation is layered. Choose the cheapest command that can falsify the current claim, then widen only when the touched boundary requires it.

## Command tiers

| Situation | First check | Wider check | Release check |
| --- | --- | --- | --- |
| Validator/lowering/golden change | `cargo xtask check-feature macro` | `cargo xtask check-workspace` | `cargo xtask check-full` |
| Reflection/layout change | `cargo xtask check-feature reflection` | `cargo xtask check-feature gpu-runtime` | `cargo xtask check-full` |
| Loop dialect change | `cargo xtask check-feature loops` | `cargo xtask check-feature gpu-semantics` | `cargo xtask check-full` |
| wgpu runtime/cache/buffer change | `cargo xtask check-feature gpu-smoke` or `gpu-runtime` | `cargo xtask check-feature wgpu` | `cargo xtask check-full` |
| GPU semantic feature | focused macro/golden test + one GPU feature test | `cargo xtask check-feature gpu-semantics` | `cargo xtask check-full` |
| Example-only change | `cargo test -p <example> -- --ignored` | `cargo xtask check-examples` | `cargo xtask check-full` |
| Artifact/export change | relevant example binary | `cargo xtask check-artifacts` | `cargo xtask check-full` |
| Unknown dirty tree | `cargo xtask check-changed` | command it selects | `cargo xtask check-full` if release evidence is needed |

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
