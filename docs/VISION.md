# GUST vision

Status: researched direction, not a description of a shipped compiler or engine.
The executable subset is described in [architecture](ARCHITECTURE.md).

GUST — GPU <3 rUST — aims to be a heterogeneous Rust execution compiler, ultimately
supporting a GPU-centric ECS game engine. Rust supplies high-level semantics and
tooling; Slang supplies portable GPU kernels and target legalization; GUST connects
CPU work, GPU work, resource residency, and their dependencies. It is not merely
an arbitrary Rust-to-SPIR-V transpiler and does not promise ordinary Rust execution
on a shader core.

## Compiler evolution

Today: syn syntax → validated narrow shader surface → Slang. rustc checks private
shadow bodies separately; the proc macro does not consume rustc-resolved types.

Proposed semantic frontend: AST/HIR for source structure, THIR/resolved types for
operators and method resolution, selective MIR for dataflow, effects, borrow/drop
obligations. Lower high-level execution into a GUST Execution Graph. CPU nodes
remain ordinary Rust; GPU nodes become Slang → WGSL/SPIR-V/native targets. Pinning
and maintaining a rustc integration is a future engineering decision, not an excuse
to discard the useful stable proc-macro path now.

## Rust feature treatment (candidates, not promises)

| Treatment | Candidate features | Proof needed |
| --- | --- | --- |
| Direct/native-ish Slang mapping | Structs, functions, methods, control flow, selected generics/traits/associated types | Resolved meaning, specialization, target support |
| Erase after checking | Lifetimes, move checking, Copy/Clone facade | No observable runtime obligation lost |
| Deterministic lowering | Result and `?`, payload enums/match, references/slices, closures/iterators, cleanup, virtual pointers | Evaluation order, layout, effects and drop semantics |
| Execution-graph meaning | Explicit CPU/GPU regions, parallel/dynamic work, async/spawn/join, continuations | Dependencies, resource ownership, bounded progress |

An apparent syntax match is not proof of semantic equivalence. Unsupported constructs
must be rejected clearly until the necessary information and lowering exist. Rust
shadow types are a checking facade, not a hidden CPU interpreter.

## Optimization principles

Normal branches are allowed. If-conversion requires uniformity, purity,
speculatability, and cost analysis; never eagerly execute side effects, invalid
loads, atomics, or barriers from a branch that would not run.

Loops require classification: map, filter/compaction, reduction, scan, gather,
scatter with conflict policy, dynamic frontier, truly sequential dependence, or
small fixed unroll. Analyze live-in/live-out values, access/alias sets, execution
domain, and ordering before parallelizing. A Rust `for` is not automatically a
dispatch. Dynamic workloads use bounded local work plus continuation/frontier
storage and subsequent dispatches; never assume global synchronization in one kernel.

Explicit CPU work (input, filesystem, network, windowing, asset loading, ordinary
gameplay where appropriate) is intentional, not a fallback for unsupported GPU
operations. Transfer only required live data; keep large particle/component pools
resident and exchange small settings, counts, or summaries.

## Acceptance-first evolution

Near-term user: an engineer runs the documented validation command without editing
shader strings and sees Rust-authored kernels compile, execute, and match independent
expected results, with inspectable Slang/WGSL/SPIR-V artifacts.

Next execution-model proof: an engineer runs one typed staged pipeline, observes
correct intermediate dependencies, and can inspect which resources cross CPU/GPU
boundaries. Only then generalize a graph API. The engine destination guides those
proofs without turning the prototype into a speculative engine framework.
