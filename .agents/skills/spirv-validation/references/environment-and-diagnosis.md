# Environment and diagnosis

Choose the environment from the intended Vulkan runtime and required SPIR-V capabilities. `spirv-val --help` lists the installed tool's environments; the script delegates validity of the supplied name to the tool. A SPIR-V version and a Vulkan version are different dimensions. Do not raise the target environment merely to silence an error unless the runtime supports it and the project contract changes deliberately.

The script checks binary length and magic as an early format check, then invokes `spirv-val --target-env ENV FILE`. Header checks alone cannot establish validity. It records the exact SHA-256. Missing tools are failures for this check, not a skip reported as success.

For diagnosis:
1. Check `OpEntryPoint` is the expected compute entry and its local size matches dispatch assumptions.
2. Inspect DescriptorSet/Binding decorations and storage classes against host metadata.
3. Check capabilities/extensions against both selected environment and device features.
4. Preserve full diagnostics in a local log; return only the relevant error and artifact path.

SPIRV-Tools validation does not prove that loads, arithmetic or stores implement the Rust program. Inspect a disassembly when useful, but actual GPU readback is the semantic evidence. wgpu may reject a module that passes external validation because its accepted frontend/features impose additional restrictions. A WGSL execution path does not validate a separately produced SPIR-V artifact at runtime.

Official source checked 2026-09-07: [Khronos SPIRV-Tools](https://github.com/KhronosGroup/SPIRV-Tools). The script's CLI form was also exercised against the installed validator during bundle QA; see the root validation report for version and limits.
