use std::{
    env, fs,
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

const EXAMPLES: &[&str] = &[
    "vector-add",
    "polynomial",
    "signal-pipeline",
    "particle-step",
    "typed-pipeline",
    "staged-graph",
    "component-pool",
];
const SMOKE_EXAMPLES: &[&str] = &["vector-add", "typed-pipeline"];

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        usage();
        return Err("missing command".into());
    };
    let args: Vec<_> = args.collect();
    match command.as_str() {
        "build-slang-reflect" => build_slang_reflect(BuildOptions::parse(&args)?)?,
        "verify" => verify(VerifyMode::parse(&args)?)?,
        "check-fast" => verify(VerifyMode::Fast)?,
        "check-full" => verify(VerifyMode::Full)?,
        "check-examples" => verify(VerifyMode::Examples)?,
        "check-artifacts" => verify(VerifyMode::Artifacts)?,
        "check-feature" => check_feature(args.first().map(String::as_str))?,
        "export-artifacts" => export_artifacts()?,
        "measure-tests" => measure_tests(&args)?,
        "hook-format-rust-after-edit" => hook_format_rust_after_edit()?,
        "self-test" => self_test()?,
        "help" | "--help" | "-h" => usage(),
        _ => return Err(format!("unknown xtask command `{command}`")),
    }
    Ok(())
}

fn usage() {
    eprintln!(
        "usage: cargo xtask <command>\n\n  build-slang-reflect [--force] [--sdk-root PATH] [--out-dir PATH]\n  verify [--mode smoke|fast|gpu|examples|artifacts|full] [--full]\n  check-feature <macro|core|wgpu|reflection|loops|examples|artifacts|full>\n  check-fast | check-examples | check-artifacts | check-full\n  export-artifacts\n  measure-tests [command ...]\n  hook-format-rust-after-edit\n  self-test"
    );
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VerifyMode {
    Smoke,
    Fast,
    Gpu,
    Examples,
    Artifacts,
    Full,
}

impl VerifyMode {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut mode = Self::Smoke;
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--full" | "-Full" | "--Full" => mode = Self::Full,
                "--mode" | "-Mode" => {
                    index += 1;
                    let Some(value) = args.get(index) else {
                        return Err("--mode needs a value".into());
                    };
                    mode = Self::from_name(value)?;
                }
                other => mode = Self::from_name(other)?,
            }
            index += 1;
        }
        Ok(mode)
    }

    fn from_name(value: &str) -> Result<Self, String> {
        match value.to_ascii_lowercase().as_str() {
            "smoke" => Ok(Self::Smoke),
            "fast" => Ok(Self::Fast),
            "gpu" => Ok(Self::Gpu),
            "examples" => Ok(Self::Examples),
            "artifacts" => Ok(Self::Artifacts),
            "full" => Ok(Self::Full),
            _ => Err(format!("unknown verify mode `{value}`")),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Smoke => "smoke",
            Self::Fast => "fast",
            Self::Gpu => "gpu",
            Self::Examples => "examples",
            Self::Artifacts => "artifacts",
            Self::Full => "full",
        }
    }
}

#[derive(Default)]
struct BuildOptions {
    force: bool,
    sdk_root: Option<PathBuf>,
    out_dir: Option<PathBuf>,
}

