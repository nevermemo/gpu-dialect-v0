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
    let source = translate(include_str!("../../../../tests/fixtures/semantics.rs")).unwrap();
    assert_eq!(
        source.replace("\r\n", "\n"),
        include_str!("../../../../tests/fixtures/semantics.slang").replace("\r\n", "\n")
    );
}

#[test]
fn numeric_golden() {
    let source = translate(include_str!("../../../../tests/fixtures/numeric.rs")).unwrap();
    assert_eq!(
        source.replace("\r\n", "\n"),
        include_str!("../../../../tests/fixtures/numeric.slang").replace("\r\n", "\n")
    );
}

#[test]
fn option_golden() {
    let source = translate(include_str!("../../../../tests/fixtures/option.rs")).unwrap();
    assert_eq!(
        source.replace("\r\n", "\n"),
        include_str!("../../../../tests/fixtures/option.slang").replace("\r\n", "\n")
    );
}

#[test]
fn loops_golden() {
    let source = translate(include_str!("../../../../tests/fixtures/loops.rs")).unwrap();
    assert_eq!(
        source.replace("\r\n", "\n"),
        include_str!("../../../../tests/fixtures/loops.slang").replace("\r\n", "\n")
    );
}

#[test]
fn unbounded_or_unproven_loop_forms_are_rejected() {
    for (source, message) in [
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let mut i = 0u32; while i < 4u32 { i = i + 1u32; } } }",
            "`while`",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { loop { break; } } }",
            "`loop`",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { for i in 0u32..=4u32 { } } }",
            "inclusive",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { for i in 0u32.. { } } }",
            "both bounds",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { for i in ..4u32 { } } }",
            "both bounds",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID, input: StructuredBuffer<uint>) { for v in input { } } }",
            "exclusive integer range",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { for (a, b) in 0u32..4u32 { } } }",
            "single identifier",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { for mut i in 0u32..4u32 { i = i + 1u32; } } }",
            "immutable",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { 'outer: for i in 0u32..4u32 { break 'outer; } } }",
            "labels",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { for i in 0u32..4u32 { break 'x; } } }",
            "labels",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { if id.x > 1u32 { break; } } }",
            "outside a loop",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { continue; } }",
            "outside a loop",
        ),
        (
            "mod bad { fn f() -> uint { for i in 0u32..4u32 { fn g() {} } 0u32 } #[kernel] fn run(id: SV_DispatchThreadID) { } }",
            "items inside",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let r = 0u32..4u32; } }",
            "only as the iterable",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { for i in 0..4u32 { } } }",
            "suffix",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { for i in 0u32..(-4) { } } }",
            "suffix",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID, out: RWStructuredBuffer<uint>) { for out in 0u32..4u32 { } } }",
            "cannot shadow",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { for i in 0u32..4u32 { let v = break; } } }",
            "statement",
        ),
    ] {
        let error = translate(source).unwrap_err();
        assert!(error.to_string().contains(message), "{error}: {source}");
    }
}

#[test]
fn for_loop_end_bound_is_evaluated_once_and_body_control_flow_maps_directly() {
    let source = translate(
        "mod shapes {
        fn count(limit: uint) -> uint {
            let mut n = 0u32;
            for i in 2u32..limit { if i == 3u32 { continue; } if i > 6u32 { break; } n = n + i; }
            n
        }
        #[kernel] fn run(id: SV_DispatchThreadID, mut out: RWStructuredBuffer<uint>) {
            if id.x < out.len() { for _ in 0u32..2u32 { out[id.x] = count(out[id.x]); } }
        }
    }",
    )
    .unwrap();
    assert!(source.contains("var __gust_end_1 = limit;"), "{source}");
    assert!(
        source.contains("for (var i = uint(2); i < __gust_end_1; i++)"),
        "{source}"
    );
    assert!(source.contains("continue;"), "{source}");
    assert!(source.contains("break;"), "{source}");
    assert!(
        source.contains(
            "for (var __gust_iter_0 = uint(0); __gust_iter_0 < __gust_end_0; __gust_iter_0++)"
        ),
        "{source}"
    );
}

