use std::path::Path;

use crate::{
    process::{CheckRecord, report_check},
    workspace::{project_files, read_text},
};

/// One checked crate: its manifest, name, and the only dependencies it may declare (all
/// kinds: normal, dev, build, and target-specific). D02 keeps this set fixed.
struct CrateRule {
    manifest: &'static str,
    name: &'static str,
    allowed: &'static [&'static str],
}

const CRATE_RULES: &[CrateRule] = &[
    CrateRule {
        manifest: "crates/gust-macros/Cargo.toml",
        name: "gust-macros",
        allowed: &["proc-macro2", "quote", "syn"],
    },
    CrateRule {
        manifest: "crates/gust/Cargo.toml",
        name: "gust",
        allowed: &["gust-macros"],
    },
    CrateRule {
        manifest: "crates/gust-wgpu/Cargo.toml",
        name: "gust-wgpu",
        allowed: &["gust", "pollster", "wgpu"],
    },
    CrateRule {
        manifest: "xtask/Cargo.toml",
        name: "xtask",
        allowed: &[],
    },
];

/// The crates that translate Rust to Slang; they must not depend on a SPIR-V library.
const COMPILER_CRATES: &[&str] = &["gust-macros", "gust"];

const CHANGE_RULE: &str = "D02 keeps the architecture fixed; explain the change in DECISIONS and update xtask/src/arch.rs";

/// `cargo xtask check-arch`: dependency allow-list per crate, no IR modules outside `future/`,
/// no SPIR-V emitter dependency. Needs only git; also part of `check-fast` and CI.
pub(crate) fn check_arch(root: &Path, checks: &mut Vec<CheckRecord>) -> Result<(), String> {
    report_check(checks, "check-arch", check_project(root))
}

/// Returns a summary on success, or one line per problem.
fn check_project(root: &Path) -> Result<String, Vec<String>> {
    let files = project_files(root).map_err(|error| vec![error])?;
    let mut problems = Vec::new();
    for rule in CRATE_RULES {
        match read_text(&root.join(rule.manifest)) {
            Ok(Some(text)) => problems.extend(dependency_problems(rule, &text)),
            _ => problems.push(format!(
                "{}: manifest missing or not readable",
                rule.manifest
            )),
        }
    }
    problems.extend(unlisted_crate_problems(&files));
    let mut rust_files = 0usize;
    for relative in files.iter().filter(|path| path.starts_with("crates/")) {
        problems.extend(ir_path_problems(relative));
        if relative.ends_with(".rs") && !in_future(relative) {
            rust_files += 1;
            match read_text(&root.join(relative)) {
                Ok(Some(text)) => problems.extend(ir_module_problems(relative, &text)),
                Ok(None) => {}
                Err(error) => problems.push(format!("{relative}: could not be read: {error}")),
            }
        }
    }
    if !problems.is_empty() {
        return Err(problems);
    }
    Ok(format!(
        "Architecture check passed: {} crate manifests match the dependency allow-list (example packages are not checked); no IR module or file under crates/ outside future/ ({rust_files} Rust files scanned); no SPIR-V library in {}.\n",
        CRATE_RULES.len(),
        COMPILER_CRATES.join(" or ")
    ))
}

/// Package names declared in any dependency table of a manifest: `[dependencies]`,
/// `[dev-dependencies]`, `[build-dependencies]`, their `target.<cfg>` variants, and the
/// `[dependencies.name]` table form. A `package = "real"` rename counts as `real`.
pub(crate) fn manifest_dependencies(text: &str) -> Vec<String> {
    #[derive(PartialEq)]
    enum Section {
        Other,
        Table,
        Single,
    }
    let mut deps: Vec<String> = Vec::new();
    let mut section = Section::Other;
    for raw in text.lines() {
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if let Some(header) = line.strip_prefix('[') {
            let header = header.trim_end_matches(']').trim();
            section = Section::Other;
            if line.starts_with("[[") || header.starts_with("workspace") {
                continue;
            }
            let segments = header_segments(header);
            if let Some(kind) = segments.iter().position(|s| is_dependency_table(s)) {
                if kind + 1 == segments.len() {
                    section = Section::Table;
                } else if kind + 2 == segments.len() {
                    section = Section::Single;
                    deps.push(segments[kind + 1].clone());
                }
            }
            continue;
        }
        match section {
            Section::Other => {}
            Section::Table => {
                let Some((key, value)) = line.split_once('=') else {
                    continue;
                };
                let name = key.trim().split('.').next().unwrap_or("").trim_matches('"');
                if name.is_empty() {
                    continue;
                }
                // `foo.workspace = true` repeats the name once per dotted key.
                let renamed = if key.trim().ends_with(".package") {
                    package_rename(&format!("{{ package = {value} }}"))
                } else {
                    package_rename(value).filter(|_| !key.contains('.'))
                };
                let name = renamed.unwrap_or_else(|| name.to_owned());
                if !deps.contains(&name) {
                    deps.push(name);
                }
            }
            Section::Single => {
                if let Some(("package", value)) =
                    line.split_once('=').map(|(key, value)| (key.trim(), value))
                    && let Some(real) = package_rename(&format!("{{ package = {value} }}"))
                    && let Some(last) = deps.last_mut()
                {
                    *last = real;
                }
            }
        }
    }
    deps
}

fn is_dependency_table(segment: &str) -> bool {
    matches!(
        segment,
        "dependencies" | "dev-dependencies" | "build-dependencies"
    )
}