impl BuildOptions {
    fn parse(args: &[String]) -> Result<Self, String> {
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

struct CheckRecord {
    program: String,
    arguments: Vec<String>,
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
}

struct ArtifactRecord {
    file: String,
    sha256: String,
}

fn workspace_root() -> Result<PathBuf, String> {
    let mut path = env::current_dir().map_err(|error| error.to_string())?;
    loop {
        if path.join("Cargo.toml").is_file() && path.join("crates").is_dir() {
            return Ok(path);
        }
        if !path.pop() {
            return Err("could not locate workspace root".into());
        }
    }
}

fn verify(mode: VerifyMode) -> Result<(), String> {
    let root = workspace_root()?;
    let started = now_utc();
    let mut checks = Vec::new();
    let mut artifacts = Vec::new();
    let result = verify_inner(&root, mode, &mut checks, &mut artifacts);
    write_validation(
        &root,
        mode,
        &started,
        result.as_ref().err(),
        &checks,
        &artifacts,
    )?;
    result?;
    println!("GUST verification passed.");
    Ok(())
}

fn verify_inner(
    root: &Path,
    mode: VerifyMode,
    checks: &mut Vec<CheckRecord>,
    artifacts: &mut Vec<ArtifactRecord>,
) -> Result<(), String> {
    require_command("cargo")?;
    require_command("rustc")?;
    require_command("slangc")?;
    if matches!(mode, VerifyMode::Artifacts | VerifyMode::Full) {
        require_command("spirv-val")?;
    }
    run_checked(root, checks, "rustc", &["--version"])?;
    run_checked(root, checks, "slangc", &["-version"])?;
    build_slang_reflect(BuildOptions::default())?;

    if matches!(
        mode,
        VerifyMode::Smoke | VerifyMode::Fast | VerifyMode::Full
    ) {
        run_checked(root, checks, "cargo", &["fmt", "--all", "--", "--check"])?;
    }
    if matches!(mode, VerifyMode::Smoke | VerifyMode::Full) {
        run_checked(
            root,
            checks,
            "cargo",
            &[
                "clippy",
                "--workspace",
                "--all-targets",
                "--",
                "-D",
                "warnings",
            ],
        )?;
    }
    match mode {
        VerifyMode::Fast => {
            run_checked(root, checks, "cargo", &["test", "-p", "gpu-dialect-macros"])?;
            run_checked(
                root,
                checks,
                "cargo",
                &["test", "-p", "gpu-dialect", "--lib"],
            )?;
            run_checked(root, checks, "cargo", &["test", "-p", "gpu-dialect-wgpu"])?;
        }
        VerifyMode::Gpu => run_checked(root, checks, "cargo", &["test", "-p", "gpu-dialect-wgpu"])?,
        VerifyMode::Smoke | VerifyMode::Full => {
            run_checked(root, checks, "cargo", &["test", "--workspace"])?
        }
        VerifyMode::Examples | VerifyMode::Artifacts => {}
    }
    if matches!(mode, VerifyMode::Examples | VerifyMode::Full) {
        for example in EXAMPLES {
            run_checked(
                root,
                checks,
                "cargo",
                &["test", "-p", example, "--", "--ignored"],
            )?;
        }
    }
    let examples_to_run = match mode {
        VerifyMode::Smoke => SMOKE_EXAMPLES,
        VerifyMode::Examples | VerifyMode::Artifacts | VerifyMode::Full => EXAMPLES,
        VerifyMode::Fast | VerifyMode::Gpu => &[],
    };
    for example in examples_to_run {
        run_checked(root, checks, "cargo", &["run", "--quiet", "-p", example])?;
    }
    if matches!(mode, VerifyMode::Artifacts | VerifyMode::Full) {
        run_checked(root, checks, "spirv-val", &["--version"])?;
        let mut spv = fs::read_dir(root.join("generated-wgpu"))
            .map_err(|error| error.to_string())?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "spv"))
            .collect::<Vec<_>>();
        spv.sort();
        if spv.len() != 12 {
            return Err(format!(
                "Expected 12 exported kernels, found {}; update this check deliberately for new examples.",
                spv.len()
            ));
        }
        for artifact in spv {
            let path = artifact.to_string_lossy().into_owned();
            run_checked(
                root,
                checks,
                "spirv-val",
                &["--target-env", "vulkan1.2", &path],
            )?;
            let file = artifact
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("<invalid>")
                .to_owned();
            let bytes = fs::read(&artifact).map_err(|error| error.to_string())?;
            artifacts.push(ArtifactRecord {
                file: file.clone(),
                sha256: sha256_hex(&bytes),
            });
            println!("Validated {file}");
        }
    }
    Ok(())
}

fn check_feature(area: Option<&str>) -> Result<(), String> {
    let root = workspace_root()?;
    let area = area.ok_or("check-feature needs an area")?;
    match area {
        "macro" => run_direct(&root, "cargo", &["test", "-p", "gpu-dialect-macros"]),
        "core" => run_direct(&root, "cargo", &["test", "-p", "gpu-dialect", "--lib"]),
        "wgpu" => verify(VerifyMode::Gpu),
        "reflection" => {
            build_slang_reflect(BuildOptions::default())?;
            run_direct(
                &root,
                "cargo",
                &["test", "-p", "gpu-dialect", "--test", "reflection"],
            )?;
            run_direct(
                &root,
                "cargo",
                &["test", "-p", "gpu-dialect-wgpu", "--test", "reflection"],
            )
        }
        "loops" => {
            run_direct(
                &root,
                "cargo",
                &["test", "-p", "gpu-dialect-macros", "loops"],
            )?;
            run_direct(
                &root,
                "cargo",
                &["test", "-p", "gpu-dialect-wgpu", "--test", "loops"],
            )
        }
        "examples" => verify(VerifyMode::Examples),
        "artifacts" => verify(VerifyMode::Artifacts),
        "full" => verify(VerifyMode::Full),
        _ => Err(format!("unknown feature area `{area}`")),
    }
}

