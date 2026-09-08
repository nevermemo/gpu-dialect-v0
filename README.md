# GUST — GPU <3 rUST

GUST is growing toward a heterogeneous Rust execution compiler and, eventually,
a GPU-centric ECS game engine. **Today it is a working experimental Rust → Slang
compute subsystem**, still named GPU Dialect in the crate APIs. There is no ECS,
renderer, rustc semantic integration, or execution-graph compiler yet.

Rust supplies the higher-level language and checking environment; Slang owns GPU
code generation; GUST will connect intentional CPU and GPU execution. We preserve
the working subsystem while proving that larger model incrementally.

Start with [current status](docs/development/STATUS.md),
[next tasks](docs/development/NEXT_TASKS.md), and
[the handoff](docs/development/HANDOFF.md). Engineering agents should read
[AGENTS.md](AGENTS.md).
The [vision](docs/VISION.md), [roadmap](docs/ROADMAP.md), and
[engine north star](docs/ENGINE_NORTH_STAR.md) distinguish implementation from plans.
Development conventions live in [repository structure](docs/development/REPO_STRUCTURE.md),
[validation strategy](docs/development/VALIDATION.md), and
[agent workflow](docs/development/AGENT_WORKFLOW.md).

The current executable path is:

```text
#[gpu] Rust module
        │
        ├─ rustc checks the shadow Rust types and kernel body
        │
        └─ proc macro translates the syn syntax tree directly to Slang
                                      │
                                      ├─ gust-slang-reflect (Slang API) ─> WGSL + layout ─> headless wgpu
                                      ├─ slangc -target wgsl ──> exported .wgsl
                                      └─ slangc -target spirv -> .spv / SPIRV-Tools
```

There is no custom shader IR and no hand-written SPIR-V emitter in this path. A
kernel descriptor stores generated Slang source plus macro-assigned binding metadata.
On its first pipeline-cache miss the wgpu backend runs the native reflection helper,
which compiles the kernel and reflects the *same linked program* through Slang's
layout API. The runtime refuses to create the pipeline unless the descriptor's
names, bindings, access modes, workgroup size, and every nested field offset, size,
alignment, and stride equal what the compiler reports; on success it caches the
shader module, bind-group layout, pipeline layout, and compute pipeline.

## Try it

Requirements:

