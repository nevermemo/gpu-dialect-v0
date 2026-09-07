include!("../../../tests/fixtures/semantics.rs");

use gpu_dialect_wgpu::{BufferBinding, GpuBufferAccess, HeadlessDevice};

#[test]
fn translated_semantics_execute_on_gpu() {
    let device =
        HeadlessDevice::new().expect("Vulkan adapter required; this test must not silently skip");
    for count in [1, 63, 64, 65, 257] {
        let values: Vec<f32> = (0..count).map(|i| (i % 7) as f32 - 3.0).collect();
        let input =
            device.create_typed_buffer("semantics input", &values, GpuBufferAccess::ReadOnly);
        let floats = device.create_typed_buffer(
            "semantics floats",
            &vec![0.0f32; count],
            GpuBufferAccess::ReadWrite,
        );
        let integers = device.create_typed_buffer(
            "semantics integers",
            &vec![0u32; count],
            GpuBufferAccess::ReadWrite,
        );
        device
            .dispatch_buffers(
                &semantics::run::DESCRIPTOR,
                count as u32,
                &[
                    BufferBinding::read_only(&input),
                    BufferBinding::read_write(&floats),
                    BufferBinding::read_write(&integers),
                ],
            )
            .unwrap();
        let actual_floats = device.read_typed_buffer(&floats).unwrap();
        let actual_integers = device.read_typed_buffer(&integers).unwrap();
        for (i, &x) in values.iter().enumerate() {
            // Independent host expectations, not execution of a shader shadow body.
            let expected = if x > 0.0 {
                x * 2.5 + 1.0
            } else if x < 0.0 {
                -x
            } else {
                7.0
            };
            assert_eq!(actual_floats[i], expected + if i >= 2 { 1.0 } else { 0.0 });
            let i = i as u32;
            let expected = if i.is_multiple_of(2) {
                255 - (i % 256) + 2 + (i / 2) + 1
            } else {
                i + 1024
            };
            assert_eq!(actual_integers[i as usize], expected);
        }
    }
}

#[test]
fn semantics_compile_to_both_targets() {
    let descriptor = &semantics::run::DESCRIPTOR;
    assert!(
        gpu_dialect::slang::compile_wgsl(descriptor)
            .unwrap()
            .contains("@compute")
    );
    let words = gpu_dialect::slang::compile_spirv(descriptor).unwrap();
    gpu_dialect::spirv::validate_structure(&words).unwrap();
}
