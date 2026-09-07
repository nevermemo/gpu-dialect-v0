# GPU-centric ECS engine destination

## Implemented now

Headless compute, persistent typed buffers, ordered batches, async jobs, timestamp
measurements, and particle/sensor examples. No ECS scheduler, rendering, entity
lifecycle, dynamic pool growth, indirect dispatch, or GPU allocation framework yet.

## Researched direction

GPU-resident component pools for positions, velocities, transforms, animation,
visibility, and other parallel state. Entity IDs/generations provide stable identity
without exposing native pointers. Compare archetype, sparse-set, and hybrid storage,
including SoA/AoS choices, query costs, structural changes, and safe deletion. Do not
choose one mega-buffer by default. A scheduler may later infer system dependencies
from `Read<T>` / `Write<T>` and effect information; initial proofs use explicit edges.

Dynamic collections need separate buffer allocation, logical length, and capacity.
Host-driven geometric growth should allocate → copy live GPU bytes → refresh
bindings → retire old resources after safe completion. No resize readback and no
automatic shrink. Test zero length, growth boundaries, stale handles, device
ownership, and in-flight work. Pools, arenas, freelists, and compaction are future
options, not requirements for the first vector-like collection.

Geometry uses fixed-size vertex/index pools plus mesh metadata/count/offsets. Do
not use an unsized structure as a structured-buffer element (see [portability](PORTABLE_SLANG_CORE.md)).
Hierarchy can use parent indices plus levels/frontiers; ordinary recursion is not
the portable scheduling model. Textures/materials start with arrays/atlases,
material grouping and tables; native binding arrays require capability gates.

## Long-term aspiration

A frame may flow through input → gameplay → physics → animation → hierarchy and
transforms → culling/LOD → compaction → indirect arguments → rendering. This is
an organizing destination, not a committed monolithic pipeline. Portable rendering
may use grouped or repeated indirect draws; native multi-draw/mesh paths depend on
capabilities. Both should expose the same engine semantics without a CPU shader fallback.

## Small proofs before an engine framework

1. A component pool grows while retaining GPU-authored data, with explicit length
   and capacity and no host readback during growth.
2. Two typed systems declare read/write dependencies and cannot run out of order.
3. GPU active-count generation drives a validated indirect dispatch.
4. Culling writes compacted visibility and indirect arguments consumed by rendering.

Each proof needs a runnable example, failure cases, inspectable artifacts, and
measurements separating compile/setup, upload, dispatch, and readback. Benchmark
against independent CPU implementations; never conclude “GPU is better” from one
cold end-to-end timing or a debug build. Choose the first actual engine user and
observable unaided result before committing a broad ECS API.
