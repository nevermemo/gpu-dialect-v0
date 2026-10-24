//! Component pool and indirect workload proofs (T08).
//!
//! A `GpuPool<Particle>` grows across frames while GPU-authored state survives
//! each growth (a GPU-to-GPU copy, never a readback). Every frame runs a graph:
//! `prepare_dispatch` derives the active count and indirect workgroup arguments
//! on the GPU from the pool's count buffer, clamped to the allocation and a
//! settings budget; `integrate` then runs as an indirect dispatch over exactly
//! that many particles. The host never learns the dispatched count. The graph is
//! built from typed calls, derives its dependencies, and is rebuilt after growth.

use std::{path::PathBuf, time::Instant};

use gust::{Independent, KernelDescriptor, gpu};
#[cfg(test)]
use gust_wgpu::{BufferBinding, BufferDispatch};
use gust_wgpu::{
    GpuBufferAccess, GpuPool, GraphReport, HeadlessDevice, StagedGraph, render_wgpu_source,
};

#[gpu]
mod pool {
    #[derive(Debug, PartialEq)]
    pub struct Particle {
        pub x: float,
        pub y: float,
        pub vx: float,
        pub vy: float,
        pub steps: uint,
    }

    #[derive(Debug, PartialEq)]
    pub struct Settings {
        pub dt: float,
        pub gravity: float,
        /// Upper bound on particles integrated per frame.
        pub budget: uint,
    }

    #[derive(Debug, PartialEq)]
    pub struct DispatchArgs {
        pub x: uint,
        pub y: uint,
        pub z: uint,
    }

    #[derive(Debug, PartialEq)]
    pub struct Active {
        pub count: uint,
    }

    fn min_uint(a: uint, b: uint) -> uint {
        if a < b { a } else { b }
    }

    // One invocation: derive the active count and the indirect arguments on the
    // GPU. The count buffer mirrors the pool's logical length; it is clamped to the
    // allocation (`particles.len()`) so a malformed count can never dispatch past
    // the buffer, and to the settings budget.
    #[allow(clippy::len_zero)]
    #[kernel(workgroup_size(1, 1, 1))]
    pub fn prepare_dispatch(
        id: SV_DispatchThreadID,
        count: StructuredBuffer<uint>,
        settings: StructuredBuffer<Settings>,
        particles: StructuredBuffer<Particle>,
        mut active: RWStructuredBuffer<Active>,
        mut args: RWStructuredBuffer<DispatchArgs>,
    ) {
        if id.x == 0u32 && 0u32 < count.len() && 0u32 < settings.len() {
            let live = min_uint(count[0u32], particles.len());
            let n = min_uint(live, settings[0u32].budget);
            active[0u32].count = n;
            // Overflow-free ceiling division: `n + 63` could wrap for a huge `n`.
            args[0u32].x = n / 64u32 + min_uint(n % 64u32, 1u32);
            args[0u32].y = 1u32;
            args[0u32].z = 1u32;
        }
    }

    // Indirect dispatch: the workgroup count came from `args`, so the guard uses
    // the GPU-derived active count and the allocation length, never a host count.
    #[allow(clippy::len_zero, clippy::assign_op_pattern)]
    #[kernel(workgroup_size(64, 1, 1))]
    pub fn integrate(
        id: SV_DispatchThreadID,
        active: StructuredBuffer<Active>,
        settings: StructuredBuffer<Settings>,
        mut particles: RWStructuredBuffer<Particle>,
    ) {
        let i = id.x;
        if 0u32 < active.len()
            && 0u32 < settings.len()
            && i < active[0u32].count
            && i < particles.len()
        {
            let s = settings[0u32];
            let mut p = particles[i];
            p.vy = p.vy + s.gravity * s.dt;
            p.x = p.x + p.vx * s.dt;
            p.y = p.y + p.vy * s.dt;
            p.steps = p.steps + 1u32;
            particles[i] = p;
        }
    }
}

pub use pool::{Active, DispatchArgs, Particle, Settings};

