# Engineering agreement

GUST is the project; GPU Dialect is the existing Rust → Slang subsystem. Read
`.ai/STATUS.md`, `.ai/NEXT_TASKS.md`, and `.ai/HANDOFF.md` before editing. Consult
`docs/DECISIONS.md` and `docs/ARCHITECTURE.md` for constraints. Design aspirations
are not implemented features or permission to perform external actions.

# AI Swarm Instructions

## Goal

Build and maintain this project using a small heterogeneous AI swarm.

The repository, tests and documentation are the shared source of truth.
Do not use long conversations as project memory.

## Routing

Use the cheapest capable worker.

- Scout:
  repository discovery, symbol search, dependency tracing, context gathering.

- Builder:
  implementation, Rust/compiler work, Slang generation, refactoring and debugging.

- Verifier:
  independent review, adversarial testing, compiler/test execution and failure analysis.

The orchestrator coordinates. It should avoid doing implementation itself.

## Workflow

For meaningful changes:

Scout → Builder → Verifier

Skip Scout when the affected files are already obvious.

Skip Builder for read-only questions.

Skip Verifier only for trivial changes.

Do not have multiple agents perform identical work unless:
- there is disagreement,
- confidence is low,
- tests fail,
- or the decision is architecturally important.

## Token discipline

Workers receive only the context required for their subtask.

Worker reports must be concise.
Do not return raw logs unless a failure requires them.
Do not paste large source files into reports.
Prefer file paths, symbols and line references.

Use repository files for durable knowledge.

## Truth hierarchy

Prefer, in order:

1. Tests and compiler/runtime results
2. Existing project invariants
3. Official language/tool documentation
4. Independent agent reasoning
5. Agent consensus

Never treat majority vote as proof.

## Architecture

Preserve the current working Rust AST → Slang pipeline unless explicitly asked to redesign it.

Load relevant Rust/Slang/testing skills only when needed.

## Completion

A meaningful code change is complete only when:

- Builder reports what changed.
- Relevant deterministic checks have run.
- Verifier reports PASS, or remaining uncertainty is explicitly surfaced.

Codex is used separately for important review, explanation and architectural guidance.
Do not wait for Codex during normal local swarm work.

## Invariants

- Preserve `#[gpu]` whole-module authoring, direct syn → Slang, readable artifacts,
  real GPU tests, typed buffers, pipeline caching, ordered batches, and async jobs.
- No new custom shader instruction IR or handwritten SPIR-V backend. An eventual
  execution graph is a high-level CPU/GPU scheduling model, not a replacement for Slang.
- Shadow signatures exist for Rust checking; their bodies are not a CPU execution
  contract. No `.cpu()` wrappers or CPU fallback. Independent host test references
  and intentional future CPU nodes are different things and remain allowed.
- Do not claim compiler reflection in the macro: bindings and restricted POD layouts
  are assigned by the macro. The runtime verifies them against Slang's reflection
  through the native helper before creating a pipeline (D17); that check is a gate,
  not a source of bindings.
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
