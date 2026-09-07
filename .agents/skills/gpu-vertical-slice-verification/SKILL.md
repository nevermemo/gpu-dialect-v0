---
name: gpu-vertical-slice-verification
description: "Verify a complete restricted Rust GPU kernel through AST validation, direct Slang emission, slangc WGSL/SPIR-V compilation, validation and headless wgpu numerical readback. Use for end-to-end acceptance or locating the first failing compiler boundary."
---

# GPU vertical slice verification

Require a traceable Rust source -> validated syn AST -> emitted Slang -> compiled shader -> wgpu readback chain. Do not introduce custom IR or accept a hand-authored shader/CPU fallback as evidence of Rust compiler execution.

1. Pick one meaningful kernel and define expected outputs plus boundary cases.
2. Run the project compiler against that source; preserve fresh artifacts, source hash and compiler command.
3. Compile requested targets. Validate SPIR-V externally and validate the executed shader through wgpu.
4. Execute and compare on the reported adapter. Report each stage as PASS, FAIL or UNAVAILABLE with evidence paths.

Read [adapter contract](references/adapter-contract.md) to wire the project's actual compiler/runtime commands into [Test-VerticalSlice.ps1](scripts/Test-VerticalSlice.ps1). The bundle cannot know absent project CLI names. Missing adapters fail explicitly. Use [Test-Toolchain.ps1](scripts/Test-Toolchain.ps1) for a standalone Slang/SPIR-V smoke test; that is not full vertical verification.
