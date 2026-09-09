use std::{env, path::PathBuf, process::Command};

pub(crate) const EXAMPLES: &[&str] = &[
    "atomic-counter",
    "vector-add",
    "polynomial",
    "signal-pipeline",
    "particle-step",
    "typed-pipeline",
    "staged-graph",
    "component-pool",
];

pub(crate) const SMOKE_EXAMPLES: &[&str] = &["vector-add", "typed-pipeline"];

pub(crate) const WORKSPACE_EXCLUDES: &[&str] = &[
    "atomic-counter",
    "vector-add",
    "polynomial",
    "signal-pipeline",
    "particle-step",
    "typed-pipeline",
    "staged-graph",
    "component-pool",
];

pub(crate) const FEATURE_AREAS: &[&str] = &[
    "macro",
    "core",
    "wgpu",
    "gpu-smoke",
    "gpu-semantics",
    "gpu-runtime",
    "reflection",
    "loops",
    "result",
    "examples",
    "artifacts",
    "full",
];

pub(crate) fn workspace_root() -> Result<PathBuf, String> {
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

pub(crate) fn workspace_test_args() -> Vec<String> {
    let mut args = vec!["test".to_owned(), "--workspace".to_owned()];
    for package in WORKSPACE_EXCLUDES {
        args.push("--exclude".to_owned());
        args.push((*package).to_owned());
    }
    args
}

pub(crate) fn changed_paths() -> Result<Vec<String>, String> {
    let root = workspace_root()?;
    let mut paths = Vec::new();
    for args in [
        ["diff", "--name-only", "HEAD"].as_slice(),
        ["ls-files", "--others", "--exclude-standard"].as_slice(),
    ] {
        let output = Command::new("git")
            .args(args)
            .current_dir(&root)
            .output()
            .map_err(|error| format!("could not launch git: {error}"))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).into_owned());
        }
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let path = line.trim().replace('\\', "/");
            if !path.is_empty() && !paths.contains(&path) {
                paths.push(path);
            }
        }
    }
    paths.sort();
    Ok(paths)
}