#[test]
fn option_unsafe_forms_are_rejected() {
    for (source, message) in [
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let o: Option<uint> = Some(1u32); let v = o.unwrap(); } }",
            "panics",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let o: Option<uint> = Some(1u32); let v = o.expect(\"x\"); } }",
            "panics",
        ),
        (
            "mod bad { struct Holder { slot: Option<uint> } #[kernel] fn run(id: SV_DispatchThreadID) {} }",
            "struct fields",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID, out: RWStructuredBuffer<Option<uint>>) {} }",
            "resource element",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let o: Option<Option<uint>> = None; } }",
            "payload",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let o: Option<uint> = Some(1u32); if let Some(_) = o { } } }",
            "`Some(identifier)`",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let o: Option<uint> = Some(1u32); if let None = o { } } }",
            "`Some(identifier)`",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let o: Option<uint> = Some(1u32); if let Some(v) = o && v > 0u32 { } } }",
            "`if let`",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID, out: RWStructuredBuffer<uint>) { let o: Option<uint> = Some(1u32); if let Some(out) = o { } } }",
            "cannot shadow",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let o: Option<uint> = Some(1u32, 2u32); } }",
            "one argument",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let o: Option<uint> = Some(1u32); let v = o.unwrap_or(1u32, 2u32); } }",
            "one argument",
        ),
    ] {
        let error = translate(source).unwrap_err();
        assert!(error.to_string().contains(message), "{error}: {source}");
    }
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
    assert!(source.contains("Pair __gust_struct_0;"), "{source}");
    assert!(
        source.contains("__gust_struct_0.second = uint(1);"),
        "{source}"
    );
    assert!(
        source.contains("__gust_struct_0.first = uint(2);"),
        "{source}"
    );
    assert!(source.contains("Pair p = __gust_struct_0;"), "{source}");
    // Source evaluation order is preserved: `second` is assigned before `first`.
    assert!(
        source.find("__gust_struct_0.second = uint(1);").unwrap()
            < source.find("__gust_struct_0.first = uint(2);").unwrap(),
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
fn struct_update_syntax_is_rejected_instead_of_dropping_fields() {
    for statement in [
        "let q = Pair { first: 9u32, ..p };",
        "p = Pair { first: 9u32, ..p };",
    ] {
        let source = format!(
            "mod update {{ struct Pair {{ first: uint, second: uint }} #[kernel] fn run(id: SV_DispatchThreadID) {{ let mut p = Pair {{ first: 1u32, second: 2u32 }}; {statement} }} }}"
        );
        let error = translate(&source).unwrap_err();
        assert!(error.to_string().contains("struct update"), "{error}");
    }
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
fn casts_to_unproven_targets_are_rejected() {
    // `uint` is a plain `u32` alias, so rustc accepts every numeric target below in
    // the shadow body; only the macro can stop `f64(id.x)` reaching slangc.
    for target in [
        "f64",
        "u64",
        "i64",
        "usize",
        "isize",
        "u8",
        "u16",
        "i8",
        "i16",
        "bool",
        "char",
        "double",
        "Pair",
        "_",
        "*const uint",
        "gpu_dialect::uint",
    ] {
        let source = format!(
            "mod bad {{ struct Pair {{ first: uint, second: uint }} #[kernel] fn run(id: SV_DispatchThreadID) {{ let v = id.x as {target}; }} }}"
        );
        let error = translate(&source).unwrap_err();
        assert!(error.to_string().contains("cast"), "{error}: {source}");
    }
    for (target, expected) in [
        ("f32", "float(id.x)"),
        ("float", "float(id.x)"),
        ("i32", "int(id.x)"),
        ("int", "int(id.x)"),
        ("u32", "uint(id.x)"),
        ("uint", "uint(id.x)"),
    ] {
        let source = format!(
            "mod ok {{ #[kernel] fn run(id: SV_DispatchThreadID) {{ let v = id.x as {target}; }} }}"
        );
        let ok = translate(&source).unwrap();
        assert!(ok.contains(expected), "{ok}");
    }
}

#[test]
fn non_32_bit_primitive_types_are_rejected_before_slang() {
    // rustc accepts each of these in the shadow; without a validator rule they
    // became `f64 v = 1.0;` and failed only inside slangc at pipeline creation.
    for (source, name) in [
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let v: f64 = 1.0; } }",
            "f64",
        ),
        (
            "mod bad { fn helper(x: u64) -> uint { 1u32 } #[kernel] fn run(id: SV_DispatchThreadID) {} }",
            "u64",
        ),
        (
            "mod bad { fn helper(x: uint) -> usize { 1 } #[kernel] fn run(id: SV_DispatchThreadID) {} }",
            "usize",
        ),
        (
            "mod bad { struct Wide { value: f64 } #[kernel] fn run(id: SV_DispatchThreadID) {} }",
            "f64",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID, out: RWStructuredBuffer<i64>) {} }",
            "i64",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let c: char = 'a'; } }",
            "char",
        ),
    ] {
        let error = translate(source).unwrap_err();
        let message = error.to_string();
        assert!(
            message.contains("not a supported GPU type") && message.contains(name),
            "{error}: {source}"
        );
    }
    let ok = translate(
        "mod ok { struct Mixed { a: float, b: int, c: uint, d: bool, e: f32, f: i32, g: u32 } #[kernel] fn run(id: SV_DispatchThreadID) { let flag: bool = true; } }",
    )
    .unwrap();
    assert!(ok.contains("bool flag = true;"), "{ok}");
}

#[test]
fn const_blocks_are_rejected() {
    for source in [
        "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let v = const { 1u32 }; } }",
        "mod bad { #[kernel] fn run(id: SV_DispatchThreadID, mut out: RWStructuredBuffer<uint>) { out[id.x] = const { 2u32 } + 1u32; } }",
        "mod bad { fn helper() -> uint { const { 3u32 } } #[kernel] fn run(id: SV_DispatchThreadID) {} }",
    ] {
        let error = translate(source).unwrap_err();
        assert!(
            error.to_string().contains("const blocks"),
            "{error}: {source}"
        );
    }
}

#[test]
fn unsupported_name_paths_are_rejected() {
    for (source, message) in [
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let v = f32::INFINITY; } }",
            "associated constants",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let v = u32::MAX; } }",
            "associated constants",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let v = core::f32::consts::PI; } }",
            "associated constants",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { let v: core::f32 = 1.0; } }",
            "CPU-only",
        ),
    ] {
        let error = translate(source).unwrap_err();
        assert!(error.to_string().contains(message), "{error}: {source}");
    }
}

#[test]
fn unsupported_effects_are_rejected() {
    for (source, message) in [
        (
            "mod bad { fn maybe() -> Option<uint> { 1u32 } #[kernel] fn run(id: SV_DispatchThreadID) { let v = maybe()?; } }",
            "`?` operator",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { unsafe { let v = 1u32; } } }",
            "unsafe blocks",
        ),
        (
            "mod bad { #[kernel] fn run(id: SV_DispatchThreadID) { asm!(\"nop\"); } }",
            "macros",
        ),
    ] {
        let error = translate(source).unwrap_err();
        assert!(error.to_string().contains(message), "{error}: {source}");
    }
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
