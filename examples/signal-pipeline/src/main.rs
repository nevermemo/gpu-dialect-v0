use std::{path::PathBuf, time::Instant};

use gpu_dialect::{JobStatus, gpu};
use gpu_dialect_wgpu::{
    F32Binding, F32BufferBinding, F32BufferDispatch, GpuBufferAccess, HeadlessDevice,
    TransferTiming, render_wgpu_source,
};

const PERSISTENT_ITERATIONS: u32 = 20;

#[gpu]
mod signal_pipeline {
    #[kernel(workgroup_size(128, 1, 1))]
    pub fn transform(
        id: SV_DispatchThreadID,
        signal: StructuredBuffer<float>,
        baseline: StructuredBuffer<float>,
        gain: StructuredBuffer<float>,
        bias: StructuredBuffer<float>,
        scale: StructuredBuffer<float>,
        mut out: RWStructuredBuffer<float>,
    ) {
        let i = id.x;
        if i < out.len() {
            let centered = signal[i] - baseline[i];
            let amplified = centered * gain[i];
            let biased = amplified + bias[i];
            let normalized = biased / scale[i];
            let energy = normalized * normalized;
            out[i] = energy + (0.25 * normalized);
        }
    }
}

fn main() {
    let element_count = benchmark_element_count();
    let inputs = Inputs::new(element_count);
    let mut cpu_output = vec![0.0; element_count];

    let cpu_started = Instant::now();
    run_cpu(&inputs, &mut cpu_output);
    let cpu_elapsed = cpu_started.elapsed();

    let device = HeadlessDevice::new().expect("a Vulkan compute adapter should be available");
    let initial_output = vec![0.0; element_count];
    let _warmup = dispatch_gpu(&device, &inputs, &initial_output).expect("signal GPU warmup");
    let gpu_started = Instant::now();
    let gpu_result = dispatch_gpu(&device, &inputs, &initial_output).expect("signal GPU dispatch");
    let gpu_elapsed = gpu_started.elapsed();
    let gpu_output = &gpu_result.writable_buffers[0];
    let error = max_abs_error(&cpu_output, gpu_output);
    assert!(error <= 2.0e-5, "CPU/GPU error was {error}");

    let persistent_signal =
        device.create_f32_buffer("signal", &inputs.signal, GpuBufferAccess::ReadOnly);
    let persistent_baseline =
        device.create_f32_buffer("baseline", &inputs.baseline, GpuBufferAccess::ReadOnly);
    let persistent_gain = device.create_f32_buffer("gain", &inputs.gain, GpuBufferAccess::ReadOnly);
    let persistent_bias = device.create_f32_buffer("bias", &inputs.bias, GpuBufferAccess::ReadOnly);
    let persistent_scale =
        device.create_f32_buffer("scale", &inputs.scale, GpuBufferAccess::ReadOnly);
    let persistent_output =
        device.create_f32_buffer("signal output", &initial_output, GpuBufferAccess::ReadWrite);
    let upload_timing = [
        (&persistent_signal, inputs.signal.as_slice()),
        (&persistent_baseline, inputs.baseline.as_slice()),
        (&persistent_gain, inputs.gain.as_slice()),
        (&persistent_bias, inputs.bias.as_slice()),
        (&persistent_scale, inputs.scale.as_slice()),
    ]
    .into_iter()
    .try_fold(TransferTiming::default(), |total, (buffer, values)| {
        let timing = device.write_f32_buffer_timed(buffer, values)?;
        Ok::<_, gpu_dialect_wgpu::Error>(TransferTiming {
            bytes: total.bytes + timing.bytes,
            elapsed: total.elapsed + timing.elapsed,
        })
    })
    .expect("timed signal input uploads");
    let persistent_bindings = [
        F32BufferBinding::ReadOnly(&persistent_signal),
        F32BufferBinding::ReadOnly(&persistent_baseline),
        F32BufferBinding::ReadOnly(&persistent_gain),
        F32BufferBinding::ReadOnly(&persistent_bias),
        F32BufferBinding::ReadOnly(&persistent_scale),
        F32BufferBinding::ReadWrite(&persistent_output),
    ];
    device
        .dispatch_f32_buffers(
            &signal_pipeline::transform::DESCRIPTOR,
            element_count as u32,
            &persistent_bindings,
        )
        .expect("persistent signal warmup");
    let persistent_started = Instant::now();
    for _ in 0..PERSISTENT_ITERATIONS {
        device
            .dispatch_f32_buffers(
                &signal_pipeline::transform::DESCRIPTOR,
                element_count as u32,
                &persistent_bindings,
            )
            .expect("persistent signal dispatch");
    }
    let persistent_elapsed = persistent_started.elapsed() / PERSISTENT_ITERATIONS;
    let batch = vec![
        F32BufferDispatch::new(
            &signal_pipeline::transform::DESCRIPTOR,
            element_count as u32,
            &persistent_bindings,
        );
        PERSISTENT_ITERATIONS as usize
    ];
    let batch_started = Instant::now();
    device
        .dispatch_f32_batch(&batch)
        .expect("batched signal dispatch");
    let batch_elapsed = batch_started.elapsed();
    let profiled_batch = device
        .dispatch_f32_batch_timed(&batch)
        .expect("timestamped signal batch");
    let mut overlap_output = vec![0.0; element_count];
    let async_submit_started = Instant::now();
    let async_job = device
        .submit_f32_batch(&batch)
        .expect("asynchronous signal batch");
    let async_submit_elapsed = async_submit_started.elapsed();
    let status_after_submit = async_job.status().expect("signal job status");
    let cpu_overlap_started = Instant::now();
    run_cpu(&inputs, &mut overlap_output);
    let cpu_overlap_elapsed = cpu_overlap_started.elapsed();
    let status_after_cpu = async_job
        .status()
        .expect("signal job status after CPU work");
    let final_wait_started = Instant::now();
    async_job.wait().expect("asynchronous signal completion");
    let final_wait_elapsed = final_wait_started.elapsed();
    assert!(max_abs_error(&cpu_output, &overlap_output) <= 2.0e-5);
    let async_timing = AsyncBatchTiming {
        submit: async_submit_elapsed,
        status_after_submit,
        cpu_overlap: cpu_overlap_elapsed,
        status_after_cpu,
        final_wait: final_wait_elapsed,
    };
    let (persistent_result, readback_timing) = device
        .read_f32_buffer_timed(&persistent_output)
        .expect("timed persistent signal readback");
    let persistent_error = max_abs_error(&cpu_output, &persistent_result);
    assert!(
        persistent_error <= 2.0e-5,
        "persistent CPU/GPU error was {persistent_error}"
    );

    let generated_source = export_generated_source();
    println!("Signal pipeline benchmark");
    println!(
        "kernel: {}",
        signal_pipeline::transform::DESCRIPTOR.qualified_name()
    );
    println!("adapter: {}", device.adapter_info().name);
    println!("elements: {element_count}");
    println!("operations per element: 3 mul, 1 sub, 2 add, 1 div");
    println!(
        "hardware GPU timestamps: {}",
        device.supports_gpu_timestamps()
    );
    println!("pipeline cache: {:?}", device.pipeline_cache_stats());
    print_comparison(
        cpu_elapsed,
        gpu_elapsed,
        persistent_elapsed,
        batch_elapsed,
        profiled_batch.gpu_elapsed,
        upload_timing,
        readback_timing,
    );
    print_async_overlap(async_timing);
    println!("maximum absolute error: {error:e}");
    println!("persistent maximum absolute error: {persistent_error:e}");
    println!("generated wgpu source: {}", generated_source.display());
}

