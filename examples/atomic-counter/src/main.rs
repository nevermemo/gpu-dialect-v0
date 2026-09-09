use std::path::PathBuf;

use gust::{gpu, slang, spirv};
use gust_wgpu::{BufferBinding, GpuBufferAccess, HeadlessDevice, render_wgpu_source};

#[gpu]
mod atomic_counter {
    /// T10 vertical slice: every thread increments the same shared counter.
    /// The final value must equal the number of threads that executed the increment.
    #[kernel(workgroup_size(1, 1, 1))]
    pub fn increment(_id: SV_DispatchThreadID, mut counter: RWStructuredBuffer<uint>) {
        atomic_add(&mut counter[0], 1u32);
    }
}

const INVOCATIONS: u32 = 257;

pub fn main() {
    let descriptor = atomic_counter::increment::DESCRIPTOR;

    let device = HeadlessDevice::new().expect("a Vulkan compute adapter should be available");
    let counter =
        device.create_typed_buffer("atomic counter", &[0u32; 1], GpuBufferAccess::ReadWrite);
    device
        .dispatch_buffers(
            &descriptor,
            INVOCATIONS,
            &[BufferBinding::read_write(&counter).independent_length()],
        )
        .expect("atomic_add should execute on the GPU");
    let gpu_result = device
        .read_typed_buffer(&counter)
        .expect("atomic counter readback should succeed");
    let host_reference = INVOCATIONS;
    let artifact_directory = export_artifacts().expect("export generated shader artifacts");

    println!("Rust -> Slang -> WGSL -> wgpu (plus SPIR-V export)");
    println!("kernel: {}", descriptor.qualified_name());
    println!("adapter: {}", device.adapter_info().name);
    println!("invocations: {INVOCATIONS}");
    println!("GPU result: {gpu_result:?}");
    println!("host reference: {host_reference}");
    println!("generated artifacts: {}", artifact_directory.display());

    assert_eq!(
        gpu_result,
        vec![host_reference],
        "atomic_add under contention must produce exactly {INVOCATIONS}"
    );
    println!("PASS: GPU result matches host reference ({host_reference})");
}

fn export_artifacts() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../generated-wgpu");
    std::fs::create_dir_all(&directory)?;
    let descriptor = &atomic_counter::increment::DESCRIPTOR;
    std::fs::write(
        directory.join("atomic_counter__increment.slang"),
        descriptor.slang_source,
    )?;
    std::fs::write(
        directory.join("atomic_counter__increment.wgsl"),
        slang::compile_wgsl(descriptor)?,
    )?;
    std::fs::write(
        directory.join("atomic_counter__increment.spv"),
        spirv::words_as_le_bytes(&slang::compile_spirv(descriptor)?),
    )?;
    std::fs::write(
        directory.join("atomic_counter__increment.rs"),
        render_wgpu_source(descriptor)?,
    )?;
    Ok(directory.canonicalize().unwrap_or(directory))
}
