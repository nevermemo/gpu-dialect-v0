use std::{path::PathBuf, time::Instant};

use gust::{JobStatus, gpu};
use gust_wgpu::{
    F32Binding, F32BufferBinding, F32BufferDispatch, GpuBufferAccess, HeadlessDevice,
    TransferTiming, render_wgpu_source,
};

const PERSISTENT_ITERATIONS: u32 = 20;

#[gpu]
mod polynomial {
    #[kernel(workgroup_size(128, 1, 1))]
    pub fn evaluate(
        id: SV_DispatchThreadID,
        input: StructuredBuffer<float>,
        mut out: RWStructuredBuffer<float>,
    ) {
        let i = id.x;
        if i < out.len() {
            let value = input[i];
            let squared = value * value;
            let cubed = squared * value;
            let curved = cubed - (2.0 * squared);
            let linear = 0.5 * value;
            out[i] = (curved + linear) + 1.0;
        }
    }
}

pub fn main() {
    let element_count = benchmark_element_count();
    let input = (0..element_count)
        .map(|index| (index % 4096) as f32 / 2048.0 - 1.0)
        .collect::<Vec<_>>();
    let mut cpu_output = vec![0.0; element_count];

    let cpu_started = Instant::now();
    run_cpu(&input, &mut cpu_output);
    let cpu_elapsed = cpu_started.elapsed();

    let device = HeadlessDevice::new().expect("a Vulkan compute adapter should be available");
    let initial_output = vec![0.0; element_count];
    let _warmup = device
        .dispatch_f32(
            &polynomial::evaluate::DESCRIPTOR,
            element_count as u32,
            &[
                F32Binding::ReadOnly(&input),
                F32Binding::ReadWrite(&initial_output),
            ],
        )
        .expect("polynomial GPU warmup");
    let gpu_started = Instant::now();
    let gpu_result = device
        .dispatch_f32(
            &polynomial::evaluate::DESCRIPTOR,
            element_count as u32,
            &[
                F32Binding::ReadOnly(&input),
                F32Binding::ReadWrite(&initial_output),
            ],
        )
        .expect("polynomial GPU dispatch");
    let gpu_elapsed = gpu_started.elapsed();
    let gpu_output = &gpu_result.writable_buffers[0];
    let error = max_abs_error(&cpu_output, gpu_output);
    assert!(error <= 1.0e-5, "CPU/GPU error was {error}");

    let persistent_input =
        device.create_f32_buffer("polynomial input", &input, GpuBufferAccess::ReadOnly);
    let persistent_output = device.create_f32_buffer(
        "polynomial output",
        &initial_output,
        GpuBufferAccess::ReadWrite,
    );
    let upload_timing = device
        .write_f32_buffer_timed(&persistent_input, &input)
        .expect("timed polynomial upload");
    let persistent_bindings = [
        F32BufferBinding::ReadOnly(&persistent_input),
        F32BufferBinding::ReadWrite(&persistent_output),
    ];
    device
        .dispatch_f32_buffers(
            &polynomial::evaluate::DESCRIPTOR,
            element_count as u32,
            &persistent_bindings,
        )
        .expect("persistent polynomial warmup");
    let persistent_started = Instant::now();
    for _ in 0..PERSISTENT_ITERATIONS {
        device
            .dispatch_f32_buffers(
                &polynomial::evaluate::DESCRIPTOR,
                element_count as u32,
                &persistent_bindings,
            )
            .expect("persistent polynomial dispatch");
    }
    let persistent_elapsed = persistent_started.elapsed() / PERSISTENT_ITERATIONS;
    let batch = vec![
        F32BufferDispatch::new(
            &polynomial::evaluate::DESCRIPTOR,
            element_count as u32,
            &persistent_bindings,
        );
        PERSISTENT_ITERATIONS as usize
    ];
    let batch_started = Instant::now();
    device
        .dispatch_f32_batch(&batch)
        .expect("batched polynomial dispatch");
    let batch_elapsed = batch_started.elapsed();
    let profiled_batch = device
        .dispatch_f32_batch_timed(&batch)
        .expect("timestamped polynomial batch");
    let mut overlap_output = vec![0.0; element_count];
    let async_submit_started = Instant::now();
    let async_job = device
        .submit_f32_batch(&batch)
        .expect("asynchronous polynomial batch");
    let async_submit_elapsed = async_submit_started.elapsed();
    let status_after_submit = async_job.status().expect("polynomial job status");
    let cpu_overlap_started = Instant::now();
    run_cpu(&input, &mut overlap_output);
    let cpu_overlap_elapsed = cpu_overlap_started.elapsed();
    let status_after_cpu = async_job
        .status()
        .expect("polynomial job status after CPU work");
    let final_wait_started = Instant::now();
    async_job
        .wait()
        .expect("asynchronous polynomial completion");
    let final_wait_elapsed = final_wait_started.elapsed();
    assert!(max_abs_error(&cpu_output, &overlap_output) <= 1.0e-5);
    let async_timing = AsyncBatchTiming {
        submit: async_submit_elapsed,
        status_after_submit,
        cpu_overlap: cpu_overlap_elapsed,
        status_after_cpu,
        final_wait: final_wait_elapsed,
    };
    let (persistent_result, readback_timing) = device
        .read_f32_buffer_timed(&persistent_output)
        .expect("timed persistent polynomial readback");
    let persistent_error = max_abs_error(&cpu_output, &persistent_result);
    assert!(
        persistent_error <= 1.0e-5,
        "persistent CPU/GPU error was {persistent_error}"
    );

    let generated_source = export_generated_source();
    println!("Polynomial benchmark");
    println!(
        "kernel: {}",
        polynomial::evaluate::DESCRIPTOR.qualified_name()
    );
    println!("adapter: {}", device.adapter_info().name);
    println!("elements: {element_count}");
    println!("operations per element: 4 mul, 1 sub, 2 add");
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

fn run_cpu(input: &[f32], output: &mut [f32]) {
    for (output, value) in output.iter_mut().zip(input.iter().copied()) {
        let squared = value * value;
        let cubed = squared * value;
        let curved = cubed - 2.0 * squared;
        let linear = 0.5 * value;
        *output = curved + linear + 1.0;
    }
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
    let descriptor = &polynomial::evaluate::DESCRIPTOR;
    let path = directory.join("polynomial__evaluate.rs");
    let source = render_wgpu_source(descriptor).expect("render polynomial wgpu source");
    std::fs::write(&path, source).expect("write polynomial wgpu source");
    std::fs::write(
        directory.join("polynomial__evaluate.slang"),
        descriptor.slang_source,
    )
    .expect("write polynomial Slang source");
    std::fs::write(
        directory.join("polynomial__evaluate.wgsl"),
        gust::slang::compile_wgsl(descriptor).expect("compile polynomial WGSL"),
    )
    .expect("write polynomial WGSL");
    std::fs::write(
        directory.join("polynomial__evaluate.spv"),
        gust::spirv::words_as_le_bytes(
            &gust::slang::compile_spirv(descriptor).expect("compile polynomial SPIR-V"),
        ),
    )
    .expect("write polynomial SPIR-V");
    path.canonicalize().unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "example validation runs only in full verification"]
    fn cpu_reference_evaluates_all_steps() {
        let input = [-1.0, 0.0, 0.5, 1.0];
        let mut output = [0.0; 4];
        run_cpu(&input, &mut output);
        let expected = input.map(|value| {
            let squared = value * value;
            value * squared - 2.0 * squared + 0.5 * value + 1.0
        });
        assert_eq!(output, expected);
    }

    #[test]
    #[ignore = "example validation runs only in full verification"]
    fn headless_gpu_matches_cpu_reference() {
        let device = match HeadlessDevice::new() {
            Ok(device) => device,
            Err(gust_wgpu::Error::NoAdapter(_)) => return,
            Err(error) => panic!("could not initialize headless wgpu: {error}"),
        };
        let input = [-1.0, -0.25, 0.0, 0.5, 1.0, 1.5];
        let mut cpu_output = [0.0; 6];
        let initial_output = [0.0; 6];
        run_cpu(&input, &mut cpu_output);
        let gpu_output = device
            .dispatch_f32(
                &polynomial::evaluate::DESCRIPTOR,
                input.len() as u32,
                &[
                    F32Binding::ReadOnly(&input),
                    F32Binding::ReadWrite(&initial_output),
                ],
            )
            .expect("GPU polynomial dispatch");
        assert!(max_abs_error(&cpu_output, &gpu_output.writable_buffers[0]) <= 1.0e-6);
    }

    #[test]
    #[ignore = "example validation runs only in full verification"]
    fn descriptor_and_generated_host_code_are_executable() {
        let descriptor = polynomial::evaluate::DESCRIPTOR;
        assert!(
            descriptor
                .slang_source
                .contains("void gpu_polynomial_evaluate")
        );
        let source = render_wgpu_source(&descriptor).expect("render source");
        assert!(source.contains("pub struct PolynomialEvaluatePipeline"));
        assert!(source.contains("pub fn encode("));
        assert!(source.contains("input.as_entire_binding()"));
        assert!(source.contains("out.as_entire_binding()"));
    }
}
