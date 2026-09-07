# Decisions and boundaries

Recorded 2026-09-07 from the owner's GUST handoff, earlier reflection deferral,
and independent inspection. Chat research is design input, not proof of code state.

| ID | Decision | Rationale / revisit condition |
| --- | --- | --- |
| D01 | GUST project identity; retain GPU Dialect crate names | Preserve working users and avoid cosmetic package churn |
| D02 | Keep syn → Slang vertical slice | Existing real GPU behavior is useful; no custom instruction IR or SPIR-V emitter |
| D03 | Separate implemented, researched, aspirational claims | Engine and graph plans are not a shipped subsystem |
| D04 | No CPU execution contract for shader shadows | Independent test references and intentional future CPU nodes remain distinct |
| D05 | Reflection slice 1 authorized on 2026-09-07 | Explicit compiler JSON inspection and POD cross-checks; missing layout evidence must remain unknown. Runtime ABI expansion and automatic reflection enforcement remain separate work |
| D06 | Fix translation correctness before language breadth | Reproduced tail-return, literal, precedence, and struct-constructor issues |
| D07 | Field-aware struct construction in bounded contexts (supersedes the temporary rejection) | Construct a fresh temporary by field name in source order, then assign the destination once; never overwrite a value while evaluating its replacement. Supported as a `let` initializer and assignment RHS; record updates (`..base`), other expression positions, and nested literals remain rejected |
| D08 | Portable tiers plus capability-gated native paths | Avoid both universal feature claims and permanent bans on native acceleration |
| D09 | Fixed-size elements plus separate dynamic metadata | No nested runtime-sized structured-buffer element layout |
| D10 | Extract a small explicit staged graph later | Typed ordered batches already provide a useful proof; no speculative graph framework now |
| D11 | Engine proofs before ECS architecture commitment | Pools, dependencies, indirect work, then culling/rendering; no premature mega-buffer |
| D12 | Manual file-based Codex/local-AI handoff | No local model endpoint or concurrent-agent protocol has been provided |
| D13 | Preserve non-Git checkout | Baseline hashes and changed-file manifest aid review but are not rollback storage |
| D14 | Lower `Option<T>` to Slang `Optional<T>` instead of banning std-prelude type names (owner, 2026-09-07) | First VISION "deterministic lowering" row made executable. Bounded to locals and helper signatures with scalar/struct payloads; `Some`/`None`/`is_some`/`is_none`/`unwrap_or`/`if let Some(x)` only. Struct-field and buffer-element Options, `unwrap`/`expect`, `Result`, `match`, and `?` stay rejected until separately proven. Non-32-bit primitives are still not emulated: WGSL lacks them and the storage ABI stays four-byte |

Rejected for this pass: rebuilding the shader backend, crate renaming, automatic
CPU fallback, presenting manual metadata as Slang reflection, unrestricted Rust
acceptance, and using copied research examples as untested portable engine code.

Open decisions: rustc integration and version pinning; source maps; typed inference
policy (struct construction is now bounded and proven, see D07); layouts after
reflection; first engine user's unaided acceptance result; entity storage;
shader/module hygiene; capability manifest schema.
