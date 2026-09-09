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

/// 257 threads each attempt to replace the initial sentinel value. One succeeds,
/// so a no-op implementation cannot pass.
#[test]
fn atomic_compare_exchange_contention_changes_the_initial_value() {
    let device =
        HeadlessDevice::new().expect("Vulkan adapter required; this test must not silently skip");
    let counter =
        device.create_typed_buffer("atomics cas", &[999u32; 1], GpuBufferAccess::ReadWrite);
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
        (1..=INVOCATIONS).contains(&value),
        "atomic_compare_exchange must replace the initial sentinel with a thread value in [1, {INVOCATIONS}], got {value}"
    );
}

#[test]
fn atomic_compare_exchange_has_deterministic_success_and_failure_cases() {
    let device =
        HeadlessDevice::new().expect("Vulkan adapter required; this test must not silently skip");
    let success = device.create_typed_buffer(
        "atomics cas success",
        &[7u32; 1],
        GpuBufferAccess::ReadWrite,
    );
    device
        .dispatch_buffers(
            &atomics::run_cas_success::DESCRIPTOR,
            1,
            &[BufferBinding::read_write(&success).independent_length()],
        )
        .unwrap();
    assert_eq!(device.read_typed_buffer(&success).unwrap(), vec![11u32]);

    let failure = device.create_typed_buffer(
        "atomics cas failure",
        &[7u32; 1],
        GpuBufferAccess::ReadWrite,
    );
    device
        .dispatch_buffers(
            &atomics::run_cas_failure::DESCRIPTOR,
            1,
            &[BufferBinding::read_write(&failure).independent_length()],
        )
        .unwrap();
    assert_eq!(device.read_typed_buffer(&failure).unwrap(), vec![7u32]);
}

#[test]
fn signed_atomic_add_exchange_and_compare_exchange_execute() {
    let device =
        HeadlessDevice::new().expect("Vulkan adapter required; this test must not silently skip");
    let counter =
        device.create_typed_buffer("signed atomics", &[-7i32; 1], GpuBufferAccess::ReadWrite);
    let binding = [BufferBinding::read_write(&counter).independent_length()];
    device
        .dispatch_buffers(&atomics::run_signed_add::DESCRIPTOR, 1, &binding)
        .unwrap();
    assert_eq!(device.read_typed_buffer(&counter).unwrap(), vec![-4i32]);
    device
        .dispatch_buffers(&atomics::run_signed_exchange::DESCRIPTOR, 1, &binding)
        .unwrap();
    assert_eq!(device.read_typed_buffer(&counter).unwrap(), vec![-5i32]);
    device
        .dispatch_buffers(&atomics::run_signed_cas::DESCRIPTOR, 1, &binding)
        .unwrap();
    assert_eq!(device.read_typed_buffer(&counter).unwrap(), vec![11i32]);
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