struct Inputs {
    signal: Vec<f32>,
    baseline: Vec<f32>,
    gain: Vec<f32>,
    bias: Vec<f32>,
    scale: Vec<f32>,
}

impl Inputs {
    fn new(element_count: usize) -> Self {
        let indices = 0..element_count;
        Self {
            signal: indices
                .clone()
                .map(|index| (index % 2048) as f32 / 256.0 - 4.0)
                .collect(),
            baseline: indices
                .clone()
                .map(|index| (index % 31) as f32 * 0.01 - 0.15)
                .collect(),
            gain: indices
                .clone()
                .map(|index| 0.75 + (index % 17) as f32 * 0.025)
                .collect(),
            bias: indices
                .clone()
                .map(|index| (index % 13) as f32 * 0.02 - 0.12)
                .collect(),
            scale: indices
                .map(|index| 1.0 + (index % 23) as f32 * 0.03)
                .collect(),
        }
    }
}

fn run_cpu(inputs: &Inputs, output: &mut [f32]) {
    for (index, output) in output.iter_mut().enumerate() {
        let centered = inputs.signal[index] - inputs.baseline[index];
        let amplified = centered * inputs.gain[index];
        let biased = amplified + inputs.bias[index];
        let normalized = biased / inputs.scale[index];
        let energy = normalized * normalized;
        *output = energy + 0.25 * normalized;
    }
}

