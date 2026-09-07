use crate::{expand::KernelOptions, slang::emit_kernel, validate::validate_module};

fn translate(source: &str) -> syn::Result<String> {
    let module: syn::ItemMod = syn::parse_str(source)?;
    validate_module(&module)?;
    let kernel = module
        .content
        .as_ref()
        .unwrap()
        .1
        .iter()
        .find_map(|item| match item {
            syn::Item::Fn(function) if function.sig.ident == "run" => Some(function),
            _ => None,
        })
        .unwrap();
    emit_kernel(
        &module,
        kernel,
        KernelOptions {
            workgroup_size: [64, 1, 1],
        },
    )
}

#[test]
fn semantics_golden() {
    let source = translate(include_str!("../../../tests/fixtures/semantics.rs")).unwrap();
    assert_eq!(
        source.replace("\r\n", "\n"),
        include_str!("../../../tests/fixtures/semantics.slang").replace("\r\n", "\n")
    );
}

#[test]
fn numeric_golden() {
    let source = translate(include_str!("../../../tests/fixtures/numeric.rs")).unwrap();
    assert_eq!(
        source.replace("\r\n", "\n"),
        include_str!("../../../tests/fixtures/numeric.slang").replace("\r\n", "\n")
    );
}

#[test]
fn generated_symbol_prefixes_are_reserved() {
    for source in [
        "mod bad { #[kernel] fn run(__gpu_len_out: SV_DispatchThreadID) {} }",
        "mod bad { fn __gust_not(x: uint) -> uint { x } #[kernel] fn run(id: SV_DispatchThreadID) {} }",
    ] {
        let error = translate(source).unwrap_err();
        assert!(error.to_string().contains("reserved"));
    }
}

#[test]
fn invalid_kernel_contracts_are_rejected_early() {
    for (source, message) in [
        (
            "mod bad { #[kernel] fn run(a: SV_DispatchThreadID, b: SV_DispatchThreadID) {} }",
            "exactly one",
        ),
        ("mod bad { #[kernel] fn run() {} }", "exactly one"),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) -> uint { 1u32 } }",
            "return unit",
        ),
        (
            "mod bad { #[kernel(workgroup_size(0, 1, 1))] fn run(id: SV_DispatchThreadID) {} }",
            "nonzero",
        ),
        (
            "mod bad { #[kernel] #[kernel] fn run(id: SV_DispatchThreadID) {} }",
            "duplicate",
        ),
        (
            "mod bad { #[kernel(workgroup_size(64, 1, 1), workgroup_size(32, 1, 1))] fn run(id: SV_DispatchThreadID) {} }",
            "duplicate",
        ),
    ] {
        let module: syn::ItemMod = syn::parse_str(source).unwrap();
        let error = crate::expand::expand(module).unwrap_err();
        assert!(error.to_string().contains(message), "{error}: {source}");
    }
}

#[test]
fn unsupported_attributes_and_builtin_receivers_are_diagnosed() {
    for (source, message) in [
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID, out: RWStructuredBuffer<uint>) { let out = 1u32; let n = out.len(); } }",
            "cannot shadow",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { #[cfg(any())] let ignored = 1u32; } }",
            "attribute",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let n = id.len(); } }",
            "storage parameter",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID, out: RWStructuredBuffer<uint>) { let n = out.len(1u32); } }",
            "arguments",
        ),
        (
            "mod bad { fn helper(b: StructuredBuffer<uint>) -> uint { b.len() } #[kernel] fn run(id: SV_DispatchThreadID) {} }",
            "value parameters",
        ),
        (
            "mod bad { struct Generic<T> { field: T } #[kernel] fn run(id: SV_DispatchThreadID) {} }",
            "generic GPU structs",
        ),
    ] {
        let error = translate(source).unwrap_err();
        assert!(error.to_string().contains(message), "{error}: {source}");
    }
}

#[test]
fn helper_tail_branches_return_values() {
    let source = translate(
        "mod tails {
        fn choose(x: float) -> float { if x > 0.0 { x * 2.0 } else { { -x } } }
        #[kernel] fn run(id: SV_DispatchThreadID, mut out: RWStructuredBuffer<float>) {
            if id.x < out.len() { out[id.x] = choose(2.0); }
        }
    }",
    )
    .unwrap();
    assert!(source.contains("return (x * 2.0);"), "{source}");
    assert!(source.contains("return (-x);"), "{source}");
    assert!(!source.contains("return out["));
}