/// Header segments split at dots outside quotes: `target.'cfg(unix)'.dependencies.foo`.
fn header_segments(header: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    for c in header.chars() {
        match (c, quote) {
            ('"' | '\'', None) => quote = Some(c),
            (c, Some(open)) if c == open => quote = None,
            ('.', None) => segments.push(std::mem::take(&mut current).trim().to_owned()),
            (c, _) => current.push(c),
        }
    }
    segments.push(current.trim().to_owned());
    segments
}

/// The `package = "name"` inside an inline table value, if any.
fn package_rename(value: &str) -> Option<String> {
    let rest = value.split("package").nth(1)?.trim_start();
    let rest = rest.strip_prefix('=')?.trim_start();
    let quote = rest.chars().next().filter(|c| matches!(c, '"' | '\''))?;
    let inner = &rest[1..];
    Some(inner[..inner.find(quote)?].to_owned())
}

/// The line without a `#` comment (a `#` inside a string is kept).
fn strip_comment(line: &str) -> &str {
    let mut quote = None;
    for (index, c) in line.char_indices() {
        match (c, quote) {
            ('"' | '\'', None) => quote = Some(c),
            (c, Some(open)) if c == open => quote = None,
            ('#', None) => return &line[..index],
            _ => {}
        }
    }
    line
}

fn is_spirv_library(name: &str) -> bool {
    name == "spirv"
        || name.starts_with("rspirv")
        || name.starts_with("spirv-")
        || name.starts_with("spirv_")
}

/// Dependencies outside the crate's allow-list, and SPIR-V libraries in the compiler crates.
fn dependency_problems(rule_crate: &CrateRule, manifest_text: &str) -> Vec<String> {
    let mut problems = Vec::new();
    for dependency in manifest_dependencies(manifest_text) {
        if rule_crate.allowed.contains(&dependency.as_str()) {
            continue;
        }
        if COMPILER_CRATES.contains(&rule_crate.name) && is_spirv_library(&dependency) {
            problems.push(format!(
                "{}: `{}` depends on SPIR-V library `{dependency}`; no SPIR-V emitter in the compiler crates ({CHANGE_RULE})",
                rule_crate.manifest, rule_crate.name
            ));
        } else {
            problems.push(format!(
                "{}: `{}` has dependency `{dependency}`, which is not in its allow-list ({}); {CHANGE_RULE}",
                rule_crate.manifest,
                rule_crate.name,
                if rule_crate.allowed.is_empty() {
                    "none".to_owned()
                } else {
                    rule_crate.allowed.join(", ")
                }
            ));
        }
    }
    problems
}

/// A crate directory under `crates/` that has no rule: its dependencies would go unchecked.
pub(crate) fn unlisted_crate_problems(files: &[String]) -> Vec<String> {
    files
        .iter()
        .filter(|path| {
            path.strip_prefix("crates/")
                .and_then(|rest| rest.strip_suffix("/Cargo.toml"))
                .is_some_and(|name| !name.contains('/'))
        })
        .filter(|path| !CRATE_RULES.iter().any(|rule| rule.manifest == path.as_str()))
        .map(|path| {
            format!("{path}: crate has no dependency allow-list; add one to xtask/src/arch.rs ({CHANGE_RULE})")
        })
        .collect()
}

fn in_future(path: &str) -> bool {
    path.split('/').any(|part| part == "future")
}

/// True for `ir` and `<anything>_ir`, any case.
fn is_ir_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name == "ir" || name.ends_with("_ir")
}

/// A directory or file named `ir`, `*_ir`, or `ir/` under `crates/`, outside `future/`.
pub(crate) fn ir_path_problems(path: &str) -> Vec<String> {
    if !path.starts_with("crates/") || in_future(path) {
        return Vec::new();
    }
    let parts: Vec<&str> = path.split('/').collect();
    let mut problems = Vec::new();
    for (index, part) in parts.iter().enumerate() {
        let name = if index + 1 == parts.len() {
            part.split('.').next().unwrap_or(part)
        } else {
            part
        };
        if is_ir_name(name) {
            problems.push(format!(
                "{path}: `{part}` looks like an instruction IR; D02 allows no custom shader IR outside a future/ path ({CHANGE_RULE})"
            ));
            break;
        }
    }
    problems
}

/// A `mod ir;` or `mod x_ir { .. }` declaration in a Rust file outside `future/`.
pub(crate) fn ir_module_problems(path: &str, text: &str) -> Vec<String> {
    if in_future(path) {
        return Vec::new();
    }
    let mut problems = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let mut rest = line.trim_start();
        if rest.starts_with("//") {
            continue;
        }
        if let Some(after) = rest.strip_prefix("pub") {
            rest = after.trim_start();
            if rest.starts_with('(') {
                rest = rest
                    .split_once(')')
                    .map_or("", |(_, tail)| tail)
                    .trim_start();
            }
        }
        let Some(rest) = rest.strip_prefix("mod ") else {
            continue;
        };
        let name: String = rest
            .trim_start()
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if is_ir_name(&name) {
            problems.push(format!(
                "{path}:{}: module `{name}` looks like an instruction IR; D02 allows no custom shader IR outside a future/ path ({CHANGE_RULE})",
                index + 1
            ));
        }
    }
    problems
}

#[cfg(test)]
#[path = "tests/arch.rs"]
mod arch_tests;