/// Particles integrated per frame at most. The indirect dispatch needs `BUDGET / 64` = 16 384 workgroups,
/// within the 65 535 workgroups per dimension of the default limits; wgpu would silently zero a larger
/// indirect dispatch, and the runtime cannot check it, so the kernel's clamp to the budget is what holds.
const BUDGET: u32 = 1 << 20;

pub fn spawn(first: usize, count: usize) -> Vec<Particle> {
    (first..first + count)
        .map(|index| Particle {
            x: (index % 17) as f32 * 0.5,
            y: (index % 13) as f32 * 0.25,
            vx: 1.0 + (index % 5) as f32 * 0.1,
            vy: (index % 7) as f32 * 0.2 - 0.6,
            steps: 0,
        })
        .collect()
}

/// Independent host reference for one frame over the first `min(len, budget)` particles.
pub fn step_cpu(particles: &mut [Particle], settings: &Settings) -> usize {
    let active = particles.len().min(settings.budget as usize);
    for p in &mut particles[..active] {
        p.vy += settings.gravity * settings.dt;
        p.x += p.vx * settings.dt;
        p.y += p.vy * settings.dt;
        p.steps += 1;
    }
    active
}

pub struct Frame {
    pub settings: gust_wgpu::GpuBuffer<Settings>,
    pub active: gust_wgpu::GpuBuffer<Active>,
    pub args: gust_wgpu::GpuBuffer<DispatchArgs>,
}

impl Frame {
    pub fn new(device: &HeadlessDevice) -> Result<Self, gust_wgpu::Error> {
        Ok(Self {
            settings: device.create_typed_buffer(
                "pool settings",
                &[Settings {
                    dt: 0.0,
                    gravity: 0.0,
                    budget: 0,
                }],
                GpuBufferAccess::ReadOnly,
            ),
            active: device.create_typed_buffer(
                "pool active",
                &[Active { count: 0 }],
                GpuBufferAccess::ReadWrite,
            ),
            args: device.create_indirect_buffer(
                "pool dispatch args",
                &[DispatchArgs { x: 0, y: 0, z: 0 }],
            )?,
        })
    }
}

/// [`run_frame_traced`] without the edges.
#[cfg(test)]
pub fn run_frame(
    device: &HeadlessDevice,
    frame: &Frame,
    particles: &GpuPool<Particle>,
    settings: &[Settings; 1],
) -> Result<(u32, GraphReport), gust_wgpu::Error> {
    let (count, report, _edges) = run_frame_traced(device, frame, particles, settings)?;
    Ok((count, report))
}

/// One frame: settings write → prepare_dispatch → indirect integrate → read the GPU-derived active
/// count back (the only readback; particles stay resident). Returns the count, the report, and the
/// dependency edges the graph derived, per node (node indices, ascending).
///
/// The graph is rebuilt every frame: growth replaces the pool's allocation, and the typed calls (locals
/// built before the graph, which borrows them) bind the allocation the pool holds right now. Nothing
/// is declared by the host. The readback of `active` depends on `prepare_dispatch`, its last writer;
/// `integrate` only reads it.
pub fn run_frame_traced(
    device: &HeadlessDevice,
    frame: &Frame,
    particles: &GpuPool<Particle>,
    settings: &[Settings; 1],
) -> Result<(u32, GraphReport, Vec<Vec<usize>>), gust_wgpu::Error> {
    // Every buffer is bound `Independent`: none has the dispatch's element count as its length (the
    // pool's allocation is its capacity), and an indirect dispatch has no host element count at all.
    let prepare_call = pool::prepare_dispatch::call(
        1,
        Independent(particles.count_buffer()),
        Independent(&frame.settings),
        Independent(particles.buffer()),
        Independent(&frame.active),
        Independent(&frame.args),
    );
    let integrate_call = pool::integrate::call(
        0,
        Independent(&frame.active),
        Independent(&frame.settings),
        Independent(particles.buffer()),
    );

    let mut graph = StagedGraph::new();
    let write = graph.write(&frame.settings, settings);
    let prepare = graph.run(&prepare_call);
    let integrate = graph.run_indirect(&integrate_call, &frame.args);
    let active = graph.read(&frame.active);
    let edges = [write, prepare, integrate, active]
        .map(|node| graph.dependency_indices(node))
        .to_vec();
    let output = device.execute_graph(&graph)?;
    let count = output.readback::<Active>(active)?[0].count;
    Ok((count, output.report, edges))
}

