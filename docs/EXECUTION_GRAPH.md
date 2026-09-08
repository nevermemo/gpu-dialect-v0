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

Implemented (T07, 2026-09-07) as `gust_wgpu::StagedGraph` in
`crates/gust-wgpu/src/graph.rs` and `examples/staged-graph`. A graph holds
upload, dispatch, and readback nodes in insertion order with host-declared
dependencies. `HeadlessDevice::execute_graph` validates before creating any GPU
object: dependencies must point at earlier nodes, every node's buffers must pass the
existing persistent-dispatch checks, and every earlier node that conflicts on a
buffer (either side writes) must be a transitive ancestor — otherwise
`GraphMissingDependency` names both nodes. Uploads are encoded as buffer copies so
they are ordered inside the same submission as the dispatches; readbacks copy to
staging and are mapped after the submission fence. The `GraphReport` records each
transfer's bytes, dispatch/workgroup counts, and the bytes of buffers that only
dispatches touched (resident). The example keeps 2·N·4 bytes resident, uploads 16
bytes, and reads back N/8 summaries, verified against an independent host reference
for N in {0, 1, 7, 8, 9, 63, 64, 65, 257, 1000} and across two settings updates.
Still true: nodes execute in insertion order; declared edges are checked, not used
for scheduling. No inference, no reordering, no transfer planner.

`BufferBinding::independent_length()` was added for this proof: a one-element
settings buffer and an N→N/8 reduction are impossible under the shared
element-count rule. It is opt-in per binding, the kernel must guard that buffer
with its own `.len()`, and an empty independent buffer is rejected when the
dispatch has work.

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
