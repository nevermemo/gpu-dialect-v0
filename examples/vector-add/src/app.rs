use std::path::PathBuf;

#[cfg(test)]
use gpu_dialect::{Access, ResourceBinding};
use gpu_dialect::{gpu, slang, spirv};
use gpu_dialect_wgpu::{F32Binding, HeadlessDevice, render_wgpu_source};

#[gpu]
mod vector_add {
    #[kernel(workgroup_size(64, 1, 1))]
    pub fn add(
        id: SV_DispatchThreadID,
        a: StructuredBuffer<float>,
        b: StructuredBuffer<float>,
        mut out: RWStructuredBuffer<float>,
    ) {
        let i = id.x;
        if i < out.len() {
            out[i] = a[i] + b[i];
        }
    }
}

pub fn main() {
    let a = [1.0, 2.0, 3.0, 4.0];
    let b = [10.0, 20.0, 30.0, 40.0];
    let expected = a
        .iter()
        .zip(b)
        .map(|(left, right)| left + right)
        .collect::<Vec<_>>();
    let seed = [0.0; 4];
    let descriptor = vector_add::add::DESCRIPTOR;

    let device = HeadlessDevice::new().expect("a Vulkan compute adapter should be available");
    let result = device
        .dispatch_f32(
            &descriptor,
            a.len() as u32,
            &[
                F32Binding::ReadOnly(&a),
                F32Binding::ReadOnly(&b),
                F32Binding::ReadWrite(&seed),
            ],
        )
        .expect("Rust-to-Slang vector add should execute on the GPU");

    let artifact_directory = export_artifacts().expect("export generated shader artifacts");
    println!("Rust -> Slang -> WGSL -> wgpu (plus SPIR-V export)");
    println!("kernel: {}", descriptor.qualified_name());
    println!("adapter: {}", device.adapter_info().name);
    println!("GPU result: {:?}", result.writable_buffers[0]);
    println!("generated artifacts: {}", artifact_directory.display());
    println!("\nGenerated Slang:\n{}", descriptor.slang_source);

    assert_eq!(result.writable_buffers[0], expected);
}

fn export_artifacts() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../generated-wgpu");
    std::fs::create_dir_all(&directory)?;
    let descriptor = &vector_add::add::DESCRIPTOR;
    std::fs::write(
        directory.join("vector_add__add.slang"),
        descriptor.slang_source,
    )?;
    std::fs::write(
        directory.join("vector_add__add.wgsl"),
        slang::compile_wgsl(descriptor)?,
    )?;
    std::fs::write(
        directory.join("vector_add__add.spv"),
        spirv::words_as_le_bytes(&slang::compile_spirv(descriptor)?),
    )?;
    std::fs::write(
        directory.join("vector_add__add.rs"),
        render_wgpu_source(descriptor)?,
    )?;
    Ok(directory.canonicalize().unwrap_or(directory))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "example validation runs only in full verification"]
    fn descriptor_is_slang_first_and_reflects_bindings() {
        let descriptor = vector_add::add::DESCRIPTOR;
        assert!(
            descriptor
                .slang_source
                .contains("StructuredBuffer<float> a")
        );
        assert!(
            descriptor
                .slang_source
                .contains("RWStructuredBuffer<float> out")
        );
        assert!(descriptor.slang_source.contains("SV_DispatchThreadID"));
        assert_eq!(descriptor.entry_point, "gpu_vector_add_add");
        assert_eq!(descriptor.parameters[0].binding, None);
        assert_eq!(descriptor.parameters[1].access, Access::ReadOnly);
        assert_eq!(
            descriptor.parameters[1].binding,
            Some(ResourceBinding {
                group: 0,
                binding: 0,
            })
        );
        assert_eq!(
            descriptor.parameters[3].binding,
            Some(ResourceBinding {
                group: 0,
                binding: 2,
            })
        );
    }

    #[test]
    #[ignore = "example validation runs only in full verification"]
    fn slang_compiles_to_readable_wgsl() {
        let descriptor = vector_add::add::DESCRIPTOR;
        let wgsl = slang::compile_wgsl(&descriptor).expect("Slang WGSL compilation");
        assert!(wgsl.contains("@compute"));
        assert!(wgsl.contains("@binding(2) @group(0)"));
        assert!(wgsl.contains("a_0[i_0] + b_0[i_0]"));
    }

    #[test]
    #[ignore = "example validation runs only in full verification"]
    fn rust_to_slang_executes_on_headless_wgpu() {
        let device = HeadlessDevice::new().expect("Vulkan adapter");
        let a = [2.0, 4.0, 8.0];
        let b = [3.0, 5.0, 7.0];
        let seed = [0.0; 3];
        let result = device
            .dispatch_f32(
                &vector_add::add::DESCRIPTOR,
                3,
                &[
                    F32Binding::ReadOnly(&a),
                    F32Binding::ReadOnly(&b),
                    F32Binding::ReadWrite(&seed),
                ],
            )
            .expect("dispatch through Slang");
        assert_eq!(result.writable_buffers[0], [5.0, 9.0, 15.0]);
    }
}
