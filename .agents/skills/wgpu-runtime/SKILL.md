---
name: wgpu-runtime
description: "Implement or debug headless wgpu compute execution, shader module validation, bind groups, buffer layout, dispatch, asynchronous readback or CPU/GPU comparisons for this compiler. Use for actual runtime behavior, not solely Slang text generation."
---

# wgpu runtime

Consume compiled shader artifacts plus explicit binding/layout metadata; keep syn internals out of the runtime. Match the repository's pinned wgpu API and features.

1. Record adapter name, device type, backend, requested features/limits and shader hash. A missing adapter is unavailable execution, not a passing kernel.
2. Validate shader and pipeline, bind exactly the declared ABI, dispatch rounded-up workgroups, copy to staging, wait for mapping and compare readback.
3. Test partial groups, empty input and invalid host lengths. Propagate validation, device, mapping and comparison errors.
4. Do not substitute CPU execution or a different shader when reporting GPU success.

Read [headless execution](references/headless-execution.md) for buffer lifecycle and test requirements. [Test-Headless.ps1](scripts/Test-Headless.ps1) runs one existing standard Rust integration test by exact name and rejects zero/ignored tests. It certifies test execution, not the test's implementation; inspect the test for real GPU assertions.