fn export_artifacts() -> Result<(), String> {
    let root = workspace_root()?;
    for example in EXAMPLES {
        println!("Exporting artifacts via {example}");
        run_direct(&root, "cargo", &["run", "--quiet", "-p", example])?;
    }
    Ok(())
}

fn measure_tests(args: &[String]) -> Result<(), String> {
    let root = workspace_root()?;
    let commands = if args.is_empty() {
        vec![
            vec!["cargo", "test", "-p", "gpu-dialect-macros"],
            vec!["cargo", "test", "-p", "gpu-dialect", "--lib"],
            vec!["cargo", "test", "-p", "gpu-dialect-wgpu"],
            vec!["cargo", "test", "--workspace"],
        ]
    } else {
        vec![args.iter().map(String::as_str).collect::<Vec<_>>()]
    };
    for command in commands {
        println!("== {}", command.join(" "));
        let start = Instant::now();
        let status = Command::new(command[0])
            .args(&command[1..])
            .current_dir(&root)
            .status()
            .map_err(|error| format!("could not launch {}: {error}", command[0]))?;
        println!(
            "elapsed_ms={} exit={}",
            start.elapsed().as_millis(),
            status.code().unwrap_or(-1)
        );
        if !status.success() {
            return Err(format!("{} failed", command.join(" ")));
        }
    }
    Ok(())
}

fn hook_format_rust_after_edit() -> Result<(), String> {
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .map_err(|error| error.to_string())?;
    if !input.contains("PostToolUse") || !input.contains(".rs") {
        return Ok(());
    }
    let edit_tool = [
        "create_file",
        "replace_string_in_file",
        "multi_replace_string_in_file",
        "insert_edit_into_file",
        "edit_file",
        "apply_patch",
    ]
    .iter()
    .any(|tool| input.contains(tool));
    if edit_tool {
        run_direct(&workspace_root()?, "cargo", &["fmt", "--all"])?;
    }
    Ok(())
}

fn self_test() -> Result<(), String> {
    require_command("cargo")?;
    require_command("rustc")?;
    require_command("slangc")?;
    let root = workspace_root()?;
    let mut checks = Vec::new();
    run_checked(&root, &mut checks, "rustc", &["--version"])?;
    if checks.len() != 1 || !checks[0].stdout.contains("rustc") {
        return Err("command capture self-test failed".into());
    }
    println!("PASS: xtask command capture and tool lookup work.");
    Ok(())
}

fn build_slang_reflect(options: BuildOptions) -> Result<(), String> {
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

fn run_checked(
    root: &Path,
    checks: &mut Vec<CheckRecord>,
    program: &str,
    args: &[&str],
) -> Result<(), String> {
    let output = Command::new(program)
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|error| format!("could not launch {program}: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    print!("{stdout}");
    eprint!("{stderr}");
    let exit_code = output.status.code();
    checks.push(CheckRecord {
        program: program.to_owned(),
        arguments: args.iter().map(|arg| (*arg).to_owned()).collect(),
        exit_code,
        stdout,
        stderr,
    });
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{} {} failed (exit {})",
            program,
            args.join(" "),
            exit_code.unwrap_or(-1)
        ))
    }
}

fn run_direct(root: &Path, program: &str, args: &[&str]) -> Result<(), String> {
    run_direct_args(
        root,
        program,
        &args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>(),
    )
}

fn run_direct_args(root: &Path, program: &str, args: &[String]) -> Result<(), String> {
    let status = Command::new(program)
        .args(args)
        .current_dir(root)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|error| format!("could not launch {program}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "{} {} failed (exit {})",
            program,
            args.join(" "),
            status.code().unwrap_or(-1)
        ))
    }
}

fn require_command(program: &str) -> Result<(), String> {
    if command_exists(program) {
        Ok(())
    } else {
        Err(format!(
            "required command `{program}` was not found on PATH"
        ))
    }
}

