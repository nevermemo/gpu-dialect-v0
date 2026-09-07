//! First explicit staged execution graph (T07).
//!
//! One CPU settings update is uploaded, GPU stage A scales and shifts a resident
//! sample buffer into a resident intermediate, GPU stage B reduces every block of
//! `BLOCK` intermediates into one summary, and only the small summary buffer is
//! read back. Dependencies are declared by the host and checked by the runtime;
//! nothing is inferred from the kernels.

use std::{path::PathBuf, time::Instant};

use gpu_dialect::{KernelDescriptor, gpu};
use gpu_dialect_wgpu::{
    BufferBinding, BufferDispatch, GpuBufferAccess, GraphReport, HeadlessDevice, StagedGraph,
    render_wgpu_source,
};

/// Intermediates reduced into one summary element.
pub const BLOCK: u32 = 8;

#[gpu]
mod staged {
    #[derive(Debug, PartialEq)]
    pub struct Settings {
        pub scale: float,
        pub offset: float,
        pub threshold: float,
        pub block: uint,
    }

    #[derive(Debug, PartialEq)]
    pub struct Summary {
        pub total: float,
        pub peak: float,
        pub above_threshold: uint,
        pub count: uint,
    }

    // Stage A: one element per sample. `settings` is a one-element buffer, bound
    // with an independent length and guarded by its own `.len()` (the dialect has
    // no `.is_empty()`, hence the lint allowance).
    #[allow(clippy::len_zero)]
    #[kernel(workgroup_size(64, 1, 1))]
    pub fn transform(
        id: SV_DispatchThreadID,
        samples: StructuredBuffer<float>,
        settings: StructuredBuffer<Settings>,
        mut intermediate: RWStructuredBuffer<float>,
    ) {
        let i = id.x;
        if i < intermediate.len() && 0u32 < settings.len() {
            let parameters = settings[0u32];
            intermediate[i] = samples[i] * parameters.scale + parameters.offset;
        }
    }

    // Stage B: one element per block. Reads `block` consecutive intermediates and
    // writes one summary; both `intermediate` and `settings` have independent lengths.
    // The block is unrolled because loops are not in the dialect yet.
    #[allow(clippy::assign_op_pattern)]
    fn fold(acc: Summary, value: float, threshold: float) -> Summary {
        let mut next = Summary {
            total: acc.total + value,
            peak: acc.peak,
            above_threshold: acc.above_threshold,
            count: acc.count + 1u32,
        };
        if value > next.peak {
            next.peak = value;
        }
        if value > threshold {
            next.above_threshold = next.above_threshold + 1u32;
        }
        next
    }

    #[allow(clippy::len_zero)]
    #[kernel(workgroup_size(64, 1, 1))]
    pub fn summarize(
        id: SV_DispatchThreadID,
        intermediate: StructuredBuffer<float>,
        settings: StructuredBuffer<Settings>,
        mut summaries: RWStructuredBuffer<Summary>,
    ) {
        let b = id.x;
        if b < summaries.len() && 0u32 < settings.len() {
            let parameters = settings[0u32];
            let start = b * parameters.block;
            let limit = intermediate.len();
            let mut acc = Summary {
                total: 0.0f32,
                peak: 0.0f32,
                above_threshold: 0u32,
                count: 0u32,
            };
            if start < limit {
                acc = fold(acc, intermediate[start], parameters.threshold);
            }
            if start + 1u32 < limit {
                acc = fold(acc, intermediate[start + 1u32], parameters.threshold);
            }
            if start + 2u32 < limit {
                acc = fold(acc, intermediate[start + 2u32], parameters.threshold);
            }
            if start + 3u32 < limit {
                acc = fold(acc, intermediate[start + 3u32], parameters.threshold);
            }
            if start + 4u32 < limit {
                acc = fold(acc, intermediate[start + 4u32], parameters.threshold);
            }
            if start + 5u32 < limit {
                acc = fold(acc, intermediate[start + 5u32], parameters.threshold);
            }
            if start + 6u32 < limit {
                acc = fold(acc, intermediate[start + 6u32], parameters.threshold);
            }
            if start + 7u32 < limit {
                acc = fold(acc, intermediate[start + 7u32], parameters.threshold);
            }
            summaries[b] = acc;
        }
    }
}

pub use staged::{Settings, Summary};

pub fn make_samples(count: usize) -> Vec<f32> {
    (0..count)
        .map(|index| ((index % 97) as f32) * 0.25 - 6.0)
        .collect()
}

pub fn summary_count(sample_count: usize) -> usize {
    sample_count.div_ceil(BLOCK as usize)
}