- The stable Rust toolchain (this checkout was validated with Rust 1.98.0;
  the manifest's older minimum is not independently verified)
- `slangc` on `PATH` (tested with Slang 2026.13.1)
- The native reflection helper, built once with
  `cargo xtask build-slang-reflect`. It needs the Slang
  SDK headers and import library (from `VULKAN_SDK` or `SLANG_SDK`) and a C++
  toolchain (MSVC on Windows). The runtime finds it in `target/slang-reflect/` or
  through `GUST_SLANG_REFLECT`; without it every pipeline creation fails with an
  explicit error rather than skipping the layout check
- A Vulkan compute adapter for the headless execution tests
- Optional: `spirv-val` from SPIRV-Tools for external SPIR-V validation

Run the smallest end-to-end example:

```sh
cargo run -p vector-add
```

It executes real code on the selected GPU and writes four inspectable artifacts to
[`generated-wgpu/`](generated-wgpu/):

- `vector_add__add.slang` — direct Rust-to-Slang output
- `vector_add__add.wgsl` — Slang-generated WGSL used by wgpu
- `vector_add__add.spv` — Slang-generated SPIR-V
- `vector_add__add.rs` — readable descriptor-specialized wgpu host code

Run the normal development suite:

```sh
cargo test --workspace
```

Example crate tests are intentionally ignored by default. They remain available for
major changes and release validation, but the ordinary workspace loop prioritizes
compiler/runtime feedback over full example proofs.

For the full release-readiness check (including ignored example tests, all seven
examples, and mandatory external validation of all twelve SPIR-V exports):

```sh
cargo xtask check-full
```

It writes `.ai/VALIDATION.json` with command exit codes, stdout, artifact hashes,
and explicit untested targets. This is local debug validation, not release benchmark
evidence or cross-platform certification.

Purpose-built check scripts keep the inner loop honest:

| Command | Purpose | Runs |
| --- | --- | --- |
| `cargo xtask check-feature macro` | Macro/validator/emitter work | `cargo test -p gpu-dialect-macros` |
| `cargo xtask check-feature reflection` | Reflection/layout work | helper version check, core reflection tests, wgpu reflection tests |
| `cargo xtask check-feature loops` | Bounded-loop work | loop golden/rejection test plus GPU loop test |
| `cargo xtask check-feature gpu-smoke` | Cheapest wgpu runtime sanity | wgpu crate unit tests only |
| `cargo xtask check-feature gpu-semantics` | GPU semantic differential work | semantics, numeric, option, loops, struct-assignment tests |
| `cargo xtask check-feature gpu-runtime` | Runtime/reflection gate work | wgpu reflection integration tests |
| `cargo xtask check-changed` | Dirty-tree routing | chooses a focused check from changed paths |
| `cargo xtask check-format` | Formatting only | `cargo fmt --all -- --check` |
| `cargo xtask check-lints` | Strict lints | workspace Clippy with warnings denied |
| `cargo xtask check-fast` | Fast routine confidence | helper version check, fmt check, macro tests, core lib tests, wgpu tests |
| `cargo xtask check-workspace` | Default workspace confidence | workspace tests with example packages excluded entirely |
| `cargo xtask check-examples` | Major example behavior confidence | ignored example tests plus example binaries |
| `cargo xtask check-artifacts` | Artifact/export confidence | example binaries plus external SPIR-V validation |
| `cargo xtask check-full` | Release confidence | full staged verification |
| `cargo xtask status` | Resume context | git status, active claim, validation summary, suggested check |
| `cargo xtask doctor` | Environment readiness | required tools and reflection helper status |
| `cargo xtask list-tests` | Test inventory | summarizes test categories from Cargo's test list |
| `cargo xtask explain-check <area>` | Command intent | prints why/when to use a check |
| `cargo xtask measure-tests --json target/test-times.json` | Profiling before pruning | timed default test commands or one custom command |

`cargo xtask verify --mode fast|gpu|examples|artifacts|full` exposes the same staged
checks. Routine verify modes do not rewrite `.ai/VALIDATION.json` unless passed
`--record`; `check-full` and `verify --mode full` record by default. The native
reflection helper build is timestamp-gated and still runs `--version`, so repeated
checks do not relink unchanged C++ code.

The same checks are exposed as VS Code tasks: `GUST: check fast`, `GUST: check
feature`, `GUST: check changed`, `GUST: check examples`, `GUST: check artifacts`, and
`GUST: check full`.
Do not introduce shared `HeadlessDevice` fixtures until `cargo xtask measure-tests` shows
adapter/device setup is the bottleneck; cache-stat tests intentionally use isolated
devices.

## Authoring model

The preferred source vocabulary deliberately resembles Slang:

```rust
use gpu_dialect::gpu;

#[gpu]
mod vector_add {
    #[kernel(workgroup_size(64, 1, 1))]
    pub fn add(
        id: SV_DispatchThreadID,
        a: StructuredBuffer<float>,
        b: StructuredBuffer<float>,
        mut out: RWStructuredBuffer<float>,
    ) {
        let i = id.x;
        if i < out.len() {
            out[i] = a[i] + b[i];
        }
    }
}
```

`#[gpu]` automatically imports the shader prelude. The names `float`, `int`,
`uint`, `SV_DispatchThreadID`, `StructuredBuffer<T>`, and
`RWStructuredBuffer<T>` are lightweight Rust shadow types: they let rustc parse,
resolve, and type-check the source while the macro translates the original syntax
tree to Slang.

For compatibility with the first prototype, `f32`, `i32`, `u32`, `Invocation`,
`Storage<T>`, and `StorageMut<T>` are still accepted and map to the same Slang
types. New code should prefer the Slang-shaped spellings.

The macro turns each public kernel name into a zero-sized host handle:

```rust
let descriptor = vector_add::add::DESCRIPTOR;
println!("{}", descriptor.slang_source);
println!("entry point: {}", descriptor.entry_point);
```

The source function is retained only as a private Rust type-checking shadow. It is
not a CPU execution API. CPU comparisons in the benchmark examples are ordinary,
independent Rust implementations outside the `#[gpu]` module.

## Generated Slang

The kernel above becomes source in this shape:

```slang
[[vk::binding(0, 0)]] StructuredBuffer<float> a;
[[vk::binding(1, 0)]] StructuredBuffer<float> b;
[[vk::binding(2, 0)]] RWStructuredBuffer<float> out;

[shader("compute")]
[numthreads(64, 1, 1)]
void gpu_vector_add_add(uint3 id : SV_DispatchThreadID)
{
    uint __gpu_len_out;
    uint __gpu_stride_out;
    out.GetDimensions(__gpu_len_out, __gpu_stride_out);
    var i = id.x;
    if (i < __gpu_len_out)
    {
        out[i] = a[i] + b[i];
    }
}
```

Resource bindings are assigned in parameter order. Entry points receive a
prefixed `gpu_<module>_<kernel>` name because source names such as `step`
can collide with Slang intrinsics. SPIR-V compilation uses
`-fvk-use-entrypoint-name` so the descriptor's entry point reaches the backend.
These prefixes are not a general hygienic name-resolution system.

Buffer `.len()` calls lower to Slang `GetDimensions` queries. The helper locals are
generated per resource, so bounds checks continue to describe the actual bound
buffer rather than a separately supplied element count.

## Runtime and artifacts

`gpu-dialect::slang` is the compiler bridge:

```rust
let spirv_words = gpu_dialect::slang::compile_spirv(&descriptor)?;
let wgsl = gpu_dialect::slang::compile_wgsl(&descriptor)?;
```

Each invocation exclusively creates an isolated temporary directory, captures Slang
diagnostics, checks SPIR-V structure, and attempts cleanup on every return path. Missing
`slangc` and compiler failures are reported as normal Rust errors.

`gpu-dialect::reflect` is the layout gate. `compile_reflected` runs the native helper
(`scripts/probes/slang-layout.cpp`, built by `cargo xtask build-slang-reflect`) to
compile one entry point and reflect the identical linked program; the result carries
the artifact, its hashes, the compiler build tag, and complete `StorageV1` layouts.
`Reflection::cross_check_kernel` compares that evidence against the descriptor and
fails on any difference. The SPIR-V it produces is byte-identical to `compile_spirv`,
and its WGSL differs from `compile_wgsl` only in line endings. The older
`reflect`/`Reflection::from_json` path reads `slangc -reflection-json`, which omits
aggregate size, alignment, and stride; it remains available as explicit partial
evidence and is not used by the runtime.

The headless backend currently feeds Slang-generated WGSL into wgpu. This keeps
whole-struct copies compatible with wgpu's shader frontend; Slang-generated SPIR-V
is still a first-class export and is validated by the integration tests with
SPIRV-Tools. A future backend can consume the SPIR-V artifact directly where the
driver/API path supports the emitted instruction set.

Pipeline compilation remains lazy and cached per device. Repeated and batched
dispatches reuse:

- the Slang-generated target shader;
- the wgpu shader module;
- the bind-group layout;
- the pipeline layout;
- the compute pipeline.

Persistent typed buffers, heterogeneous struct/scalar bindings, batched command
encoding, asynchronous jobs, and GPU timestamp queries remain available from
`gpu-dialect-wgpu`.

`StagedGraph` is the first explicit execution-graph slice: upload, dispatch, and
readback nodes with host-declared dependencies, run as one ordered submission.
Execution rejects a graph whose declared edges do not cover the buffer hazards its
nodes create, and reports upload/readback bytes and the bytes that stayed resident.
Nothing is inferred from kernels. `BufferBinding::independent_length()` opts one
binding out of the shared element-count rule for settings buffers and reductions;
the kernel must then guard that buffer with its own `.len()`.

`GpuPool<T>` is the first growable resident collection: explicit capacity, a
host-tracked logical length mirrored into a one-element count buffer for kernels,
and host-driven geometric growth that copies the live prefix GPU-to-GPU (never a
readback) and retires the old allocation behind the copy's fence. A buffer from
`create_indirect_buffer` can drive `StagedGraph::dispatch_indirect`, whose
workgroup count the GPU derives; see the contract in
[docs/ENGINE_NORTH_STAR.md](docs/ENGINE_NORTH_STAR.md).

## Examples

The workspace contains seven runnable programs:

- `vector-add` uses the preferred Slang-shaped Rust vocabulary and demonstrates
  artifact export.
- `polynomial` evaluates a multi-step cubic expression and compares independent
  CPU and GPU implementations, including persistent, batched, timed, and async
  execution.
- `signal-pipeline` performs centering, gain, bias, normalization, energy, and a
  final adjustment across six buffers, with the same benchmark modes.
- `particle-step` exercises nested POD structs, integer metadata, whole-struct
  copies, field updates, heterogeneous typed buffers, and dependent batched
  kernels.
- `typed-pipeline` uses six distinct storage structs, three helper functions,
  three compute kernels, and one ordered `calibrate → classify → finalize` batch.
  Every stage consumes the previous stage's typed output and exports its own
  Slang, WGSL, SPIR-V, and readable wgpu host code.
- `staged-graph` runs one CPU settings upload → `transform` → `summarize` → summary
  readback as an explicit `StagedGraph`, keeping the samples and the intermediate
  resident and asserting transfer byte counts, ordering across settings updates, and
  rejection of undeclared dependencies.
- `component-pool` grows a `GpuPool<Particle>` every frame while GPU-authored
  positions survive each GPU-to-GPU copy, and integrates through an indirect
  dispatch whose active count and workgroup arguments a one-invocation kernel
  derives on the GPU, clamped to the allocation and a settings budget.

Release-mode benchmark examples use fixed sizes selected in each example's
`main` function; there is currently no environment-variable size override:

```sh
cargo run --release -p polynomial
cargo run --release -p signal-pipeline
cargo run --release -p typed-pipeline
```

CPU numbers in those examples measure separate host implementations. That makes
the comparison explicit: the shader frontend no longer promises that one Rust
function has identical CPU and GPU operational semantics.

## Current subset

The direct syntax translator currently supports:

- inline `#[gpu]` modules containing named-field structs and functions;
- compute kernels with one dispatch-thread parameter and structured-buffer
  resources;
- `float`/`f32`, `int`/`i32`, `uint`/`u32`, booleans, and nested POD structs;
- local bindings (including explicit type annotations), arithmetic and comparison operators, assignments, indexing,
  field access, `if`/`else`, returns, casts, and calls to helpers in the same module;
- implicit helper returns (including tail `if`/`else` and blocks), 32-bit numeric
  suffixes/radix literals, Rust expression grouping, and boolean/integer `!`;
- bounded `for i in start..end` loops whose bounds type-check as 32-bit integers,
  with `break`/`continue` (the end bound is evaluated once; see D18);
- one-dimensional runtime dispatch with explicit three-dimensional Slang
  workgroup sizes in descriptors.

The validator intentionally rejects allocation, standard-library access, macros,
closures, async/await, references, `while`/`loop`/inclusive ranges, `match`, unsafe
code, arbitrary method calls, and external function calls inside GPU modules. These
restrictions describe the implemented translator, not limits of Slang itself.

Shader struct literals are explicitly rejected pending field-aware lowering;
whole-struct buffer copies and field updates work. Booleans are expression values,
not host-shareable `GpuPod` fields. Only padding-free 32-bit scalar/nested-struct
storage layouts are executable today. Constant-buffer syntax is not yet supported
by the headless runtime. See [architecture](docs/ARCHITECTURE.md) for known semantic
gaps; successful Rust checking does not prove equivalence to Slang inference.

## Workspace

```text
crates/
  gpu-dialect/          shadow types, descriptors, ABI metadata, slangc bridge
  gpu-dialect-macros/   validation and direct syn AST -> Slang translation
  gpu-dialect-wgpu/     cached headless wgpu execution and persistent buffers
examples/
  vector-add/
  polynomial/
  signal-pipeline/
  particle-step/
  typed-pipeline/
generated-wgpu/         readable artifacts emitted by examples
docs/
  ARCHITECTURE.md
```

## Development and next milestones

```sh
cargo xtask check-fast
cargo test --workspace
```

Use focused checks first. `cargo test --workspace` skips ignored example tests;
`cargo xtask check-full` runs them with `--ignored`, runs each example binary, and
validates exported SPIR-V artifacts. Per-feature compile-smoke tests use WGSL only;
SPIR-V structure/semantic validation is centralized in full verification.

Golden Slang and a real-GPU semantics fixture live in `tests/fixtures/`; update
expected output only after reviewing the change and testing both target compilers.
The [roadmap](docs/ROADMAP.md) prioritizes correctness, actionable diagnostics,
ABI/capability evidence, and then a small explicit staged-execution proof.
Slang reflection now gates every pipeline for the StorageV1 subset (D17); broader
resource layouts need the same compiler evidence before they enter the runtime.
New ECS work starts with a small GPU-resident component pool, not a full engine rewrite.

See [the architecture note](docs/ARCHITECTURE.md) for the boundary decisions and
their rationale.

Licensed under MIT or Apache-2.0.
