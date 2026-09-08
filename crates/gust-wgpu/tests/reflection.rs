use gust::{
    Access, GpuPod, KernelDescriptor, ParameterDescriptor, ParameterKind, ResourceBinding,
    ResourceLayout, reflect::Error as ReflectionError,
};
use gust_wgpu::{BufferBinding, Error, GpuBufferAccess, HeadlessDevice};

const PARAMETERS: &[ParameterDescriptor] = &[ParameterDescriptor {
    name: "output",
    rust_type: "RWStructuredBuffer<float>",
    kind: ParameterKind::Storage,
    access: Access::ReadWrite,
    resource_layout: Some(ResourceLayout::storage_v1(<f32 as GpuPod>::LAYOUT)),
    binding: Some(ResourceBinding {
        group: 0,
        binding: 0,
    }),
}];

const KERNEL: KernelDescriptor = KernelDescriptor {
    module: "reflection_test",
    name: "write",
    entry_point: "write_value",
    workgroup_size: [64, 1, 1],
    parameters: PARAMETERS,
    slang_source: r#"
        [[vk::binding(0, 0)]] RWStructuredBuffer<float> output;
        [shader("compute")] [numthreads(64, 1, 1)]
        void write_value(uint3 id : SV_DispatchThreadID) {
            if (id.x == 0) output[0] = 17.0;
        }
    "#,
};

fn assert_mismatch(error: Error, expected: &str) {
    let Error::Reflection(ReflectionError::Mismatch(message)) = error else {
        panic!("expected a reflection mismatch, got {error}");
    };
    assert_eq!(message, expected);
}

#[test]
fn rejects_workgroup_mismatch_before_dispatch() {
    let device = HeadlessDevice::new().expect("Vulkan adapter required");
    let output = device.create_f32_buffer("reflection gate", &[3.0], GpuBufferAccess::ReadWrite);
    let descriptor = KernelDescriptor {
        workgroup_size: [32, 1, 1],
        ..KERNEL
    };
    let error = device
        .dispatch_buffers(&descriptor, 1, &[BufferBinding::read_write(&output)])
        .expect_err("descriptor/shader workgroup mismatch must fail before dispatch");
    assert_mismatch(error, "compute workgroup dimensions differ");
    assert_eq!(device.read_f32_buffer(&output).unwrap(), [3.0]);
    let stats = device.pipeline_cache_stats();
    assert_eq!((stats.entries, stats.misses, stats.hits), (0, 0, 0));
}

#[test]
fn rejects_resource_mismatches_before_dispatch() {
    let device = HeadlessDevice::new().expect("Vulkan adapter required");
    let floats = device.create_f32_buffer("reflection floats", &[3.0], GpuBufferAccess::ReadWrite);
    let integers =
        device.create_typed_buffer("reflection uints", &[5u32], GpuBufferAccess::ReadWrite);
    for (parameter, binding, expected) in [
        (
            ParameterDescriptor {
                name: "renamed",
                ..PARAMETERS[0]
            },
            BufferBinding::read_write(&floats),
            "missing compiler binding renamed",
        ),
        (
            ParameterDescriptor {
                binding: Some(ResourceBinding {
                    group: 0,
                    binding: 1,
                }),
                ..PARAMETERS[0]
            },
            BufferBinding::read_write(&floats),
            "output.binding: host 1, compiler 0",
        ),
        (
            ParameterDescriptor {
                access: Access::ReadOnly,
                ..PARAMETERS[0]
            },
            BufferBinding::read_only(&floats),
            "output.access differs",
        ),
        (
            ParameterDescriptor {
                resource_layout: Some(ResourceLayout::storage_v1(<u32 as GpuPod>::LAYOUT)),
                ..PARAMETERS[0]
            },
            BufferBinding::read_write(&integers),
            "output: type or field count differs",
        ),
    ] {
        let descriptor = KernelDescriptor {
            parameters: Box::leak(Box::new([parameter])),
            ..KERNEL
        };
        let error = device
            .dispatch_buffers(&descriptor, 1, &[binding])
            .expect_err("resource mismatch must fail before dispatch");
        assert_mismatch(error, expected);
    }
    let descriptor = KernelDescriptor {
        parameters: &[],
        ..KERNEL
    };
    assert_mismatch(
        device.dispatch_buffers(&descriptor, 1, &[]).unwrap_err(),
        "unexpected compiler resource bindings",
    );
    assert_eq!(device.read_f32_buffer(&floats).unwrap(), [3.0]);
    assert_eq!(device.read_typed_buffer(&integers).unwrap(), [5]);
    let stats = device.pipeline_cache_stats();
    assert_eq!((stats.entries, stats.misses, stats.hits), (0, 0, 0));
}