/// Independent host reference: ordinary Rust over the same inputs.
pub fn run_cpu(samples: &[f32], settings: &Settings) -> Vec<Summary> {
    samples
        .chunks(settings.block as usize)
        .map(|block| {
            let mut summary = Summary {
                total: 0.0,
                peak: 0.0,
                above_threshold: 0,
                count: 0,
            };
            for &sample in block {
                let value = sample * settings.scale + settings.offset;
                summary.total += value;
                if value > summary.peak {
                    summary.peak = value;
                }
                if value > settings.threshold {
                    summary.above_threshold += 1;
                }
                summary.count += 1;
            }
            summary
        })
        .collect()
}

/// Resident GPU state that survives across settings updates.
pub struct ResidentState {
    pub samples: gpu_dialect_wgpu::GpuBuffer<f32>,
    pub settings: gpu_dialect_wgpu::GpuBuffer<Settings>,
    pub intermediate: gpu_dialect_wgpu::GpuBuffer<f32>,
    pub summaries: gpu_dialect_wgpu::GpuBuffer<Summary>,
}

impl ResidentState {
    pub fn new(device: &HeadlessDevice, samples: &[f32]) -> Self {
        let summaries = summary_count(samples.len());
        Self {
            samples: device.create_typed_buffer(
                "graph samples",
                samples,
                GpuBufferAccess::ReadOnly,
            ),
            settings: device.create_typed_buffer(
                "graph settings",
                &[Settings {
                    scale: 0.0,
                    offset: 0.0,
                    threshold: 0.0,
                    block: BLOCK,
                }],
                GpuBufferAccess::ReadOnly,
            ),
            intermediate: device.create_typed_buffer(
                "graph intermediate",
                &vec![0.0f32; samples.len()],
                GpuBufferAccess::ReadWrite,
            ),
            summaries: device.create_typed_buffer(
                "graph summaries",
                &vec![
                    Summary {
                        total: 0.0,
                        peak: 0.0,
                        above_threshold: 0,
                        count: 0,
                    };
                    summaries
                ],
                GpuBufferAccess::ReadWrite,
            ),
        }
    }
}

/// Build and run the four-node graph for one settings update.
pub fn run_graph(
    device: &HeadlessDevice,
    state: &ResidentState,
    settings: &[Settings; 1],
) -> Result<(Vec<Summary>, GraphReport), gpu_dialect_wgpu::Error> {
    let sample_count = state.samples.len() as u32;
    let summary_count = state.summaries.len() as u32;
    let transform_bindings = [
        BufferBinding::read_only(&state.samples),
        BufferBinding::read_only(&state.settings).independent_length(),
        BufferBinding::read_write(&state.intermediate),
    ];
    let summarize_bindings = [
        BufferBinding::read_only(&state.intermediate).independent_length(),
        BufferBinding::read_only(&state.settings).independent_length(),
        BufferBinding::read_write(&state.summaries),
    ];

    let mut graph = StagedGraph::new();
    let upload = graph.upload(&state.settings, settings, &[]);
    let stage_a = graph.dispatch(
        BufferDispatch::new(
            &staged::transform::DESCRIPTOR,
            sample_count,
            &transform_bindings,
        ),
        &[upload],
    );
    let stage_b = graph.dispatch(
        BufferDispatch::new(
            &staged::summarize::DESCRIPTOR,
            summary_count,
            &summarize_bindings,
        ),
        &[stage_a],
    );
    let readback = graph.readback(&state.summaries, &[stage_b]);

    let output = device.execute_graph(&graph)?;
    Ok((output.readback::<Summary>(readback)?, output.report))
}

pub fn descriptors() -> [&'static KernelDescriptor; 2] {
    [
        &staged::transform::DESCRIPTOR,
        &staged::summarize::DESCRIPTOR,
    ]
}

