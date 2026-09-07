# Engineering agreement

GUST is the project; GPU Dialect is the existing Rust → Slang subsystem. Read
`.ai/STATUS.md`, `.ai/NEXT_TASKS.md`, and `.ai/HANDOFF.md` before editing. Consult
`docs/DECISIONS.md` and `docs/ARCHITECTURE.md` for constraints. Design aspirations
are not implemented features or permission to perform external actions.

## Invariants

- Preserve `#[gpu]` whole-module authoring, direct syn → Slang, readable artifacts,
  real GPU tests, typed buffers, pipeline caching, ordered batches, and async jobs.
- No new custom shader instruction IR or handwritten SPIR-V backend. An eventual
  execution graph is a high-level CPU/GPU scheduling model, not a replacement for Slang.
- Shadow signatures exist for Rust checking; their bodies are not a CPU execution
  contract. No `.cpu()` wrappers or CPU fallback. Independent host test references
  and intentional future CPU nodes are different things and remain allowed.
- Do not claim compiler reflection: bindings and restricted POD layouts are assigned
  by the macro. Reflection was explicitly deferred by the owner.
- Portable behavior is required; native accelerations need capability gates and
  the same visible semantics. No GPU inter-workgroup spin waits.
- Prefer a diagnostic over silently translating unsupported Rust semantics.
- Do not rename crates, initialize Git, remove working examples, add dependencies,
  or expand architecture merely to align branding. Explain necessary changes.

## Workflow and verification

1. Check the working tree (or hashes if no Git), installed tools, and baseline tests.
2. Claim a bounded task and file scope in `.ai/STATUS.md` before work. If another
   agent owns overlapping files, coordinate with the human; do not overwrite them.
3. Add a failing regression first when fixing a bug. Test compiler output and actual
   GPU behavior, not only substring assertions. No silent GPU/compiler test skips
   in new validation; clearly distinguish optional external validation.
4. Run from the workspace root:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p vector-add
cargo run -p typed-pipeline
```

Use stable Rust, `slangc` on PATH, and a Vulkan adapter. For release evidence run
`scripts/verify.ps1 -Full`: it requires SPIRV-Tools, runs every example, and validates
all exported SPIR-V files. Missing tools are blockers, not passes. Baseline versions
and actual results belong in `.ai/STATUS.md`; commands alone are not evidence.
The script overwrites `.ai/VALIDATION.json` with the latest pass/failure record;
inspect its timestamp and scope before using it as evidence.

## Artifact and collaboration rules

- `generated-wgpu/` contains derived `.slang`, `.wgsl`, `.spv`, and readable `.rs`
  host examples. Regenerate by running the five examples; never hand-edit exports.
  The generated host snippets are inspectable examples, not standalone packages.
- `tests/fixtures/semantics.slang` is a reviewed expectation, not an auto-updated
  export. A translation change requires an explained golden diff and GPU tests.
- Keep commits small if Git is available. This takeover began without Git;
  `.ai/BASELINE.sha256` records source/artifact hashes before edits, not a backup.
- Do not change account allowances, spend credits, contact another AI, or start
  background work without authority. A local AI resumes manually from these files.
- Before handing off, clear ownership, record exact files/tests/known failures and
  the next command. Do not mark researched plans complete. Record reliable allowance
  readings only; the human's allowance stop policy applies to this Codex run, not
  an endless instruction for a future local agent.
