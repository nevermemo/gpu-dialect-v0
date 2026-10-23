use std::path::PathBuf;

use super::{ShaderTarget, SlangcOptions, SpirvValOptions, check_spirv_header, sidecar};

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

#[test]
fn slangc_options_parse_in_any_order() {
    let options = SlangcOptions::parse(&args(&[
        "--target",
        "spirv",
        "k.slang",
        "--out",
        "out/k.spv",
        "--entry",
        "main",
    ]))
    .unwrap();
    assert_eq!(options.source, PathBuf::from("k.slang"));
    assert_eq!(options.entry, "main");
    assert_eq!(options.target, ShaderTarget::Spirv);
    assert_eq!(options.out, PathBuf::from("out/k.spv"));
    assert!(options.extra.is_empty());
}

#[test]
fn slangc_compiler_args_are_explicit_and_keep_extra_args_last() {
    let options = SlangcOptions::parse(&args(&[
        "k.slang", "--entry", "main", "--target", "wgsl", "--out", "k.wgsl", "--", "-O0", "-DX=1",
    ]))
    .unwrap();
    assert_eq!(
        options.compiler_args(),
        args(&[
            "k.slang", "-entry", "main", "-stage", "compute", "-target", "wgsl", "-o", "k.wgsl",
            "-O0", "-DX=1",
        ])
    );
}

#[test]
fn slangc_options_reject_missing_unknown_and_repeated_input() {
    for partial in [
        // No source, no entry, no target, no output.
        &["--entry", "main", "--target", "wgsl", "--out", "k"][..],
        &["k.slang", "--target", "wgsl", "--out", "k"][..],
        &["k.slang", "--entry", "main", "--out", "k"][..],
        &["k.slang", "--entry", "main", "--target", "wgsl"][..],
    ] {
        assert!(
            SlangcOptions::parse(&args(partial)).is_err(),
            "{partial:?} should be rejected"
        );
    }
    assert!(SlangcOptions::parse(&args(&["--bogus"])).is_err());
    assert!(
        SlangcOptions::parse(&args(&[
            "a.slang", "b.slang", "--entry", "e", "--target", "wgsl", "--out", "o"
        ]))
        .is_err()
    );
    assert!(
        SlangcOptions::parse(&args(&[
            "a.slang", "--entry", "e", "--target", "glsl", "--out", "o"
        ]))
        .is_err()
    );
    assert!(SlangcOptions::parse(&args(&["a.slang", "--entry"])).is_err());
}

#[test]
fn sidecar_appends_to_the_full_file_name() {
    assert_eq!(
        sidecar(&PathBuf::from("out/vector-add.wgsl"), "command.json"),
        PathBuf::from("out/vector-add.wgsl.command.json")
    );
}

#[test]
fn spirv_val_options_need_one_file_and_an_environment() {
    let options = SpirvValOptions::parse(&args(&["k.spv", "--target-env", "vulkan1.2"])).unwrap();
    assert_eq!(options.file, PathBuf::from("k.spv"));
    assert_eq!(options.target_env, "vulkan1.2");
    assert!(SpirvValOptions::parse(&args(&["k.spv"])).is_err());
    assert!(SpirvValOptions::parse(&args(&["--target-env", "vulkan1.2"])).is_err());
    assert!(SpirvValOptions::parse(&args(&["a.spv", "b.spv", "--target-env", "e"])).is_err());
    assert!(SpirvValOptions::parse(&args(&["a.spv", "--target-env"])).is_err());
    assert!(SpirvValOptions::parse(&args(&["a.spv", "--bogus", "--target-env", "e"])).is_err());
}

#[test]
fn spirv_header_accepts_magic_and_rejects_short_misaligned_or_foreign_files() {
    let mut module = vec![0x03, 0x02, 0x23, 0x07];
    module.resize(20, 0);
    assert!(check_spirv_header(&module).is_ok());
    module.resize(24, 0);
    assert!(check_spirv_header(&module).is_ok());

    assert!(check_spirv_header(&[]).is_err());
    assert!(check_spirv_header(&module[..16]).is_err());
    assert!(check_spirv_header(&module[..22]).is_err());

    let mut big_endian = vec![0x07, 0x23, 0x02, 0x03];
    big_endian.resize(20, 0);
    let error = check_spirv_header(&big_endian).unwrap_err();
    assert!(error.contains("magic"), "{error}");
    let mut text = b"struct Params {}".to_vec();
    text.resize(20, b' ');
    assert!(check_spirv_header(&text).is_err());
}