fn dispatch_gpu(
    device: &HeadlessDevice,
    inputs: &Inputs,
    initial_output: &[f32],
) -> Result<gpu_dialect_wgpu::DispatchOutput, gpu_dialect_wgpu::Error> {
    device.dispatch_f32(
        &signal_pipeline::transform::DESCRIPTOR,
        inputs.signal.len() as u32,
        &[
            F32Binding::ReadOnly(&inputs.signal),
            F32Binding::ReadOnly(&inputs.baseline),
            F32Binding::ReadOnly(&inputs.gain),
            F32Binding::ReadOnly(&inputs.bias),
            F32Binding::ReadOnly(&inputs.scale),
            F32Binding::ReadWrite(initial_output),
        ],
    )
}

fn benchmark_element_count() -> usize {
    if cfg!(debug_assertions) {
        1 << 18
    } else {
        1 << 22
    }
}

fn max_abs_error(left: &[f32], right: &[f32]) -> f32 {
    left.iter()
        .zip(right)
        .map(|(left, right)| (left - right).abs())
        .fold(0.0, f32::max)
}

fn print_comparison(
    cpu: std::time::Duration,
    gpu: std::time::Duration,
    persistent: std::time::Duration,
    batch: std::time::Duration,
    gpu_batch: Option<std::time::Duration>,
    upload: TransferTiming,
    readback: TransferTiming,
) {
    let end_to_end_ratio = cpu.as_secs_f64() / gpu.as_secs_f64();
    let persistent_ratio = cpu.as_secs_f64() / persistent.as_secs_f64();
    let batch_per_dispatch = batch / PERSISTENT_ITERATIONS;
    let batching_speedup = persistent.as_secs_f64() / batch_per_dispatch.as_secs_f64();
    println!("CPU reference: {cpu:?}");
    println!("GPU end-to-end: {gpu:?}");
    println!("GPU persistent dispatch: {persistent:?}");
    println!(
        "GPU batch ({} dispatches): {batch:?} total, {batch_per_dispatch:?} per dispatch",
        PERSISTENT_ITERATIONS
    );
    println!(
        "GPU input upload ({} bytes): {:?}",
        upload.bytes, upload.elapsed
    );
    match gpu_batch {
        Some(elapsed) => println!(
            "GPU batch timestamp: {elapsed:?} total, {:?} per dispatch",
            elapsed / PERSISTENT_ITERATIONS
        ),
        None => println!("GPU batch timestamp: unavailable"),
    }
    println!(
        "GPU output readback ({} bytes): {:?}",
        readback.bytes, readback.elapsed
    );
    if end_to_end_ratio >= 1.0 {
        println!("GPU end-to-end is {end_to_end_ratio:.2}x faster than CPU");
    } else {
        println!(
            "CPU is {:.2}x faster than GPU end-to-end",
            end_to_end_ratio.recip()
        );
    }
    if persistent_ratio >= 1.0 {
        println!("GPU persistent dispatch is {persistent_ratio:.2}x faster than CPU");
    } else {
        println!(
            "CPU is {:.2}x faster than GPU persistent dispatch",
            persistent_ratio.recip()
        );
    }
    println!("batching improves host throughput by {batching_speedup:.2}x");
}