pub fn descriptors() -> [&'static KernelDescriptor; 2] {
    [
        &pool::prepare_dispatch::DESCRIPTOR,
        &pool::integrate::DESCRIPTOR,
    ]
}

pub fn assert_close(actual: &[Particle], expected: &[Particle]) {
    assert_eq!(actual.len(), expected.len());
    let close = |a: f32, b: f32| (a - b).abs() <= 1.0e-4 * b.abs().max(1.0);
    for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        assert!(
            close(actual.x, expected.x)
                && close(actual.y, expected.y)
                && close(actual.vx, expected.vx)
                && close(actual.vy, expected.vy),
            "particle[{index}]: {actual:?} vs {expected:?}"
        );
        assert_eq!(actual.steps, expected.steps, "steps[{index}]");
    }
}

fn export_artifacts() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../generated-wgpu");
    std::fs::create_dir_all(&directory)?;
    for descriptor in descriptors() {
        let stem = format!("{}__{}", descriptor.module, descriptor.name);
        std::fs::write(
            directory.join(format!("{stem}.slang")),
            descriptor.slang_source,
        )?;
        std::fs::write(
            directory.join(format!("{stem}.wgsl")),
            gust::slang::compile_wgsl(descriptor)?,
        )?;
        std::fs::write(
            directory.join(format!("{stem}.spv")),
            gust::spirv::words_as_le_bytes(&gust::slang::compile_spirv(descriptor)?),
        )?;
        std::fs::write(
            directory.join(format!("{stem}.rs")),
            render_wgpu_source(descriptor)?,
        )?;
    }
    Ok(directory.canonicalize().unwrap_or(directory))
}

