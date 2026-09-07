# Headless execution contract

Use the wgpu version in Cargo.lock; use version-specific docs when editing API calls. No surface/window is required for compute. Request an adapter/device using the project's backend policy, report adapter information and negotiate only required features/limits. Software adapters must be labeled; do not call them an RTX hardware run.

Load the exact shader file under test. WGSL is the portable WebGPU source path. In Rust wgpu, `ShaderSource::SpirV` requires the `spirv` crate feature and takes words; use the pinned version's safe ingestion utilities. Do not use unsafe passthrough to evade validation. Successful WGSL execution establishes only that target's runtime path.

Keep buffer sizes, element stride, binding group/index, read/write permissions and minimum binding sizes consistent with emitter metadata. Use storage buffers for compute and a separate `MAP_READ | COPY_DST` staging buffer for readback; add `COPY_SRC` to output. Upload via the queue or suitable initialized buffers. Host repr(C) alone does not prove shader packing.

Dispatch ceil(n/workgroup_size) groups without arithmetic overflow and check device limits. The shader must guard every accessed buffer, or the host must enforce a shared valid length. Handle n=0 deliberately on the host before creating invalid zero-sized bindings or dispatching. Submit work, request asynchronous mapping, drive completion with the pinned version's polling/event API, check callback errors, copy mapped bytes into owned host memory, drop mapped views and unmap before reuse. Bound waits; do not hang CI indefinitely.

The exact-test script supports standard Rust libtest integration targets and runs with `--include-ignored`. The selected test must fail on unavailable adapters, validation/mapping errors or numerical mismatches if used for GPU acceptance. A test that returns early can still pass libtest; reviewers must check that behavior. Features are explicit. Full vertical verification additionally requires a machine-readable runtime report; see the vertical-slice adapter contract.

Sources checked 2026-09-07: [wgpu ShaderSource](https://docs.rs/wgpu/latest/wgpu/enum.ShaderSource.html), [Buffer](https://docs.rs/wgpu/latest/wgpu/struct.Buffer.html), [Device](https://docs.rs/wgpu/latest/wgpu/struct.Device.html). Live docs describe wgpu 30.x; preserve an existing older lockfile.
