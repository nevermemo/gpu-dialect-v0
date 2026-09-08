use std::{env, process::ExitCode};

mod checks;
mod process;
mod reflect;
mod time;
mod validation;
mod verify;
mod workspace;

use checks::{
    check_changed, check_feature, check_format, check_lints, check_workspace, doctor,
    explain_check, export_artifacts, hook_format_rust_after_edit, list_tests, measure_tests,
    self_test, status,
};
use reflect::{BuildOptions, build_slang_reflect};
use verify::{VerifyCommand, VerifyMode, verify};
use workspace::FEATURE_AREAS;

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
        "verify" => {
            let command = VerifyCommand::parse(&args)?;
            verify(command.mode, command.record)?;
        }
        "check-fast" => verify(VerifyMode::Fast, false)?,
        "check-full" => verify(VerifyMode::Full, true)?,
        "check-examples" => verify(VerifyMode::Examples, false)?,
        "check-artifacts" => verify(VerifyMode::Artifacts, false)?,
        "check-workspace" => check_workspace()?,
        "check-format" => check_format()?,
        "check-lints" => check_lints()?,
        "check-feature" => check_feature(args.first().map(String::as_str))?,
        "check-changed" => check_changed()?,
        "status" => {
            if !args.is_empty() {
                return Err("status takes no arguments; `status --update` was intentionally not implemented".into());
            }
            status()?;
        }
        "doctor" => doctor()?,
        "list-tests" => list_tests()?,
        "explain-check" => explain_check(args.first().map(String::as_str))?,
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
        "usage: cargo xtask <command>\n\n  build-slang-reflect [--force] [--sdk-root PATH] [--out-dir PATH]\n  verify [--mode smoke|fast|gpu|examples|artifacts|full] [--record|--no-record]\n  check-feature <{}>\n  check-changed | check-workspace | check-format | check-lints\n  check-fast | check-examples | check-artifacts | check-full\n  status | doctor | list-tests | explain-check <area>\n  export-artifacts\n  measure-tests [--json PATH] [command ...]\n  hook-format-rust-after-edit\n  self-test",
        FEATURE_AREAS.join("|")
    );
}
