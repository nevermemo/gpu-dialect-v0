---
name: spirv-validation
description: "Validate or inspect emitted SPIR-V modules, Vulkan target environments, capabilities, entry points and resource decorations using SPIRV-Tools. Use for .spv artifacts and backend validation errors; validation alone never proves kernel semantics."
---

# SPIR-V validation

Validate the exact fresh artifact produced by slangc, against the runtime's declared target environment.

1. Record input hash, slangc version/options and `spirv-val --version`.
2. Run [Test-Spirv.ps1](scripts/Test-Spirv.ps1) with an explicit `-TargetEnv` such as `vulkan1.2` only when that matches the runtime contract.
3. If it fails, use `spirv-dis` to inspect entry point, execution mode, capabilities and decorations. Fix generation or capability negotiation rather than disabling validation.
4. Report structural validation separately from wgpu acceptance and numerical GPU execution. A valid no-op module is still a no-op.

Read [environment and diagnosis](references/environment-and-diagnosis.md) only for environment selection or validation failures. The script requires the validator and fails on missing, empty, malformed or invalid input.
