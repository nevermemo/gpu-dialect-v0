use std::{
    path::Path,
    process::{Command, Stdio},
};

pub(crate) struct CheckRecord {
    pub(crate) program: String,
    pub(crate) arguments: Vec<String>,
    pub(crate) exit_code: Option<i32>,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

pub(crate) fn run_checked(
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

pub(crate) fn run_checked_owned(
    root: &Path,
    checks: &mut Vec<CheckRecord>,
    program: &str,
    args: &[String],
) -> Result<(), String> {
    let borrowed = args.iter().map(String::as_str).collect::<Vec<_>>();
    run_checked(root, checks, program, &borrowed)
}

pub(crate) fn run_direct(root: &Path, program: &str, args: &[&str]) -> Result<(), String> {
    run_direct_args(
        root,
        program,
        &args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>(),
    )
}

pub(crate) fn run_direct_args(root: &Path, program: &str, args: &[String]) -> Result<(), String> {
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

pub(crate) fn print_command(root: &Path, program: &str, args: &[&str]) -> Result<(), String> {
    let output = Command::new(program)
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|error| format!("could not launch {program}: {error}"))?;
    print!("{}", String::from_utf8_lossy(&output.stdout));
    eprint!("{}", String::from_utf8_lossy(&output.stderr));
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{} {} failed (exit {})",
            program,
            args.join(" "),
            output.status.code().unwrap_or(-1)
        ))
    }
}

pub(crate) fn require_command(program: &str) -> Result<(), String> {
    if command_exists(program) {
        Ok(())
    } else {
        Err(format!(
            "required command `{program}` was not found on PATH"
        ))
    }
}

pub(crate) fn command_exists(program: &str) -> bool {
    Command::new(program)
        .arg(if cfg!(windows) { "/?" } else { "--version" })
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}
