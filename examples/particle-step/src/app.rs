use std::{path::PathBuf, time::Instant};

use gpu_dialect::{GpuPod, gpu};
use gpu_dialect_wgpu::{
    BufferBinding, BufferDispatch, GpuBufferAccess, HeadlessDevice, render_wgpu_source,
};

#[gpu]
mod particles {
    #[kernel(workgroup_size(64, 1, 1))]
    pub fn scalar_order(
        id: SV_DispatchThreadID,
        mut out: RWStructuredBuffer<float>,
        mut echo: RWStructuredBuffer<float>,
    ) {
        let i = id.x;
        if i < out.len() {
            out[i] = 1.0;
            out[i] = 3.0;
            echo[i] = out[i];
        }
    }
    #[derive(Debug, PartialEq)]
    pub struct Pair {
        pub x: f32,
        pub y: f32,
    }

    #[kernel(workgroup_size(64, 1, 1))]
    pub fn field_order(
        id: SV_DispatchThreadID,
        input: StructuredBuffer<Particle>,
        mut out: RWStructuredBuffer<Particle>,
        mut echo: RWStructuredBuffer<float>,
    ) {
        let i = id.x;
        if i < out.len() {
            let before = input[i];
            out[i] = before;
            out[i].velocity.y = before.velocity.y + 3.0;
            echo[i] = out[i].velocity.y;
        }
    }

    #[derive(Debug, PartialEq)]
    pub struct Particle {
        pub position: Pair,
        pub velocity: Pair,
        pub mass: f32,
        pub tag: u32,
        pub charge: i32,
    }

    #[kernel(workgroup_size(64, 1, 1))]
    pub fn step(
        id: SV_DispatchThreadID,
        input: StructuredBuffer<Particle>,
        acceleration: StructuredBuffer<Pair>,
        mut out: RWStructuredBuffer<Particle>,
        mut energy: RWStructuredBuffer<float>,
    ) {
        let i = id.x;
        if i < out.len() {
            let particle = input[i];
            let force = acceleration[i];
            let vx = particle.velocity.x + force.x * 0.016;
            let vy = particle.velocity.y + force.y * 0.016;
            // Preserve mass and integer metadata with a whole-struct copy.
            out[i] = particle;
            out[i].velocity.x = vx;
            out[i].velocity.y = vy;
            out[i].position.x = particle.position.x + vx * 0.016;
            out[i].position.y = particle.position.y + vy * 0.016;
            // Read updated fields back to exercise memory ordering within an invocation.
            let speed_squared =
                out[i].velocity.x * out[i].velocity.x + out[i].velocity.y * out[i].velocity.y;
            energy[i] = speed_squared * particle.mass * 0.5;
        }
    }

    #[kernel(workgroup_size(64, 1, 1))]
    pub fn snapshot(
        id: SV_DispatchThreadID,
        input: StructuredBuffer<Particle>,
        mut out: RWStructuredBuffer<Particle>,
    ) {
        let i = id.x;
        if i < out.len() {
            out[i] = input[i];
        }
    }
}

fn data(count: usize) -> (Vec<particles::Particle>, Vec<particles::Pair>) {
    let input = (0..count)
        .map(|i| particles::Particle {
            position: particles::Pair {
                x: (i % 257) as f32 * 0.125,
                y: -2.0,
            },
            velocity: particles::Pair {
                x: 0.5,
                y: (i % 31) as f32 * -0.0625,
            },
            mass: 1.0 + (i % 7) as f32 * 0.25,
            tag: i as u32 ^ 0xdead_beef,
            charge: (i % 5) as i32 - 2,
        })
        .collect();
    let acceleration = (0..count)
        .map(|i| particles::Pair {
            x: (i % 13) as f32 * 0.25,
            y: -9.81,
        })
        .collect();
    (input, acceleration)
}

fn cpu_step(
    input: &[particles::Particle],
    acceleration: &[particles::Pair],
) -> (Vec<particles::Particle>, Vec<f32>) {
    let mut output = input.to_vec();
    let mut energy = vec![0.0; input.len()];
    for (index, (particle, force)) in input.iter().zip(acceleration).enumerate() {
        let vx = particle.velocity.x + force.x * 0.016;
        let vy = particle.velocity.y + force.y * 0.016;
        output[index].velocity.x = vx;
        output[index].velocity.y = vy;
        output[index].position.x = particle.position.x + vx * 0.016;
        output[index].position.y = particle.position.y + vy * 0.016;
        energy[index] = (vx * vx + vy * vy) * particle.mass * 0.5;
    }
    (output, energy)
}

