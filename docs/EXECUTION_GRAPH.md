# Execution graph and CPU/GPU domains

Status: researched direction. Current execution is explicit host-driven batches;
there is no graph compiler, dependency inference, automatic partitioning, or transfer
planner. Start with `examples/typed-pipeline`, which already proves three ordered
dispatches over different resident types, and extract only what another use requires.

A future graph describes high-level execution nodes and resource-version edges,
not shader instructions. Candidate nodes: CPU task, GPU direct/indirect dispatch,
render pass, upload, readback, copy, join, and continuation. Each node declares
reads/writes, execution domain, observable effects, and dependencies. Producer/consumer
ordering and resource transitions must be lowered for the selected backend; an
edge is not itself a universal GPU barrier.

## First bounded proof

One CPU settings update → typed GPU stage A → GPU stage B → small summary readback.
Verify output, ordering, intermediate GPU residency, and transfer byte counts.
Initially require explicit host-provided dependencies and reject invalid resources;
do not pretend the syn frontend can infer all effects or aliases.

Async/await/spawn/join should ultimately mean dependencies and continuations.
Existing `WgpuJob` is an asynchronous submission handle, not shader async support.
Waiting must happen at a host boundary or a later dispatch, never as a resident
shader spinning for another workgroup.

Dynamic `while_gpu` candidates perform a bounded local budget, persist continuation
or frontier data, write the next active count, and build an indirect dispatch.
Termination, maximum work budgets, cancellation, and malformed counts need explicit
contracts before implementation. A dependency cycle is not fixed by a spin loop.

## Residency

Keep component/particle/geometry buffers on the GPU across stages and frames. Move
only required live-in/live-out values; intentional CPU work can use filesystem,
network, input, assets, or UI. A CPU node is not a substitute chosen silently when
a shader fails to compile. Buffer growth is a GPU-to-GPU copy, not a readback and
reupload. Ownership and safe retirement must account for in-flight submissions.

Automatic loop extraction, effect inference, Rust semantic integration, and graph
optimization are later milestones with independent tests and decision records.
