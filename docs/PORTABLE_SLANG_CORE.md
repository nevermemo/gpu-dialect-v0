# Portability contract and evidence

Status: policy plus limited executable evidence. Last reviewed 2026-09-07.

| Tier | Meaning | Current evidence |
| --- | --- | --- |
| A: Portable GPU Core | Conservative Slang GPU subset across intended GPU targets; excludes CUDA-specific and Slang CPU execution assumptions | WGSL and SPIR-V compilation tests only |
| B: WebGPU engine profile | Explicit bounded bindings, portable resource layouts and synchronization, no native-only requirements | Native Vulkan wgpu execution of WGSL; no browser test yet |
| C: Native optimization | Capability-gated binding arrays, nonuniform indexing, stronger atomics, multi-draw or mesh paths | Not implemented or tested here |

Tier labels are not certification. Metal, DXIL, browser WebGPU, and additional
vendors remain untested. A future machine-readable manifest should record compiler
version, target/options, adapter/features, fixture, compile/validate/execute outcome,
and unsupported reasons. The initial `.ai/VALIDATION.json` records local command
outcomes, versions in stdout, and exported SPIR-V hashes; it is not that full
cross-target capability matrix. Missing tools cannot count as successful probes.

## Capability probe records

`gpu_dialect::slang::probe(target)` compiles a known-good minimal compute kernel to
`target` and returns a `TargetProbe { target, supported, detail }` record. `supported`
is true only when `slangc` actually emitted the target; `detail` carries the success
marker or the compiler diagnostic on failure. This is the first machine-readable
capability record: downstream claims must cite a probe record, not an assumption.
The probe is capability evidence, not a correctness test. Cross-target discovery
(DXIL, Metal, browser WebGPU) and adapter/feature capture remain future work beyond
the completed T05 probe slice (T05 is complete; those are not part of its scope).

## Resource baseline

Keep element data in `StructuredBuffer<T>` / `RWStructuredBuffer<T>` with fixed-size
elements. Store logical counts/capacities/offsets as separate metadata. Do not nest
an unsized geometry structure inside `RWStructuredBuffer<GeometryBuffer>`.

WGSL permits a runtime-sized array as a storage-buffer store type, or the last
member of the top-level storage structure; array elements cannot themselves be
runtime-sized. See the [WGSL type rules](https://www.w3.org/TR/WGSL/#runtime-sized)
and [Slang WGSL resource mapping](https://shader-slang.org/slang/user-guide/wgsl-target-specific).
Slang maps structured buffers into storage arrays. A custom top-level header plus
trailing array would need a separate, proven layout path; it is not today's ABI.

Portable references are indices, offsets, and IDs. A future `GpuPtr<T>` may carry
buffer identity plus offset, with bounds and lifetime rules; it is not a native
address or a promise to preserve arbitrary Rust pointer behavior.

Do not say “wgpu has no bindless or multi-draw.” Native capabilities exist, but
their requirements and backend support differ. Check [wgpu 30 feature flags](https://docs.rs/wgpu/30.0.1/wgpu/struct.Features.html)
and enable them only after probing. The engine baseline must work without those
optional paths; native fast paths preserve the same output semantics.

## Numeric and ABI boundary

Today: 32-bit scalar arithmetic and recursively padding-free, four-byte-aligned
storage structs; booleans are not host POD. No vector/matrix/texture/uniform layout
promise. Float comparisons use explicit tolerances when operations can round
differently. No claim of general Rust integer overflow or float-to-int cast semantics.
Any wider ABI needs target layout tests before public buffer support.
