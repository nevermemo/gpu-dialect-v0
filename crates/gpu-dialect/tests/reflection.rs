use gpu_dialect::{
    cross_check_pod,
    reflect::{Reflection, reflect},
    slang::Target,
};

#[gpu_dialect::gpu]
mod sample {
    pub struct Inner {
        pub value: float,
        pub tag: uint,
    }
    pub struct Outer {
        pub inner: Inner,
        pub weight: int,
    }
    #[kernel]
    pub fn run(
        id: SV_DispatchThreadID,
        input: StructuredBuffer<Outer>,
        mut output: RWStructuredBuffer<Outer>,
    ) {
        if id.x < output.len() {
            output[id.x] = input[id.x];
        }
    }
}

#[test]
fn compiler_reports_nested_fields_for_both_targets() {
    for target in [Target::Wgsl, Target::Spirv] {
        let reflection = reflect(&sample::run::DESCRIPTOR, target).unwrap();
        assert_eq!(reflection.resources.len(), 2);
        let input = &reflection.resources[0];
        assert_eq!((input.binding, input.space), (0, 0));
        let checked = cross_check_pod::<sample::Outer>(input).unwrap();
        assert_eq!(checked.checked_fields, 4);
        assert!(!checked.alignment_verified);
        assert!(!checked.aggregate_size_verified);
        assert!(!checked.element_stride_verified);
        assert!(!checked.is_full_abi_match());
        assert_eq!(input.access, gpu_dialect::Access::ReadOnly);
        assert_eq!(
            reflection.resources[1].access,
            gpu_dialect::Access::ReadWrite
        );
        assert!(cross_check_pod::<u32>(input).is_err());
        let mut mismatch = input.clone();
        mismatch.element_stride = Some(64);
        assert!(cross_check_pod::<sample::Outer>(&mismatch).is_err());
        for change in 0..5 {
            let mut mismatch = input.clone();
            let gpu_dialect::ReflectedTypeKind::Struct(fields) = &mut mismatch.element.kind else {
                panic!("struct expected")
            };
            match change {
                0 => fields[0].offset = 4,
                1 => fields[0].size = 4,
                2 => fields[0].name = "wrong".into(),
                3 => {
                    fields.pop();
                }
                _ => {
                    fields[1].ty.kind =
                        gpu_dialect::ReflectedTypeKind::Scalar(gpu_dialect::ScalarKind::U32)
                }
            }
            assert!(
                cross_check_pod::<sample::Outer>(&mismatch).is_err(),
                "mutation {change}"
            );
        }
    }
}

#[test]
fn compiler_failure_preserves_target_and_kernel_diagnostics() {
    let mut kernel = sample::run::DESCRIPTOR;
    kernel.slang_source = "this is invalid Slang";
    let error = reflect(&kernel, Target::Wgsl).unwrap_err();
    assert!(matches!(
        error,
        gpu_dialect::reflect::Error::Compiler(gpu_dialect::slang::Error::CompilationFailed {
            target: Target::Wgsl,
            ..
        })
    ));
    assert!(error.to_string().contains("compiler exit:"));
    assert!(
        error
            .to_string()
            .contains("originating Rust kernel: sample::run")
    );
}

#[test]
fn scalar_fixture_records_nonzero_space_and_rejects_unknown_layouts() {
    let source = r#"{"parameters":[{"name":"a","binding":{"kind":"descriptorTableSlot","index":7,"space":2},"type":{"kind":"resource","baseShape":"structuredBuffer","resultType":{"kind":"scalar","scalarType":"uint32"}}}]}"#;
    let reflection = Reflection::from_json(Target::Spirv, source).unwrap();
    let resource = &reflection.resources[0];
    assert_eq!((resource.binding, resource.space), (7, 2));
    assert_eq!(cross_check_pod::<u32>(resource).unwrap().checked_fields, 0);
    for invalid in [
        source.replace("uint32", "bool"),
        source.replace("structuredBuffer", "texture2D"),
        source.replace("\"space\":2", "\"space\":-1"),
        source.replace("\"space\":2", "\"count\":2"),
        source.replace("\"resultType\"", "\"access\":\"unknown\",\"resultType\""),
    ] {
        assert!(Reflection::from_json(Target::Spirv, &invalid).is_err());
    }
}

#[test]
fn rejects_malformed_or_unsupported_reflection() {
    for json in [
        "{}",
        "{\"parameters\":[],}",
        "{\"parameters\":[],\"parameters\":[]}",
        "{\"parameters\":[]} trailing",
        "{\"parameters\":[{}]}",
    ] {
        assert!(Reflection::from_json(Target::Wgsl, json).is_err(), "{json}");
    }
}

#[test]
fn entry_point_resources_are_not_silently_ignored() {
    for parameter in [
        r#"{"name":"buffer","type":{"kind":"resource"}}"#,
        r#"{"name":"uniforms","type":{"kind":"parameterBlock"}}"#,
        r#"{"name":"value","binding":{"kind":"uniform","offset":0,"size":4},"type":{"kind":"scalar","scalarType":"uint32"}}"#,
        r#"{"name":"value","type":{"kind":"scalar","scalarType":"uint32"}}"#,
    ] {
        let json = format!(r#"{{"parameters":[],"entryPoints":[{{"parameters":[{parameter}]}}]}}"#);
        assert!(
            Reflection::from_json(Target::Wgsl, &json).is_err(),
            "{json}"
        );
    }
}
