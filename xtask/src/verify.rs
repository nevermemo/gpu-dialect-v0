use std::{fs, path::Path};

use crate::{
    process::{CheckRecord, require_command, run_checked, run_checked_owned},
    reflect::{BuildOptions, build_slang_reflect},
    time::now_utc,
    validation::{ArtifactRecord, sha256_hex, write_validation},
    workspace::{EXAMPLES, SMOKE_EXAMPLES, workspace_root, workspace_test_args},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VerifyMode {
    Smoke,
    Fast,
    Gpu,
    Examples,
    Artifacts,
    Full,
}

impl VerifyMode {
    pub(crate) fn from_name(value: &str) -> Result<Self, String> {
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

    pub(crate) fn as_str(self) -> &'static str {
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

pub(crate) struct VerifyCommand {
    pub(crate) mode: VerifyMode,
    pub(crate) record: bool,
}

impl VerifyCommand {
    pub(crate) fn parse(args: &[String]) -> Result<Self, String> {
        let mut mode = VerifyMode::Smoke;
        let mut record = None;
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--full" | "-Full" | "--Full" => mode = VerifyMode::Full,
                "--mode" | "-Mode" => {
                    index += 1;
                    let Some(value) = args.get(index) else {
                        return Err("--mode needs a value".into());
                    };
                    mode = VerifyMode::from_name(value)?;
                }
                "--record" => record = Some(true),
                "--no-record" => record = Some(false),
                other => mode = VerifyMode::from_name(other)?,
            }
            index += 1;
        }
        Ok(Self {
            mode,
            record: record.unwrap_or(mode == VerifyMode::Full),
        })
    }
}

pub(crate) fn verify(mode: VerifyMode, record: bool) -> Result<(), String> {
    let root = workspace_root()?;
    let started = now_utc();
    let mut checks = Vec::new();
    let mut artifacts = Vec::new();
    let result = verify_inner(&root, mode, &mut checks, &mut artifacts);
    if record {
        write_validation(
            &root,
            mode,
            &started,
            result.as_ref().err(),
            &checks,
            &artifacts,
        )?;
    }
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
            run_checked_owned(root, checks, "cargo", &workspace_test_args())?
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
