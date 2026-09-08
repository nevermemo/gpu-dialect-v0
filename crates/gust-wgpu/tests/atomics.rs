include!("../../../tests/fixtures/atomics.rs");

use gust_wgpu::{BufferBinding, GpuBufferAccess, HeadlessDevice};

/// 257 threads (257 workgroups of 1) all increment the same shared `u32` counter.
/// The final value must equal exactly 257, matching an independent host reference.
/// The kernel uses `workgroup_size(1, 1, 1)` so the dispatch produces exactly
/// `INVOCATIONS` threads rather than a workgroup-rounded count.
#[test]
fn atomic_add_contention_produces_exact_sum() {
    let device =
        HeadlessDevice::new().expect("Vulkan adapter required; this test must not silently skip");
    eprintln!("atomics adapter: {:?}", device.adapter_info());
    const INVOCATIONS: u32 = 257;
    let counter =
        device.create_typed_buffer("atomics counter", &[0u32; 1], GpuBufferAccess::ReadWrite);
    device
        .dispatch_buffers(
            &atomics::run::DESCRIPTOR,
            INVOCATIONS,
            &[BufferBinding::read_write(&counter).independent_length()],
        )
        .unwrap();
    let actual = device.read_typed_buffer(&counter).unwrap();
    let host_reference = INVOCATIONS;
    assert_eq!(
        actual,
        vec![host_reference],
        "atomic_add under contention must produce exactly {INVOCATIONS}"
    );
}

#[test]
fn atomics_compile_to_wgsl() {
    let descriptor = &atomics::run::DESCRIPTOR;
    assert!(
        gust::slang::compile_wgsl(descriptor)
            .unwrap()
            .contains("@compute")
    );
}