fn assert_matches(
    actual: &[particles::Particle],
    energy: &[f32],
    expected: &[particles::Particle],
    expected_energy: &[f32],
) {
    assert_eq!(actual.len(), expected.len());
    assert_eq!(energy.len(), expected_energy.len());
    for ((actual, energy), (expected, expected_energy)) in actual
        .iter()
        .zip(energy)
        .zip(expected.iter().zip(expected_energy))
    {
        for (actual, expected) in [
            (actual.position.x, expected.position.x),
            (actual.position.y, expected.position.y),
            (actual.velocity.x, expected.velocity.x),
            (actual.velocity.y, expected.velocity.y),
            (actual.mass, expected.mass),
            (*energy, *expected_energy),
        ] {
            assert!(
                (actual - expected).abs() <= 1e-5 * expected.abs().max(1.0),
                "GPU value {actual} differs from CPU value {expected}"
            );
        }
        assert_eq!(actual.tag, expected.tag);
        assert_eq!(actual.charge, expected.charge);
    }
}

pub fn main() -> Result<(), Box<dyn std::error::Error>> {
    let count = 1_048_576;
    let (input, acceleration) = data(count);
    let device = HeadlessDevice::new()?;
    let cpu_started = Instant::now();
    let (expected, expected_energy) = cpu_step(&input, &acceleration);
    let cpu_elapsed = cpu_started.elapsed();
    let gpu_input = device.create_typed_buffer("particles", &input, GpuBufferAccess::ReadOnly);
    let gpu_acceleration =
        device.create_typed_buffer("acceleration", &acceleration, GpuBufferAccess::ReadOnly);
    let gpu_output =
        device.create_typed_buffer("updated particles", &input, GpuBufferAccess::ReadWrite);
    let gpu_snapshot = device.create_typed_buffer("snapshot", &input, GpuBufferAccess::ReadWrite);
    let gpu_energy =
        device.create_f32_buffer("energy", &vec![0.0; count], GpuBufferAccess::ReadWrite);
    let step_bindings = [
        BufferBinding::read_only(&gpu_input),
        BufferBinding::read_only(&gpu_acceleration),
        BufferBinding::read_write(&gpu_output),
        BufferBinding::read_write(&gpu_energy),
    ];
    let snapshot_bindings = [
        BufferBinding::read_only(&gpu_output),
        BufferBinding::read_write(&gpu_snapshot),
    ];
    let batch = [
        BufferDispatch::new(&particles::step::DESCRIPTOR, count as u32, &step_bindings),
        BufferDispatch::new(
            &particles::snapshot::DESCRIPTOR,
            count as u32,
            &snapshot_bindings,
        ),
    ];
    device.dispatch_batch(&batch)?; // Warm the compiled-kernel cache and complete initial uploads.
    let timing = device.dispatch_batch_timed(&batch)?;
    let actual = device.read_typed_buffer(&gpu_snapshot)?;
    let energy = device.read_f32_buffer(&gpu_energy)?;
    assert_matches(&actual, &energy, &expected, &expected_energy);
    let job = gpu_dialect::Device::submit(&device, &batch)?;
    job.wait()?;
    println!(
        "Particle step: {count} particles on {}",
        device.adapter_info().name
    );
    println!(
        "Particle storage ABI: {} bytes, alignment {}, stride {}",
        <particles::Particle as GpuPod>::LAYOUT.size,
        <particles::Particle as GpuPod>::LAYOUT.alignment,
        <particles::Particle as GpuPod>::LAYOUT.stride
    );
    println!("CPU step including output allocation: {cpu_elapsed:?}");
    println!(
        "GPU resident step + snapshot, host time: {:?}",
        timing.host_elapsed
    );
    println!(
        "GPU resident step + snapshot, hardware time: {:?}",
        timing.gpu_elapsed
    );
    println!(
        "Nested fields, integer metadata, energy output, and dependent snapshot match the CPU reference."
    );
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../generated-wgpu");
    std::fs::create_dir_all(&directory)?;
    for descriptor in [
        &particles::step::DESCRIPTOR,
        &particles::snapshot::DESCRIPTOR,
    ] {
        let stem = format!("{}__{}", descriptor.module, descriptor.name);
        let path = directory.join(format!("{stem}.rs"));
        std::fs::write(&path, render_wgpu_source(descriptor)?)?;
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
        println!(
            "generated shader artifacts: {}",
            path.canonicalize()?.display()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpu_dialect::TypeLayoutKind;

    fn device() -> Option<HeadlessDevice> {
        match HeadlessDevice::new() {
            Ok(device) => Some(device),
            Err(gpu_dialect_wgpu::Error::NoAdapter(_)) => None,
            Err(error) => panic!("could not initialize headless wgpu: {error}"),
        }
    }

    #[test]
    #[ignore = "example validation runs only in full verification"]
    fn nested_layout_matches_executable_artifacts() {
        let layout = <particles::Particle as GpuPod>::LAYOUT;
        assert_eq!((layout.size, layout.alignment, layout.stride), (28, 4, 28));
        let TypeLayoutKind::Struct(fields) = layout.kind else {
            panic!("expected struct")
        };
        assert_eq!(
            fields.iter().map(|field| field.offset).collect::<Vec<_>>(),
            [0, 8, 16, 20, 24]
        );
        for descriptor in [
            &particles::step::DESCRIPTOR,
            &particles::snapshot::DESCRIPTOR,
        ] {
            let source = render_wgpu_source(descriptor).unwrap();
            assert!(source.contains("Particle [storage-v1; size=28, align=4, stride=28]"));
        }
    }

    #[test]
    #[ignore = "example validation runs only in full verification"]
    fn scalar_read_after_write() {
        let Some(device) = device() else { return };
        let result = device
            .dispatch_f32(
                &particles::scalar_order::DESCRIPTOR,
                1,
                &[
                    gpu_dialect_wgpu::F32Binding::ReadWrite(&[0.0]),
                    gpu_dialect_wgpu::F32Binding::ReadWrite(&[0.0]),
                ],
            )
            .unwrap();
        assert_eq!(result.writable_buffers, vec![vec![3.0], vec![3.0]]);
    }

    #[test]
    #[ignore = "example validation runs only in full verification"]
    fn field_read_after_write() {
        let Some(device) = device() else { return };
        let out = device.create_typed_buffer("particle", &data(1).0, GpuBufferAccess::ReadWrite);
        let input = device.create_typed_buffer("input", &data(1).0, GpuBufferAccess::ReadOnly);
        let echo = device.create_f32_buffer("echo", &[0.0], GpuBufferAccess::ReadWrite);
        device
            .dispatch_buffers(
                &particles::field_order::DESCRIPTOR,
                1,
                &[
                    BufferBinding::read_only(&input),
                    BufferBinding::read_write(&out),
                    BufferBinding::read_write(&echo),
                ],
            )
            .unwrap();
        assert_eq!(device.read_typed_buffer(&out).unwrap()[0].velocity.y, 3.0);
        assert_eq!(device.read_f32_buffer(&echo).unwrap(), vec![3.0]);
    }

    #[test]
    #[ignore = "example validation runs only in full verification"]
    fn cpu_step_has_expected_physics_and_preserves_metadata() {
        let (input, acceleration) = data(1);
        let (output, energy) = cpu_step(&input, &acceleration);
        assert_eq!(output[0].position.x, 0.008);
        assert!((output[0].velocity.y + 0.15696).abs() < 1e-6);
        assert!((energy[0] - 0.13731822).abs() < 1e-6);
        assert_eq!(output[0].tag, input[0].tag);
        assert_eq!(output[0].charge, -2);
    }

    #[test]
    #[ignore = "example validation runs only in full verification"]
    fn gpu_nested_structs_and_dependent_batches_match_cpu() {
        let Some(device) = device() else { return };
        for count in [0, 1, 63, 64, 65, 129] {
            let (input, acceleration) = data(count);
            let (expected, expected_energy) = cpu_step(&input, &acceleration);
            let source = device.create_typed_buffer("input", &input, GpuBufferAccess::ReadOnly);
            let force =
                device.create_typed_buffer("force", &acceleration, GpuBufferAccess::ReadOnly);
            let out = device.create_typed_buffer("out", &input, GpuBufferAccess::ReadWrite);
            let snapshot =
                device.create_typed_buffer("snapshot", &input, GpuBufferAccess::ReadWrite);
            let energy =
                device.create_f32_buffer("energy", &vec![0.0; count], GpuBufferAccess::ReadWrite);
            let step_bindings = [
                BufferBinding::read_only(&source),
                BufferBinding::read_only(&force),
                BufferBinding::read_write(&out),
                BufferBinding::read_write(&energy),
            ];
            let snapshot_bindings = [
                BufferBinding::read_only(&out),
                BufferBinding::read_write(&snapshot),
            ];
            let batch = [
                BufferDispatch::new(&particles::step::DESCRIPTOR, count as u32, &step_bindings),
                BufferDispatch::new(
                    &particles::snapshot::DESCRIPTOR,
                    count as u32,
                    &snapshot_bindings,
                ),
            ];
            gpu_dialect::Device::submit(&device, &batch)
                .unwrap()
                .wait()
                .unwrap();
            assert_matches(
                &device.read_typed_buffer(&snapshot).unwrap(),
                &device.read_f32_buffer(&energy).unwrap(),
                &expected,
                &expected_energy,
            );
            let timing = device.dispatch_batch_timed(&batch).unwrap();
            assert_eq!(
                timing.gpu_elapsed.is_some(),
                count > 0 && device.supports_gpu_timestamps()
            );
            assert_matches(
                &device.read_typed_buffer(&snapshot).unwrap(),
                &device.read_f32_buffer(&energy).unwrap(),
                &expected,
                &expected_energy,
            );
        }
        assert_eq!(device.pipeline_cache_stats().entries, 2);
    }
}