#[test]
fn cached_pipeline_does_not_authorize_a_changed_contract() {
    let device = HeadlessDevice::new().expect("Vulkan adapter required");
    let output = device.create_f32_buffer("cached reflection", &[3.0], GpuBufferAccess::ReadWrite);
    let bindings = [BufferBinding::read_write(&output)];
    device.dispatch_buffers(&KERNEL, 1, &bindings).unwrap();
    let descriptor = KernelDescriptor {
        workgroup_size: [32, 1, 1],
        ..KERNEL
    };
    assert_mismatch(
        device
            .dispatch_buffers(&descriptor, 1, &bindings)
            .unwrap_err(),
        "compute workgroup dimensions differ",
    );
    let descriptor = KernelDescriptor {
        parameters: Box::leak(Box::new([ParameterDescriptor {
            name: "renamed",
            ..PARAMETERS[0]
        }])),
        ..KERNEL
    };
    assert_mismatch(
        device
            .dispatch_buffers(&descriptor, 1, &bindings)
            .unwrap_err(),
        "missing compiler binding renamed",
    );
    device.dispatch_buffers(&KERNEL, 1, &bindings).unwrap();
    assert_eq!(device.read_f32_buffer(&output).unwrap(), [17.0]);
    let stats = device.pipeline_cache_stats();
    assert_eq!((stats.entries, stats.misses, stats.hits), (1, 1, 1));
}

#[gust::gpu]
mod nested {
    pub struct Inner {
        pub value: float,
        pub tag: uint,
    }
    pub struct Outer {
        pub inner: Inner,
        pub weight: int,
    }
    #[kernel]
    pub fn transform(
        id: SV_DispatchThreadID,
        input: StructuredBuffer<Outer>,
        tags: StructuredBuffer<uint>,
        mut output: RWStructuredBuffer<Outer>,
        mut results: RWStructuredBuffer<uint>,
    ) {
        if id.x < output.len() {
            output[id.x] = input[id.x];
            output[id.x].inner.value = input[id.x].inner.value * 2.0 + 1.0;
            output[id.x].inner.tag = tags[id.x] + 7;
            output[id.x].weight = -input[id.x].weight;
            results[id.x] = tags[id.x] * 3 + id.x;
        }
    }
}

#[test]
fn reflected_nested_and_scalar_buffers_execute_with_cache_hits() {
    let device = HeadlessDevice::new().expect("Vulkan adapter required; no silent GPU skip");
    for count in [0, 1, 63, 64, 65, 257] {
        let values: Vec<_> = (0..count)
            .map(|index| nested::Outer {
                inner: nested::Inner {
                    value: index as f32 * 0.5 - 19.0,
                    tag: 91,
                },
                weight: index as i32 - 33,
            })
            .collect();
        let tags: Vec<_> = (0..count).map(|index| index as u32 * 11 + 2).collect();
        let zeros = vec![
            nested::Outer {
                inner: nested::Inner { value: 0.0, tag: 0 },
                weight: 0,
            };
            count
        ];
        let input = device.create_typed_buffer("nested input", &values, GpuBufferAccess::ReadOnly);
        let tag_input =
            device.create_typed_buffer("scalar input", &tags, GpuBufferAccess::ReadOnly);
        let output =
            device.create_typed_buffer("nested output", &zeros, GpuBufferAccess::ReadWrite);
        let results = device.create_typed_buffer(
            "scalar output",
            &vec![0u32; count],
            GpuBufferAccess::ReadWrite,
        );
        for _ in 0..2 {
            device
                .dispatch_buffers(
                    &nested::transform::DESCRIPTOR,
                    count as u32,
                    &[
                        BufferBinding::read_only(&input),
                        BufferBinding::read_only(&tag_input),
                        BufferBinding::read_write(&output),
                        BufferBinding::read_write(&results),
                    ],
                )
                .unwrap();
            let actual = device.read_typed_buffer(&output).unwrap();
            let actual_tags = device.read_typed_buffer(&results).unwrap();
            assert_eq!(actual.len(), count);
            assert_eq!(actual_tags.len(), count);
            for index in 0..count {
                assert_eq!(actual[index].inner.value, index as f32 - 37.0);
                assert_eq!(actual[index].inner.tag, index as u32 * 11 + 9);
                assert_eq!(actual[index].weight, 33 - index as i32);
                assert_eq!(actual_tags[index], index as u32 * 34 + 6);
            }
        }
    }
    let stats = device.pipeline_cache_stats();
    assert_eq!((stats.entries, stats.misses, stats.hits), (1, 1, 9));
}
