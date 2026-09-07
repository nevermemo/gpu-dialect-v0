# Decisions and boundaries

Recorded 2026-09-07 from the owner's GUST handoff, earlier reflection deferral,
and independent inspection. Chat research is design input, not proof of code state.

| ID | Decision | Rationale / revisit condition |
| --- | --- | --- |
| D01 | GUST project identity; retain GPU Dialect crate names | Preserve working users and avoid cosmetic package churn |
| D02 | Keep syn → Slang vertical slice | Existing real GPU behavior is useful; no custom instruction IR or SPIR-V emitter |
| D03 | Separate implemented, researched, aspirational claims | Engine and graph plans are not a shipped subsystem |
| D04 | No CPU execution contract for shader shadows | Independent test references and intentional future CPU nodes remain distinct |
| D05 | Reflection deferred | Owner explicitly requested stability first; prerequisite before wide resource/ABI expansion |
| D06 | Fix translation correctness before language breadth | Reproduced tail-return, literal, precedence, and struct-constructor issues |
| D07 | Field-aware struct construction in bounded contexts (supersedes the temporary rejection) | Slang has no field-name initializers, so lower to construct-then-assign by name in source order; this preserves field identity and evaluation order. Supported as a `let` initializer and assignment RHS; other expression positions and nested literals remain rejected |
| D08 | Portable tiers plus capability-gated native paths | Avoid both universal feature claims and permanent bans on native acceleration |
| D09 | Fixed-size elements plus separate dynamic metadata | No nested runtime-sized structured-buffer element layout |
| D10 | Extract a small explicit staged graph later | Typed ordered batches already provide a useful proof; no speculative graph framework now |
| D11 | Engine proofs before ECS architecture commitment | Pools, dependencies, indirect work, then culling/rendering; no premature mega-buffer |
| D12 | Manual file-based Codex/local-AI handoff | No local model endpoint or concurrent-agent protocol has been provided |
| D13 | Preserve non-Git checkout | Baseline hashes and changed-file manifest aid review but are not rollback storage |

Rejected for this pass: rebuilding the shader backend, crate renaming, automatic
CPU fallback, presenting manual metadata as Slang reflection, unrestricted Rust
acceptance, and using copied research examples as untested portable engine code.

Open decisions: rustc integration and version pinning; source maps; typed inference
policy (struct construction is now bounded and proven, see D07); layouts after
reflection; first engine user's unaided acceptance result; entity storage;
shader/module hygiene; capability manifest schema.
