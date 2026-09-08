use gust::gpu;
use gust_wgpu::{HeadlessDevice, U32Binding};

#[gpu]
mod atomic_counter {
    /// T10 vertical slice: every thread increments the same shared counter.
    /// The final value must equal the number of threads that executed the increment.
    #[kernel]
    pub fn increment(_id: SV_DispatchThreadID, mut counter: RWStructuredBuffer<uint>) {
        atomic_add(&mut counter[0], 1u32);
    }
}

const INVOCATIONS: u32 = 257;

pub fn main() {
    let seed = [0u32; 1];
    let descriptor = atomic_counter::increment::DESCRIPTOR;

    let device = HeadlessDevice::new().expect("a Vulkan compute adapter should be available");
    let result = device
        .dispatch_u32(&descriptor, INVOCATIONS, &[U32Binding::ReadWrite(&seed)])
        .expect("atomic_add should execute on the GPU");

    let gpu_result = result.writable_buffers[0].clone();
    let host_reference = INVOCATIONS;

    println!("Rust -> Slang -> WGSL -> wgpu (atomic_add contention test)");
    println!("kernel: {}", descriptor.qualified_name());
    println!("adapter: {}", device.adapter_info().name);
    println!("invocations: {INVOCATIONS}");
    println!("GPU result: {gpu_result:?}");
    println!("host reference: {host_reference}");

    assert_eq!(
        gpu_result,
        vec![host_reference],
        "atomic_add under contention must produce exactly {INVOCATIONS}"
    );
    println!("PASS: GPU result matches host reference ({host_reference})");
}
