include!("../../../tests/fixtures/atomics.rs");

use gust_wgpu::{BufferBinding, GpuBufferAccess, HeadlessDevice};

const INVOCATIONS: u32 = 257;

/// 257 threads all increment the same shared `u32` counter.
/// The final value must equal exactly 257, matching an independent host reference.
#[test]
fn atomic_add_contention_produces_exact_sum() {
    let device =
        HeadlessDevice::new().expect("Vulkan adapter required; this test must not silently skip");
    eprintln!("atomics adapter: {:?}", device.adapter_info());
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
    assert_eq!(
        actual,
        vec![INVOCATIONS],
        "atomic_add under contention must produce exactly {INVOCATIONS}"
    );
}

/// 257 threads each attempt `atomic_min(counter, id.x)`.
/// Initial value is 1000; the minimum of {0..256} is 0.
#[test]
fn atomic_min_produces_minimum() {
    let device =
        HeadlessDevice::new().expect("Vulkan adapter required; this test must not silently skip");
    let counter =
        device.create_typed_buffer("atomics min", &[1000u32; 1], GpuBufferAccess::ReadWrite);
    device
        .dispatch_buffers(
            &atomics::run_min::DESCRIPTOR,
            INVOCATIONS,
            &[BufferBinding::read_write(&counter).independent_length()],
        )
        .unwrap();
    let actual = device.read_typed_buffer(&counter).unwrap();
    assert_eq!(
        actual,
        vec![0u32],
        "atomic_min under contention must produce the minimum thread ID (0)"
    );
}

/// 257 threads each attempt `atomic_max(counter, id.x)`.
/// Initial value is 0; the maximum of {0..256} is 256.
#[test]
fn atomic_max_produces_maximum() {
    let device =
        HeadlessDevice::new().expect("Vulkan adapter required; this test must not silently skip");
    let counter = device.create_typed_buffer("atomics max", &[0u32; 1], GpuBufferAccess::ReadWrite);
    device
        .dispatch_buffers(
            &atomics::run_max::DESCRIPTOR,
            INVOCATIONS,
            &[BufferBinding::read_write(&counter).independent_length()],
        )
        .unwrap();
    let actual = device.read_typed_buffer(&counter).unwrap();
    assert_eq!(
        actual,
        vec![INVOCATIONS - 1],
        "atomic_max under contention must produce the maximum thread ID ({})",
        INVOCATIONS - 1
    );
}

/// 257 threads each `atomic_exchange(counter, id.x)`.
/// Final value is non-deterministic (last writer wins) but must be in [0, 256].
#[test]
fn atomic_exchange_produces_valid_value() {
    let device =
        HeadlessDevice::new().expect("Vulkan adapter required; this test must not silently skip");
    let counter = device.create_typed_buffer(
        "atomics exchange",
        &[9999u32; 1],
        GpuBufferAccess::ReadWrite,
    );
    device
        .dispatch_buffers(
            &atomics::run_exchange::DESCRIPTOR,
            INVOCATIONS,
            &[BufferBinding::read_write(&counter).independent_length()],
        )
        .unwrap();
    let actual = device.read_typed_buffer(&counter).unwrap();
    let value = actual[0];
    assert!(
        value < INVOCATIONS,
        "atomic_exchange must produce a value in [0, {INVOCATIONS}), got {value}"
    );
}

/// 257 threads each attempt `atomic_compare_exchange(counter, 0, id.x)`.
/// Only threads that see 0 will succeed. Final value must be in [0, 256].
#[test]
fn atomic_compare_exchange_produces_valid_value() {
    let device =
        HeadlessDevice::new().expect("Vulkan adapter required; this test must not silently skip");
    let counter = device.create_typed_buffer("atomics cas", &[0u32; 1], GpuBufferAccess::ReadWrite);
    device
        .dispatch_buffers(
            &atomics::run_cas::DESCRIPTOR,
            INVOCATIONS,
            &[BufferBinding::read_write(&counter).independent_length()],
        )
        .unwrap();
    let actual = device.read_typed_buffer(&counter).unwrap();
    let value = actual[0];
    assert!(
        value < INVOCATIONS,
        "atomic_compare_exchange must produce a value in [0, {INVOCATIONS}), got {value}"
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
