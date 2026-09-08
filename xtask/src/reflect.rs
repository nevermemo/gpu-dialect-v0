use std::{
    env, fs,
    path::{Path, PathBuf},
};

use crate::{
    process::{command_exists, run_direct, run_direct_args},
    workspace::workspace_root,
};

#[derive(Default)]
pub(crate) struct BuildOptions {
    force: bool,
    sdk_root: Option<PathBuf>,
    out_dir: Option<PathBuf>,
}

impl BuildOptions {
    pub(crate) fn parse(args: &[String]) -> Result<Self, String> {
        let mut options = Self::default();
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--force" | "-Force" => options.force = true,
                "--sdk-root" | "-SdkRoot" => {
                    index += 1;
                    options.sdk_root = Some(PathBuf::from(
                        args.get(index).ok_or("--sdk-root needs a path")?,
                    ));
                }
                "--out-dir" | "--output-directory" | "-OutputDirectory" => {
                    index += 1;
                    options.out_dir = Some(PathBuf::from(
                        args.get(index).ok_or("--out-dir needs a path")?,
                    ));
                }
                other => return Err(format!("unknown build-slang-reflect option `{other}`")),
            }
            index += 1;
        }
        Ok(options)
    }
}

pub(crate) fn build_slang_reflect(options: BuildOptions) -> Result<(), String> {
    let root = workspace_root()?;
    let sdk_root = options
        .sdk_root
        .or_else(|| env::var_os("SLANG_SDK").map(PathBuf::from))
        .or_else(|| env::var_os("VULKAN_SDK").map(PathBuf::from))
        .ok_or("Set SLANG_SDK (standalone Slang SDK) or VULKAN_SDK, or pass --sdk-root.")?;
    let out_dir = options
        .out_dir
        .unwrap_or_else(|| root.join("target/slang-reflect"));
    fs::create_dir_all(&out_dir).map_err(|error| error.to_string())?;
    let source = root.join("scripts/probes/slang-layout.cpp");
    let include = [sdk_root.join("include/slang"), sdk_root.join("include")]
        .into_iter()
        .find(|path| path.join("slang.h").is_file())
        .ok_or_else(|| {
            format!(
                "slang.h was not found under {}/include[/slang].",
                sdk_root.display()
            )
        })?;
    let lib_dir = sdk_root.join("lib");
    let bin_dir = sdk_root.join("bin");
    if cfg!(windows) {
        build_slang_reflect_windows(
            options.force,
            &source,
            &include,
            &lib_dir,
            &bin_dir,
            &out_dir,
        )
    } else {
        build_slang_reflect_unix(options.force, &source, &include, &lib_dir, &out_dir)
    }
}

fn build_slang_reflect_windows(
    force: bool,
    source: &Path,
    include: &Path,
    lib_dir: &Path,
    bin_dir: &Path,
    out_dir: &Path,
) -> Result<(), String> {
    let library = ["slang-compiler.lib", "slang.lib"]
        .iter()
        .map(|name| lib_dir.join(name))
        .find(|path| path.is_file())
        .ok_or_else(|| format!("No Slang import library in {}.", lib_dir.display()))?;
    let dll = bin_dir.join(format!(
        "{}.dll",
        library
            .file_stem()
            .and_then(|name| name.to_str())
            .ok_or("invalid Slang library name")?
    ));
    if !dll.is_file() {
        return Err(format!(
            "Missing matching compiler library: {}",
            dll.display()
        ));
    }
    let exe = out_dir.join("gust-slang-reflect.exe");
    if !force && newer_than_all(&exe, &[source, &library, &dll])? {
        run_version(&exe)?;
        println!("Reflection compiler: {} (up to date)", exe.display());
        return Ok(());
    }
    let obj = out_dir.join("gust-slang-reflect.obj");
    let args = vec![
        "/nologo".into(),
        "/std:c++17".into(),
        "/EHsc".into(),
        "/W4".into(),
        "/WX".into(),
        format!("/I{}", include.display()),
        source.display().to_string(),
        format!("/Fo{}", obj.display()),
        format!("/Fe{}", exe.display()),
        "/link".into(),
        library.display().to_string(),
    ];
    if command_exists("cl") {
        run_direct_args(Path::new("."), "cl", &args)?;
    } else {
        let vcvars = find_vcvars64()?;
        let command = format!("\"{}\" && cl {}", vcvars.display(), quote_cmd_args(&args));
        run_direct_args(Path::new("."), "cmd", &["/C".into(), command])?;
    }
    fs::copy(
        &dll,
        out_dir.join(dll.file_name().ok_or("invalid dll name")?),
    )
    .map_err(|error| error.to_string())?;
    for name in ["slang-glsl-module.dll", "slang-glslang.dll", "slang-rt.dll"] {
        let dependency = bin_dir.join(name);
        if dependency.is_file() {
            fs::copy(&dependency, out_dir.join(name)).map_err(|error| error.to_string())?;
        }
    }
    run_version(&exe)?;
    println!("Reflection compiler: {}", exe.display());
    Ok(())
}