#[derive(Clone, Copy)]
struct AsyncBatchTiming {
    submit: std::time::Duration,
    status_after_submit: JobStatus,
    cpu_overlap: std::time::Duration,
    status_after_cpu: JobStatus,
    final_wait: std::time::Duration,
}

fn print_async_overlap(timing: AsyncBatchTiming) {
    println!("GPU asynchronous batch submission: {:?}", timing.submit);
    println!(
        "job status after submission: {:?}",
        timing.status_after_submit
    );
    println!("overlapped CPU reference: {:?}", timing.cpu_overlap);
    println!("job status after CPU work: {:?}", timing.status_after_cpu);
    println!("final job wait: {:?}", timing.final_wait);
}

fn export_generated_source() -> PathBuf {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../generated-wgpu");
    std::fs::create_dir_all(&directory).expect("create generated-wgpu directory");
    let descriptor = &signal_pipeline::transform::DESCRIPTOR;
    let path = directory.join("signal_pipeline__transform.rs");
    let source = render_wgpu_source(descriptor).expect("render signal pipeline wgpu source");
    std::fs::write(&path, source).expect("write signal pipeline wgpu source");
    std::fs::write(
        directory.join("signal_pipeline__transform.slang"),
        descriptor.slang_source,
    )
    .expect("write signal pipeline Slang source");
    std::fs::write(
        directory.join("signal_pipeline__transform.wgsl"),
        gpu_dialect::slang::compile_wgsl(descriptor).expect("compile signal pipeline WGSL"),
    )
    .expect("write signal pipeline WGSL");
    std::fs::write(
        directory.join("signal_pipeline__transform.spv"),
        gpu_dialect::spirv::words_as_le_bytes(
            &gpu_dialect::slang::compile_spirv(descriptor).expect("compile signal pipeline SPIR-V"),
        ),
    )
    .expect("write signal pipeline SPIR-V");
    path.canonicalize().unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "example validation runs only in full verification"]
    fn cpu_reference_runs_the_full_pipeline() {
        let inputs = Inputs::new(8);
        let mut output = [0.0; 8];
        run_cpu(&inputs, &mut output);
        for (index, actual) in output.into_iter().enumerate() {
            let centered = inputs.signal[index] - inputs.baseline[index];
            let normalized =
                (centered * inputs.gain[index] + inputs.bias[index]) / inputs.scale[index];
            let expected = normalized * normalized + 0.25 * normalized;
            assert_eq!(actual, expected);
        }
    }

    #[test]
    #[ignore = "example validation runs only in full verification"]
    fn headless_gpu_matches_cpu_reference() {
        let device = match HeadlessDevice::new() {
            Ok(device) => device,
            Err(gpu_dialect_wgpu::Error::NoAdapter(_)) => return,
            Err(error) => panic!("could not initialize headless wgpu: {error}"),
        };
        let inputs = Inputs::new(257);
        let mut cpu_output = vec![0.0; 257];
        let initial_output = vec![0.0; 257];
        run_cpu(&inputs, &mut cpu_output);
        let gpu_output = dispatch_gpu(&device, &inputs, &initial_output)
            .expect("GPU signal dispatch")
            .writable_buffers
            .remove(0);
        assert!(max_abs_error(&cpu_output, &gpu_output) <= 2.0e-5);
    }

    #[test]
    #[ignore = "example validation runs only in full verification"]
    fn descriptor_and_generated_host_code_are_executable() {
        let descriptor = signal_pipeline::transform::DESCRIPTOR;
        assert!(
            descriptor
                .slang_source
                .contains("void gpu_signal_pipeline_transform")
        );
        let source = render_wgpu_source(&descriptor).expect("render source");
        assert!(source.contains("pub struct SignalPipelineTransformPipeline"));
        assert!(source.contains("pub fn encode("));
        assert!(source.contains("binding: 5,"));
        assert!(source.contains("scale.as_entire_binding()"));
    }
}