#[test]
fn typed_locals_preserve_numeric_type() {
    let source = translate("mod typed {
        fn quotient(x: uint) -> uint { let divisor: uint = 2; let mut answer: uint = x / divisor; answer += 1u32; answer }
        #[kernel] fn run(id: SV_DispatchThreadID) { let result: uint = quotient(id.x); }
    }").unwrap();
    assert!(source.contains("uint divisor = 2;"), "{source}");
    assert!(source.contains("uint answer = (x / divisor);"), "{source}");
    assert!(source.contains("uint result = quotient(id.x);"), "{source}");
}

#[test]
fn binding_modes_and_typed_resource_shadowing_fail_explicitly() {
    for (source, message) in [
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let ref x: uint = 1u32; } }",
            "binding modes",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let id: uint = 1u32; } }",
            "cannot shadow",
        ),
        (
            "mod bad { #[kernel] fn run(ref id: SV_DispatchThreadID) {} }",
            "binding modes",
        ),
    ] {
        let error = translate(source).unwrap_err();
        assert!(error.to_string().contains(message), "{error}");
    }
}

#[test]
fn rust_literals_are_lowered_not_copied() {
    let source = translate(
        "mod literals {
        #[kernel] fn run(id: SV_DispatchThreadID, mut out: RWStructuredBuffer<uint>) {
            if id.x < out.len() { out[id.x] = 0xff_u32 + 0b10u32 + 1_024u32 + (2.5f32 as uint); }
        }
    }",
    )
    .unwrap();
    assert!(source.contains("uint(255)"), "{source}");
    assert!(source.contains("uint(2)"), "{source}");
    assert!(source.contains("uint(1024)"), "{source}");
    assert!(source.contains("float(2.5)"), "{source}");
}

#[test]
fn binary_grouping_preserves_rust_precedence() {
    let source = translate(
        "mod grouping {
        fn masked(x: uint) -> bool { return x & 1u32 == 0u32; }
        #[kernel] fn run(id: SV_DispatchThreadID) { let even = masked(id.x); }
    }",
    )
    .unwrap();
    assert!(
        source.contains("return ((x & uint(1)) == uint(0));"),
        "{source}"
    );
}

#[test]
fn struct_constructor_preserves_field_identity_and_source_order() {
    let source = translate(
        "mod constructors {
        struct Pair { first: uint, second: uint }
        #[kernel] fn run(id: SV_DispatchThreadID) { let p = Pair { second: 1u32, first: 2u32 }; }
    }",
    )
    .unwrap();
    // Field identity is preserved by name even though `second` is written first.
    assert!(source.contains("Pair p;"), "{source}");
    assert!(source.contains("p.second = uint(1);"), "{source}");
    assert!(source.contains("p.first = uint(2);"), "{source}");
    // Source evaluation order is preserved: `second` is assigned before `first`.
    assert!(
        source.find("p.second = uint(1);").unwrap() < source.find("p.first = uint(2);").unwrap(),
        "{source}"
    );
}

#[test]
fn struct_literal_in_expression_position_is_diagnosed() {
    let error = translate(
        "mod constructors {
        struct Pair { first: uint, second: uint }
        fn take(p: Pair) -> uint { p.first }
        #[kernel] fn run(id: SV_DispatchThreadID) { let v = take(Pair { first: 1u32, second: 2u32 }); }
    }",
    )
    .unwrap_err();
    assert!(error.to_string().contains("struct literals"), "{error}");
}

#[test]
fn unsupported_numeric_width_is_diagnosed() {
    let error = translate(
        "mod widths {
        #[kernel] fn run(id: SV_DispatchThreadID) { let wide = 1u64; }
    }",
    )
    .unwrap_err();
    assert!(error.to_string().contains("32-bit"), "{error}");
}

#[test]
fn integer_division_by_literal_zero_is_diagnosed() {
    for source in [
        "mod bad { #[kernel] fn run(id: SV_DispatchThreadID, mut out: RWStructuredBuffer<uint>) { out[id.x] = out[id.x] / 0u32; } }",
        "mod bad { #[kernel] fn run(id: SV_DispatchThreadID, mut out: RWStructuredBuffer<int>) { out[id.x] = out[id.x] % 0; } }",
        "mod bad { #[kernel] fn run(id: SV_DispatchThreadID, mut out: RWStructuredBuffer<uint>) { out[id.x] = out[id.x] / -0u32; } }",
    ] {
        let error = translate(source).unwrap_err();
        assert!(
            error.to_string().contains("literal zero"),
            "{error}: {source}"
        );
    }
    // Float division by zero is defined (it yields inf/NaN), so it must not be
    // rejected even though the divisor literal is zero.
    let ok = translate(
        "mod ok { #[kernel] fn run(id: SV_DispatchThreadID, mut out: RWStructuredBuffer<float>) { out[id.x] = out[id.x] / 0.0; } }",
    )
    .unwrap();
    assert!(ok.contains("(out[id.x] / 0.0)"), "{ok}");
}