fn build_slang_reflect_unix(
    force: bool,
    source: &Path,
    include: &Path,
    lib_dir: &Path,
    out_dir: &Path,
) -> Result<(), String> {
    let extension = if cfg!(target_os = "macos") {
        "dylib"
    } else {
        "so"
    };
    let library = [
        format!("libslang-compiler.{extension}"),
        format!("libslang.{extension}"),
    ]
    .into_iter()
    .map(|name| lib_dir.join(name))
    .find(|path| path.is_file())
    .ok_or_else(|| format!("No Slang shared library in {}.", lib_dir.display()))?;
    let exe = out_dir.join("gust-slang-reflect");
    if !force && newer_than_all(&exe, &[source, &library])? {
        run_version(&exe)?;
        println!("Reflection compiler: {} (up to date)", exe.display());
        return Ok(());
    }
    let compiler = env::var("CXX").unwrap_or_else(|_| "c++".to_owned());
    let args = vec![
        "-std=c++17".into(),
        "-Wall".into(),
        "-Wextra".into(),
        "-Werror".into(),
        format!("-I{}", include.display()),
        source.display().to_string(),
        library.display().to_string(),
        format!("-Wl,-rpath,{}", lib_dir.display()),
        "-o".into(),
        exe.display().to_string(),
    ];
    run_direct_args(Path::new("."), &compiler, &args)?;
    run_version(&exe)?;
    println!("Reflection compiler: {}", exe.display());
    Ok(())
}

fn run_version(exe: &Path) -> Result<(), String> {
    run_direct(
        Path::new("."),
        exe.to_str().ok_or("non-utf8 executable path")?,
        &["--version"],
    )
}

fn newer_than_all(output: &Path, inputs: &[&Path]) -> Result<bool, String> {
    if !output.is_file() {
        return Ok(false);
    }
    let output_time = fs::metadata(output)
        .map_err(|error| error.to_string())?
        .modified()
        .map_err(|error| error.to_string())?;
    for input in inputs {
        let input_time = fs::metadata(input)
            .map_err(|error| error.to_string())?
            .modified()
            .map_err(|error| error.to_string())?;
        if output_time < input_time {
            return Ok(false);
        }
    }
    Ok(true)
}

fn find_vcvars64() -> Result<PathBuf, String> {
    let program_files = env::var_os("ProgramFiles(x86)")
        .map(PathBuf::from)
        .ok_or("ProgramFiles(x86) is not set")?;
    let vswhere = program_files.join("Microsoft Visual Studio/Installer/vswhere.exe");
    if !vswhere.is_file() {
        return Err("Use an x64 MSVC developer shell or install the MSVC C++ build tools.".into());
    }
    let output = std::process::Command::new(vswhere)
        .args([
            "-latest",
            "-products",
            "*",
            "-requires",
            "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
            "-property",
            "installationPath",
        ])
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err("An x64 MSVC C++ toolchain is required.".into());
    }
    let install = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let vcvars = PathBuf::from(install).join("VC/Auxiliary/Build/vcvars64.bat");
    if vcvars.is_file() {
        Ok(vcvars)
    } else {
        Err("vcvars64.bat was not found in the Visual Studio installation".into())
    }
}

fn quote_cmd_args(args: &[String]) -> String {
    args.iter()
        .map(|arg| format!("\"{}\"", arg.replace('"', "\\\"")))
        .collect::<Vec<_>>()
        .join(" ")
}
