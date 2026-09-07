# Project adapter contract

No compiler checkout was supplied with this bundle. The verification driver is complete, but the project must supply two small PowerShell adapter scripts for its actual compiler and runtime interfaces. They are integration points, not new compiler layers or a custom IR. Do not invent Cargo packages or claim GPU acceptance before these execute real project code.

## Emit adapter

Accept named parameters `ProjectRoot`, `SourcePath`, `OutputPath`. Invoke the existing Rust compiler/macro build to validate SourcePath and emit Slang directly from syn AST into OutputPath. Throw or exit nonzero on failure. Never copy the bundled Slang fixture or a cached shader. Resolve includes/imports from ProjectRoot. Log the exact command and versions without secrets. The driver supplies a unique fresh output directory.

## Runtime adapter

Accept `ProjectRoot`, `ShaderPath`, `ResultPath`. Run headless wgpu on that exact shader, validating module/pipeline and comparing readback with independent CPU expectations. Throw/exit nonzero on failure. Write JSON to ResultPath only after all tests pass:

```json
{
  "schemaVersion": 1,
  "status": "PASS",
  "execution": "wgpu",
  "shaderSha256": "64 hex digits computed from ShaderPath",
  "adapter": "actual adapter name",
  "backend": "actual wgpu backend",
  "deviceType": "actual device type",
  "hardware": true,
  "cases": [
    {"name": "single", "n": 1, "compared": 1, "passed": true},
    {"name": "partial-group", "n": 65, "compared": 65, "passed": true}
  ]
}
```

This illustrates the report schema, not a report to copy. Choose lengths for the actual workgroup size and kernel; use distinct nonzero expected outputs. Each case must compare all n elements; the driver requires at least one n>1 case. Add length 0 under the host contract and sizes around workgroup boundaries. `hardware` must reflect the selected adapter, not the requested preference. Use `-RequireHardware` when software execution is unacceptable. If using Rust tests, ensure no early-return skip on missing GPU and capture actual runtime evidence.

The driver hashes the input and output artifacts, rejects missing output/report, rejects mismatched shader hashes and failed/empty comparisons, and records JSON plus logs in a unique `target/skill-verification/vertical-*` directory. It runs both target compilations only when both are requested; it executes the selected RuntimeTarget. A pass on WGSL plus valid SPIR-V does not claim SPIR-V execution. Run again with RuntimeTarget=spirv and a compatible runtime adapter to prove that path.

Adapters are trusted project code, not a security boundary: a forged report cannot prove execution. Review the actual adapter/test implementation. JSON results are validation metadata, not a compiler IR. Frontend compile-fail and broader workspace checks remain separate in compiler-testing.

Example invocation from project root, once those project adapters exist:

```powershell
$v = '.\.agents\skills\gpu-vertical-slice-verification\scripts'
& "$v\Test-VerticalSlice.ps1" -ProjectRoot . -SourcePath .\tests\kernels\vector_add.rs `
  -EmitScript .\scripts\emit-verification.ps1 -RuntimeScript .\scripts\run-verification.ps1 `
  -EntryPoint computeMain -Targets wgsl,spirv -RuntimeTarget wgsl -TargetEnv vulkan1.2 -RequireHardware
```

The example paths are an interface illustration; use the actual project source/adapters. The target environment must match the project's runtime contract.