pub fn main() -> Result<(), Box<dyn std::error::Error>> {
    let device = HeadlessDevice::new()?;
    let frame = Frame::new(&device)?;
    let settings = Settings {
        dt: 0.016,
        gravity: -9.8,
        budget: BUDGET,
    };

    let initial = if cfg!(debug_assertions) {
        1 << 12
    } else {
        1 << 16
    };
    let mut reference = spawn(0, initial);
    let mut particles = device.create_pool("particles", &reference, initial);
    println!(
        "Component pool: start {initial} particles, capacity {}",
        particles.capacity()
    );

    let frames = 6;
    let mut total_copied = 0;
    let mut last_edges = Vec::new();
    for frame_index in 0..frames {
        let started = Instant::now();
        let (active, report, edges) = run_frame_traced(&device, &frame, &particles, &[settings])?;
        last_edges = edges;
        let frame_elapsed = started.elapsed();
        let cpu_started = Instant::now();
        let expected_active = step_cpu(&mut reference, &settings);
        let cpu_elapsed = cpu_started.elapsed();
        assert_eq!(active as usize, expected_active);

        // Grow after each frame: GPU-authored positions must survive the copy.
        let spawned = spawn(reference.len(), reference.len());
        let growth = device.pool_push(&mut particles, &spawned)?;
        reference.extend(spawned);
        if let Some(growth) = growth {
            total_copied += growth.copied_bytes;
        }
        let reclaimed = device.pool_reclaim(&mut particles)?;
        println!(
            "frame {frame_index}: active {active}, graph {frame_elapsed:?} (CPU reference {cpu_elapsed:?}), upload {} B, readback {} B, resident {} B, growth {growth:?}, reclaimed {reclaimed}",
            report.upload_bytes, report.readback_bytes, report.resident_bytes
        );
    }

    let actual = device.pool_read(&particles)?;
    assert_close(&actual, &reference);
    let artifacts = export_artifacts()?;
    println!(
        "final: {} particles, capacity {}, generation {}, {} growths, {} bytes copied GPU-to-GPU, {} retired allocations pending",
        particles.len(),
        particles.capacity(),
        particles.generation(),
        particles.growth_history().len(),
        total_copied,
        particles.retired_allocations()
    );
    println!("adapter: {}", device.adapter_info().name);
    println!(
        "derived dependencies per node (write, prepare_dispatch, integrate, read; declared: none): {last_edges:?}"
    );
    println!("pipeline cache: {:?}", device.pipeline_cache_stats());
    println!("generated artifacts: {}", artifacts.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use gust_wgpu::Error;

    fn settings(budget: u32) -> Settings {
        Settings {
            dt: 0.5,
            gravity: -2.0,
            budget,
        }
    }

    #[test]
    fn the_budget_keeps_the_indirect_dispatch_within_the_default_workgroup_limit() {
        let groups = BUDGET.div_ceil(pool::integrate::DESCRIPTOR.workgroup_size[0]);
        assert!(
            groups <= gust_wgpu::DEFAULT_MAX_COMPUTE_WORKGROUPS_PER_DIMENSION,
            "a budget of {BUDGET} particles needs {groups} workgroups"
        );
    }

    #[test]
    #[ignore = "example validation runs only in full verification"]
    fn every_stage_compiles_to_wgsl() {
        for descriptor in descriptors() {
            let wgsl = gust::slang::compile_wgsl(descriptor).unwrap();
            assert!(wgsl.contains("@compute"));
        }
    }

    #[test]
    #[ignore = "example validation runs only in full verification"]
    fn gpu_authored_state_survives_growth_without_readback() {
        let device = HeadlessDevice::new().expect("Vulkan adapter");
        let frame = Frame::new(&device).unwrap();
        let mut reference = spawn(0, 3);
        let mut particles = device.create_pool("particles", &reference, 4);
        assert_eq!((particles.len(), particles.capacity()), (3, 4));

        // Frame 1 mutates the three live particles on the GPU.
        let (active, _) = run_frame(&device, &frame, &particles, &[settings(1000)]).unwrap();
        assert_eq!(active, 3);
        step_cpu(&mut reference, &settings(1000));

        // Growth boundaries: 4 -> 8 (geometric), 8 -> 70 (required exceeds doubling).
        let spawned = spawn(3, 2);
        let growth = device.pool_push(&mut particles, &spawned).unwrap().unwrap();
        reference.extend(spawned);
        assert_eq!(
            growth,
            gust_wgpu::GrowthRecord {
                old_capacity: 4,
                new_capacity: 8,
                copied_bytes: 3 * size_of::<Particle>() as u64
            }
        );
        let spawned = spawn(5, 65);
        let growth = device.pool_push(&mut particles, &spawned).unwrap().unwrap();
        reference.extend(spawned);
        assert_eq!((growth.old_capacity, growth.new_capacity), (8, 70));
        assert_eq!(growth.copied_bytes, 5 * size_of::<Particle>() as u64);
        assert!(device.pool_push(&mut particles, &[]).unwrap().is_none());
        assert_eq!(
            (
                particles.len(),
                particles.capacity(),
                particles.generation()
            ),
            (70, 70, 2)
        );

        // The mutated state from frame 1 is intact after two copies, and frame 2
        // integrates all 70 (two workgroups, partial second) through the indirect path.
        assert_close(&device.pool_read(&particles).unwrap(), &reference);
        let (active, report, edges) =
            run_frame_traced(&device, &frame, &particles, &[settings(1000)]).unwrap();
        assert_eq!(active, 70);
        // The readback of `active` depends on prepare_dispatch (its last writer), not on integrate.
        assert_eq!(edges, [vec![], vec![0], vec![0, 1], vec![1]]);
        assert_eq!((report.derived_edges, report.declared_edges), (4, 0));
        assert_eq!(report.indirect_dispatches, 1);
        assert_eq!(report.dispatches, 2);
        assert_eq!(report.readback_bytes, size_of::<Active>() as u64);
        assert_eq!(report.upload_bytes, size_of::<Settings>() as u64);
        step_cpu(&mut reference, &settings(1000));
        assert_close(&device.pool_read(&particles).unwrap(), &reference);

        // Retirement: both copies have completed by now.
        assert_eq!(particles.retired_allocations(), 2);
        assert_eq!(device.pool_reclaim(&mut particles).unwrap(), 2);
        assert_eq!(particles.retired_allocations(), 0);
    }

    #[test]
    #[ignore = "example validation runs only in full verification"]
    fn active_count_is_derived_on_the_gpu_and_clamped() {
        let device = HeadlessDevice::new().expect("Vulkan adapter");
        let frame = Frame::new(&device).unwrap();
        let mut reference = spawn(0, 130);
        let mut particles = device.create_pool("particles", &reference, 130);

        // Budget below the length: only the first 100 move.
        let (active, _) = run_frame(&device, &frame, &particles, &[settings(100)]).unwrap();
        assert_eq!(active, 100);
        assert_eq!(step_cpu(&mut reference, &settings(100)), 100);
        assert_close(&device.pool_read(&particles).unwrap(), &reference);

        // Truncate: the count buffer shrinks, so fewer particles are active, and
        // the read-back prefix shrinks with it.
        device.pool_truncate(&mut particles, 65).unwrap();
        reference.truncate(65);
        let (active, _) = run_frame(&device, &frame, &particles, &[settings(1000)]).unwrap();
        assert_eq!(active, 65);
        step_cpu(&mut reference, &settings(1000));
        assert_close(&device.pool_read(&particles).unwrap(), &reference);
        assert!(matches!(
            device.pool_truncate(&mut particles, 66),
            Err(Error::PoolTruncateGrows {
                len: 65,
                requested: 66
            })
        ));

        // Zero length: no particle is active, nothing changes, nothing panics.
        device.pool_truncate(&mut particles, 0).unwrap();
        let (active, report) = run_frame(&device, &frame, &particles, &[settings(1000)]).unwrap();
        assert_eq!(active, 0);
        assert_eq!(report.indirect_dispatches, 1);
        assert!(device.pool_read(&particles).unwrap().is_empty());

        // Push after truncation reuses capacity and resumes from the logical length.
        let spawned = spawn(0, 2);
        assert!(
            device
                .pool_push(&mut particles, &spawned)
                .unwrap()
                .is_none()
        );
        assert_eq!(particles.len(), 2);
        assert_eq!(device.pool_read(&particles).unwrap(), spawned);
    }

    #[test]
    #[ignore = "example validation runs only in full verification"]
    fn growth_while_a_frame_is_in_flight_preserves_its_writes() {
        // Queue order: the growth copy runs after the still-executing integrate,
        // so the new allocation observes that frame's writes.
        let device = HeadlessDevice::new().expect("Vulkan adapter");
        let frame = Frame::new(&device).unwrap();
        let mut reference = spawn(0, 64);
        let mut particles = device.create_pool("particles", &reference, 64);
        device
            .write_typed_buffer(&frame.settings, &[settings(1000)])
            .unwrap();
        device
            .write_typed_buffer(&frame.active, &[Active { count: 64 }])
            .unwrap();
        // Submit without waiting, then grow immediately; the typed call borrowed the old allocation
        // only inside `submit`, so `pool_push` can take the pool mutably.
        let job = pool::integrate::submit(
            &device,
            64,
            Independent(&frame.active),
            Independent(&frame.settings),
            Independent(particles.buffer()),
        )
        .unwrap();
        let spawned = spawn(64, 64);
        let growth = device.pool_push(&mut particles, &spawned).unwrap().unwrap();
        assert_eq!((growth.old_capacity, growth.new_capacity), (64, 128));
        job.wait().unwrap();
        step_cpu(&mut reference, &settings(1000));
        reference.extend(spawned);
        assert_close(&device.pool_read(&particles).unwrap(), &reference);
    }

    #[test]
    #[ignore = "example validation runs only in full verification"]
    fn indirect_dispatch_rejects_wrong_buffers_and_bindings() {
        let device = HeadlessDevice::new().expect("Vulkan adapter");
        let frame = Frame::new(&device).unwrap();
        let particles = device.create_pool("particles", &spawn(0, 8), 8);
        let settings = [settings(1000)];
        let integrate_bindings = [
            BufferBinding::read_only(&frame.active).independent_length(),
            BufferBinding::read_only(&frame.settings).independent_length(),
            BufferBinding::read_write(particles.buffer()).independent_length(),
        ];
        let integrate = BufferDispatch::new(&pool::integrate::DESCRIPTOR, 0, &integrate_bindings);

        // Args must come from create_indirect_buffer; a plain storage buffer of
        // the right layout is rejected before any GPU work.
        let plain = device.create_typed_buffer(
            "plain args",
            &[DispatchArgs { x: 1, y: 1, z: 1 }],
            GpuBufferAccess::ReadWrite,
        );
        let mut graph = StagedGraph::new();
        graph.dispatch_indirect(integrate, &plain, &[]);
        assert!(matches!(
            device.execute_graph(&graph),
            Err(Error::IndirectArgsLayout(_))
        ));

        // The wrong element layout cannot even create an indirect buffer.
        assert!(matches!(
            device.create_indirect_buffer("wrong", &[Active { count: 1 }]),
            Err(Error::IndirectArgsLayout("Active"))
        ));
        assert!(matches!(
            device.create_indirect_buffer("wrong", &[1u32, 1, 1]),
            Err(Error::IndirectArgsLayout("u32"))
        ));

        // Every binding of an indirect dispatch must opt in to an independent length.
        let strict = [
            BufferBinding::read_only(&frame.active),
            BufferBinding::read_only(&frame.settings).independent_length(),
            BufferBinding::read_write(particles.buffer()).independent_length(),
        ];
        let mut graph = StagedGraph::new();
        graph.dispatch_indirect(
            BufferDispatch::new(&pool::integrate::DESCRIPTOR, 0, &strict),
            &frame.args,
            &[],
        );
        assert!(matches!(
            device.execute_graph(&graph),
            Err(Error::IndirectBindingLength { parameter }) if parameter == "active"
        ));

        // The args buffer is a read for hazard checking: a dispatch that reads
        // args written by prepare_dispatch must declare that edge.
        let prepare_bindings = [
            BufferBinding::read_only(particles.count_buffer()).independent_length(),
            BufferBinding::read_only(&frame.settings).independent_length(),
            BufferBinding::read_only(particles.buffer()).independent_length(),
            BufferBinding::read_write(&frame.active).independent_length(),
            BufferBinding::read_write(&frame.args).independent_length(),
        ];
        let mut graph = StagedGraph::new();
        let upload = graph.upload(&frame.settings, &settings, &[]);
        graph.dispatch(
            BufferDispatch::new(&pool::prepare_dispatch::DESCRIPTOR, 1, &prepare_bindings),
            &[upload],
        );
        graph.dispatch_indirect(integrate, &frame.args, &[upload]);
        assert!(matches!(
            device.execute_graph(&graph),
            Err(Error::GraphMissingDependency {
                node: 2,
                producer: 1
            })
        ));

        // The derived graph over the same buffers gets the edge the rejected graph lacked: integrate
        // depends on prepare_dispatch (through `args`, `active` and the particles) without a declared
        // edge, and the readback depends on prepare_dispatch, not integrate.
        let prepare_call = pool::prepare_dispatch::call(
            1,
            Independent(particles.count_buffer()),
            Independent(&frame.settings),
            Independent(particles.buffer()),
            Independent(&frame.active),
            Independent(&frame.args),
        );
        let integrate_call = pool::integrate::call(
            0,
            Independent(&frame.active),
            Independent(&frame.settings),
            Independent(particles.buffer()),
        );
        let mut derived = StagedGraph::new();
        let write = derived.write(&frame.settings, &settings);
        let prepare = derived.run(&prepare_call);
        let integrate = derived.run_indirect(&integrate_call, &frame.args);
        let read = derived.read(&frame.active);
        let edges = |node| derived.dependency_indices(node);
        assert_eq!(
            [edges(write), edges(prepare), edges(integrate), edges(read)],
            [vec![], vec![0], vec![0, 1], vec![1]]
        );
        assert!(
            [write, prepare, integrate, read]
                .into_iter()
                .all(|node| derived.is_derived(node))
        );
        let output = device.execute_graph(&derived).unwrap();
        assert_eq!(
            (output.report.derived_edges, output.report.declared_edges),
            (4, 0)
        );
        assert_eq!(output.readback::<Active>(read).unwrap()[0].count, 8);
    }
}