fn command_exists(program: &str) -> bool {
    Command::new(program)
        .arg(if cfg!(windows) { "/?" } else { "--version" })
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
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
    let output = Command::new(vswhere)
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

fn write_validation(
    root: &Path,
    mode: VerifyMode,
    started: &str,
    failure: Option<&String>,
    checks: &[CheckRecord],
    artifacts: &[ArtifactRecord],
) -> Result<(), String> {
    let path = root.join(".ai/VALIDATION.json");
    let mut json = String::new();
    json.push_str("{\n");
    json.push_str("  \"schema_version\": 1,\n");
    json.push_str(&format!("  \"started_utc\": {},\n", json_string(started)));
    json.push_str(&format!(
        "  \"finished_utc\": {},\n",
        json_string(&now_utc())
    ));
    json.push_str(&format!("  \"mode\": {},\n", json_string(mode.as_str())));
    json.push_str(&format!("  \"passed\": {},\n", failure.is_none()));
    match failure {
        Some(error) => json.push_str(&format!("  \"failure\": {},\n", json_string(error))),
        None => json.push_str("  \"failure\": null,\n"),
    }
    json.push_str("  \"scope\": \"Debug native Vulkan wgpu execution of Slang-generated WGSL; SPIR-V export validation is separate.\",\n");
    json.push_str("  \"untested\": [\"browser WebGPU\", \"Metal\", \"DXIL\", \"other GPU vendors\", \"declared minimum Rust version\"],\n");
    json.push_str("  \"checks\": [\n");
    for (index, check) in checks.iter().enumerate() {
        if index != 0 {
            json.push_str(",\n");
        }
        json.push_str("    {\n");
        json.push_str(&format!(
            "      \"program\": {},\n",
            json_string(&check.program)
        ));
        json.push_str("      \"arguments\": [");
        for (arg_index, arg) in check.arguments.iter().enumerate() {
            if arg_index != 0 {
                json.push_str(", ");
            }
            json.push_str(&json_string(arg));
        }
        json.push_str("],\n");
        match check.exit_code {
            Some(code) => json.push_str(&format!("      \"exit_code\": {code},\n")),
            None => json.push_str("      \"exit_code\": null,\n"),
        }
        json.push_str(&format!(
            "      \"stdout\": {},\n",
            json_string(&check.stdout)
        ));
        json.push_str(&format!(
            "      \"stderr\": {}\n",
            json_string(&check.stderr)
        ));
        json.push_str("    }");
    }
    json.push_str("\n  ],\n");
    json.push_str("  \"spirv_artifacts\": [\n");
    for (index, artifact) in artifacts.iter().enumerate() {
        if index != 0 {
            json.push_str(",\n");
        }
        json.push_str("    {\n");
        json.push_str(&format!(
            "      \"file\": {},\n",
            json_string(&artifact.file)
        ));
        json.push_str(&format!(
            "      \"sha256\": {},\n",
            json_string(&artifact.sha256)
        ));
        json.push_str("      \"validation_target\": \"vulkan1.2\",\n");
        json.push_str("      \"status\": \"passed\"\n");
        json.push_str("    }");
    }
    json.push_str("\n  ]\n}\n");
    fs::write(path, json).map_err(|error| error.to_string())
}

fn json_string(value: &str) -> String {
    let mut out = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < ' ' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = sha256(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn now_utc() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() as i64);
    let days = seconds.div_euclid(86_400);
    let day_seconds = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = day_seconds / 3_600;
    let minute = (day_seconds % 3_600) / 60;
    let second = day_seconds % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn civil_from_days(days_since_epoch: i64) -> (i64, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    (year, month as u32, day as u32)
}

// Small SHA-256 implementation for validation records; avoids pulling a dependency into xtask.
fn sha256(input: &[u8]) -> [u8; 32] {
    const H0: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut data = input.to_vec();
    let bit_len = (data.len() as u64) * 8;
    data.push(0x80);
    while data.len() % 64 != 56 {
        data.push(0);
    }
    data.extend_from_slice(&bit_len.to_be_bytes());
    let mut h = H0;
    let (chunks, remainder) = data.as_chunks::<64>();
    debug_assert!(remainder.is_empty());
    for chunk in chunks {
        let mut w = [0u32; 64];
        let (words, remainder) = chunk.as_chunks::<4>();
        debug_assert!(remainder.is_empty());
        for (i, word) in words.iter().take(16).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let mut a = h[0];
        let mut b = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];
        let mut f = h[5];
        let mut g = h[6];
        let mut hh = h[7];
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }
    let mut out = [0u8; 32];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}