pub fn assert_close(actual: &[Summary], expected: &[Summary]) {
    assert_eq!(actual.len(), expected.len());
    for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        let close = |a: f32, b: f32| (a - b).abs() <= 1.0e-4 * b.abs().max(1.0);
        assert!(
            close(actual.total, expected.total),
            "total[{index}]: {actual:?} vs {expected:?}"
        );
        assert!(
            close(actual.peak, expected.peak),
            "peak[{index}]: {actual:?} vs {expected:?}"
        );
        assert_eq!(
            actual.above_threshold, expected.above_threshold,
            "above[{index}]"
        );
        assert_eq!(actual.count, expected.count, "count[{index}]");
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
            gpu_dialect::slang::compile_wgsl(descriptor)?,
        )?;
        std::fs::write(
            directory.join(format!("{stem}.spv")),
            gpu_dialect::spirv::words_as_le_bytes(&gpu_dialect::slang::compile_spirv(descriptor)?),
        )?;
        std::fs::write(
            directory.join(format!("{stem}.rs")),
            render_wgpu_source(descriptor)?,
        )?;
    }
    Ok(directory.canonicalize().unwrap_or(directory))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let count = if cfg!(debug_assertions) {
        1 << 18
    } else {
        1 << 22
    };
    let samples = make_samples(count);
    let device = HeadlessDevice::new()?;
    let state = ResidentState::new(&device, &samples);

    let updates = [
        Settings {
            scale: 1.5,
            offset: 2.0,
            threshold: 4.0,
            block: BLOCK,
        },
        Settings {
            scale: -0.5,
            offset: 10.0,
            threshold: 11.0,
            block: BLOCK,
        },
    ];
    let mut last_report = None;
    for settings in updates {
        let cpu_started = Instant::now();
        let expected = run_cpu(&samples, &settings);
        let cpu_elapsed = cpu_started.elapsed();
        let (actual, report) = run_graph(&device, &state, &[settings])?;
        assert_close(&actual, &expected);
        println!(
            "settings {settings:?}: CPU reference {cpu_elapsed:?}, graph {:?}, upload {} B, readback {} B, resident {} B",
            report.host_elapsed, report.upload_bytes, report.readback_bytes, report.resident_bytes
        );
        last_report = Some(report);
    }
    let artifacts = export_artifacts()?;

    println!(
        "Staged graph: {count} samples -> {} summaries",
        summary_count(count)
    );
    println!("adapter: {}", device.adapter_info().name);
    println!("nodes per execution: upload -> transform -> summarize -> readback");
    println!("pipeline cache: {:?}", device.pipeline_cache_stats());
    println!("last report: {:?}", last_report.expect("two updates ran"));
    println!("generated artifacts: {}", artifacts.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpu_dialect_wgpu::{Error, TransferDirection};

    #[test]
    fn every_stage_compiles_to_wgsl_and_valid_spirv() {
        for descriptor in descriptors() {
            let wgsl = gpu_dialect::slang::compile_wgsl(descriptor).unwrap();
            assert!(wgsl.contains("@compute"));
            let spirv = gpu_dialect::slang::compile_spirv(descriptor).unwrap();
            gpu_dialect::spirv::validate_structure(&spirv).unwrap();
        }
    }

    #[test]
    fn graph_matches_cpu_and_keeps_intermediate_resident() {
        let device = HeadlessDevice::new().expect("Vulkan adapter");
        for count in [0, 1, 7, 8, 9, 63, 64, 65, 257, 1000] {
            let samples = make_samples(count);
            let state = ResidentState::new(&device, &samples);
            for settings in [
                Settings {
                    scale: 1.5,
                    offset: 2.0,
                    threshold: 4.0,
                    block: BLOCK,
                },
                Settings {
                    scale: -0.5,
                    offset: 10.0,
                    threshold: 11.0,
                    block: BLOCK,
                },
            ] {
                let expected = run_cpu(&samples, &settings);
                let (actual, report) = run_graph(&device, &state, &[settings]).unwrap();
                assert_close(&actual, &expected);
                assert_eq!(report.upload_bytes, size_of::<Settings>() as u64);
                assert_eq!(
                    report.readback_bytes,
                    (summary_count(count) * size_of::<Summary>()) as u64
                );
                // Samples and the intermediate never cross the host boundary.
                assert_eq!(report.resident_bytes, 2 * (count * size_of::<f32>()) as u64);
                assert_eq!(report.dispatches, if count == 0 { 0 } else { 2 });
                let expected_workgroups = u64::from((count as u32).div_ceil(64))
                    + u64::from((summary_count(count) as u32).div_ceil(64));
                assert_eq!(report.workgroups, expected_workgroups);
                assert_eq!(report.transfers.len(), 2);
                assert_eq!(report.transfers[0].direction, TransferDirection::Upload);
                assert_eq!(report.transfers[1].direction, TransferDirection::Readback);
            }
        }
    }

    #[test]
    fn second_update_observes_new_settings_in_order() {
        // If the upload were not ordered before stage A, the second run would
        // still see the first settings.
        let device = HeadlessDevice::new().expect("Vulkan adapter");
        let samples = make_samples(64);
        let state = ResidentState::new(&device, &samples);
        let first = Settings {
            scale: 1.0,
            offset: 0.0,
            threshold: 0.0,
            block: BLOCK,
        };
        let second = Settings {
            scale: 2.0,
            offset: 100.0,
            threshold: 50.0,
            block: BLOCK,
        };
        run_graph(&device, &state, &[first]).unwrap();
        let (actual, _) = run_graph(&device, &state, &[second]).unwrap();
        assert_close(&actual, &run_cpu(&samples, &second));
    }

    #[test]
    fn undeclared_dependencies_and_invalid_resources_are_rejected() {
        let device = HeadlessDevice::new().expect("Vulkan adapter");
        let samples = make_samples(64);
        let state = ResidentState::new(&device, &samples);
        let settings = [Settings {
            scale: 1.0,
            offset: 0.0,
            threshold: 0.0,
            block: BLOCK,
        }];
        let transform_bindings = [
            BufferBinding::read_only(&state.samples),
            BufferBinding::read_only(&state.settings).independent_length(),
            BufferBinding::read_write(&state.intermediate),
        ];
        let summarize_bindings = [
            BufferBinding::read_only(&state.intermediate).independent_length(),
            BufferBinding::read_only(&state.settings).independent_length(),
            BufferBinding::read_write(&state.summaries),
        ];
        let transform =
            BufferDispatch::new(&staged::transform::DESCRIPTOR, 64, &transform_bindings);
        let summarize = BufferDispatch::new(&staged::summarize::DESCRIPTOR, 8, &summarize_bindings);

        // Stage A reads the settings the upload writes, but declares no edge.
        let mut graph = StagedGraph::new();
        let _upload = graph.upload(&state.settings, &settings, &[]);
        graph.dispatch(transform, &[]);
        assert!(matches!(
            device.execute_graph(&graph),
            Err(Error::GraphMissingDependency {
                node: 1,
                producer: 0
            })
        ));

        // Stage B reads the intermediate stage A writes; depending only on the
        // upload does not cover that hazard.
        let mut graph = StagedGraph::new();
        let upload = graph.upload(&state.settings, &settings, &[]);
        let _stage_a = graph.dispatch(transform, &[upload]);
        graph.dispatch(summarize, &[upload]);
        assert!(matches!(
            device.execute_graph(&graph),
            Err(Error::GraphMissingDependency {
                node: 2,
                producer: 1
            })
        ));

        // Transitive coverage is accepted: readback depends on B, B on A, A on upload.
        let mut graph = StagedGraph::new();
        let upload = graph.upload(&state.settings, &settings, &[]);
        let stage_a = graph.dispatch(transform, &[upload]);
        let stage_b = graph.dispatch(summarize, &[stage_a]);
        let readback = graph.readback(&state.summaries, &[stage_b]);
        let output = device.execute_graph(&graph).unwrap();
        assert_eq!(output.readback::<Summary>(readback).unwrap().len(), 8);
        assert!(matches!(
            output.readback::<f32>(readback),
            Err(Error::GraphReadbackType { .. })
        ));
        assert!(matches!(
            output.readback::<Summary>(stage_b),
            Err(Error::GraphUnknownReadback(2))
        ));

        // A node id from another graph is not an earlier node of this one.
        let mut other = StagedGraph::new();
        other.upload(&state.settings, &settings, &[]);
        let foreign = other.dispatch(transform, &[]);
        let mut graph = StagedGraph::new();
        graph.dispatch(transform, &[foreign]);
        assert!(matches!(
            device.execute_graph(&graph),
            Err(Error::GraphDependencyOrder {
                node: 0,
                dependency: 1
            })
        ));

        // Reading back a read-only buffer is rejected before any GPU work.
        let mut graph = StagedGraph::new();
        graph.readback(&state.samples, &[]);
        assert!(matches!(
            device.execute_graph(&graph),
            Err(Error::PersistentBufferNotReadable)
        ));

        // An upload of the wrong length is rejected.
        let mut graph = StagedGraph::new();
        graph.upload(&state.intermediate, &[1.0f32, 2.0], &[]);
        assert!(matches!(
            device.execute_graph(&graph),
            Err(Error::PersistentBufferLength {
                expected: 64,
                actual: 2
            })
        ));

        // The strict shared-length rule still applies without the opt-in.
        let strict = [
            BufferBinding::read_only(&state.samples),
            BufferBinding::read_only(&state.settings),
            BufferBinding::read_write(&state.intermediate),
        ];
        assert!(matches!(
            device.dispatch_buffers(&staged::transform::DESCRIPTOR, 64, &strict),
            Err(Error::BufferLength { .. })
        ));

        // An empty independent-length buffer would expose a placeholder allocation.
        let empty = device.create_typed_buffer::<Settings>("empty", &[], GpuBufferAccess::ReadOnly);
        let placeholder = [
            BufferBinding::read_only(&state.samples),
            BufferBinding::read_only(&empty).independent_length(),
            BufferBinding::read_write(&state.intermediate),
        ];
        assert!(matches!(
            device.dispatch_buffers(&staged::transform::DESCRIPTOR, 64, &placeholder),
            Err(Error::IndependentLengthEmpty { .. })
        ));
    }
}
