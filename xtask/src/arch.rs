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

const ROOT_MANIFEST: &str = "Cargo.toml";

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
    let workspace = match read_text(&root.join(ROOT_MANIFEST)) {
        Ok(Some(text)) => workspace_dependencies(&text),
        _ => {
            problems.push(format!("{ROOT_MANIFEST}: manifest missing or not readable"));
            Vec::new()
        }
    };
    problems.extend(workspace_dependency_problems(&workspace));
    for rule in CRATE_RULES {
        match read_text(&root.join(rule.manifest)) {
            Ok(Some(text)) => problems.extend(dependency_problems(rule, &text, &workspace)),
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
        "Architecture check passed: {} crate manifests match the dependency allow-list, with `workspace = true` entries resolved through the root [workspace.dependencies] table (example packages are not checked); no IR module or file under crates/ outside future/ ({rust_files} Rust files scanned); no SPIR-V library in {}.\n",
        CRATE_RULES.len(),
        COMPILER_CRATES.join(" or ")
    ))
}

/// Which kind of table the parser is inside.
#[derive(PartialEq)]
enum Section {
    Other,
    /// `[dependencies]`: one dependency per key.
    Table,
    /// `[dependencies.name]`: the keys describe the last dependency.
    Single,
}

/// One dependency as a member manifest declares it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct DeclaredDependency {
    /// The key it is declared under.
    pub(crate) key: String,
    /// The `package = "real"` rename, if the manifest gives one.
    pub(crate) package: Option<String>,
    /// `workspace = true`: the root `[workspace.dependencies]` entry decides what it is.
    pub(crate) inherits: bool,
}

/// One entry of the root `[workspace.dependencies]` table.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct WorkspaceDependency {
    pub(crate) key: String,
    pub(crate) package: Option<String>,
    pub(crate) path: Option<String>,
    pub(crate) git: bool,
    /// An alternative registry (`registry` or `registry-index`).
    pub(crate) registry: bool,
}

impl WorkspaceDependency {
    /// The package name the entry stands for.
    fn package_name(&self) -> &str {
        self.package.as_deref().unwrap_or(&self.key)
    }

    fn set_field(&mut self, field: &str, value: &str) {
        match field {
            "package" => self.package = unquote(value),
            "path" => self.path = unquote(value),
            "git" => self.git = true,
            "registry" | "registry-index" => self.registry = true,
            _ => {}
        }
    }
}

/// Dependencies declared in any dependency table of a manifest: `[dependencies]`,
/// `[dev-dependencies]`, `[build-dependencies]`, their `target.<cfg>` variants, and the
/// `[dependencies.name]` table form.
pub(crate) fn declared_dependencies(text: &str) -> Vec<DeclaredDependency> {
    let mut deps: Vec<DeclaredDependency> = Vec::new();
    let mut section = Section::Other;
    // Index of the first dependency of the current table: a dotted key such as
    // `foo.workspace = true` repeats the name once per key, within one table only.
    let mut table_start = 0;
    for raw in text.lines() {
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if let Some(header) = line.strip_prefix('[') {
            let header = header.trim_end_matches(']').trim();
            section = Section::Other;
            table_start = deps.len();
            if line.starts_with("[[") || header.starts_with("workspace") {
                continue;
            }
            let segments = header_segments(header);
            if let Some(kind) = segments.iter().position(|s| is_dependency_table(s)) {
                if kind + 1 == segments.len() {
                    section = Section::Table;
                } else if kind + 2 == segments.len() {
                    section = Section::Single;
                    deps.push(DeclaredDependency {
                        key: segments[kind + 1].clone(),
                        package: None,
                        inherits: false,
                    });
                }
            }
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match section {
            Section::Other => {}
            Section::Table => {
                let (name, field) = match key.trim().split_once('.') {
                    Some((name, field)) => (name, Some(field.trim())),
                    None => (key.trim(), None),
                };
                let name = name.trim().trim_matches('"');
                if name.is_empty() {
                    continue;
                }
                let at = match deps[table_start..].iter().position(|dep| dep.key == name) {
                    Some(offset) => table_start + offset,
                    None => {
                        deps.push(DeclaredDependency {
                            key: name.to_owned(),
                            package: None,
                            inherits: false,
                        });
                        deps.len() - 1
                    }
                };
                let dep = &mut deps[at];
                match field {
                    Some("package") => dep.package = unquote(value),
                    Some("workspace") => dep.inherits |= value.trim() == "true",
                    Some(_) => {}
                    None => {
                        dep.package = package_rename(value);
                        dep.inherits |=
                            inline_field(value, "workspace").is_some_and(|flag| flag == "true");
                    }
                }
            }
            Section::Single => {
                if let Some(dep) = deps.last_mut() {
                    match key.trim() {
                        "package" => dep.package = unquote(value),
                        "workspace" => dep.inherits |= value.trim() == "true",
                        _ => {}
                    }
                }
            }
        }
    }
    deps
}

/// What a declared dependency is, as a package name. `workspace = true` takes the name from
/// the root entry (its `package` rename counts); without a root entry the key is returned.
fn resolve_dependency(dep: &DeclaredDependency, workspace: &[WorkspaceDependency]) -> String {
    if dep.inherits
        && let Some(entry) = workspace.iter().find(|entry| entry.key == dep.key)
    {
        return entry.package_name().to_owned();
    }
    dep.package.clone().unwrap_or_else(|| dep.key.clone())
}

/// Package names a manifest depends on, with `package = "real"` renames and inherited
/// workspace entries resolved. Each name once, in declaration order.
#[cfg(test)]
pub(crate) fn manifest_dependencies(text: &str, workspace: &[WorkspaceDependency]) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for dep in declared_dependencies(text) {
        let name = resolve_dependency(&dep, workspace);
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// The entries of the `[workspace.dependencies]` table of the root manifest, including the
/// `[workspace.dependencies.name]` table form.
pub(crate) fn workspace_dependencies(text: &str) -> Vec<WorkspaceDependency> {
    let mut deps: Vec<WorkspaceDependency> = Vec::new();
    let mut section = Section::Other;
    for raw in text.lines() {
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if let Some(header) = line.strip_prefix('[') {
            let header = header.trim_end_matches(']').trim();
            section = Section::Other;
            if line.starts_with("[[") {
                continue;
            }
            let segments = header_segments(header);
            if segments.len() >= 2 && segments[0] == "workspace" && segments[1] == "dependencies" {
                match segments.len() {
                    2 => section = Section::Table,
                    3 => {
                        section = Section::Single;
                        deps.push(WorkspaceDependency {
                            key: segments[2].clone(),
                            ..WorkspaceDependency::default()
                        });
                    }
                    _ => {}
                }
            }
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match section {
            Section::Other => {}
            Section::Table => {
                let (name, field) = match key.trim().split_once('.') {
                    Some((name, field)) => (name, Some(field.trim())),
                    None => (key.trim(), None),
                };
                let name = name.trim().trim_matches('"');
                if name.is_empty() {
                    continue;
                }
                let at = match deps.iter().position(|dep| dep.key == name) {
                    Some(at) => at,
                    None => {
                        deps.push(WorkspaceDependency {
                            key: name.to_owned(),
                            ..WorkspaceDependency::default()
                        });
                        deps.len() - 1
                    }
                };
                let dep = &mut deps[at];
                match field {
                    Some(field) => dep.set_field(field, value),
                    None => {
                        for field in ["package", "path", "git", "registry", "registry-index"] {
                            if let Some(inner) = inline_field(value, field) {
                                dep.set_field(field, &inner);
                            }
                        }
                    }
                }
            }
            Section::Single => {
                if let Some(dep) = deps.last_mut() {
                    dep.set_field(key.trim(), value);
                }
            }
        }
    }
    deps
}

/// The value text of `field` in an inline table `{ a = 1, b = "x" }`, as written.
fn inline_field(value: &str, field: &str) -> Option<String> {
    let inner = value.trim().strip_prefix('{')?.strip_suffix('}')?;
    let (mut depth, mut quote, mut start) = (0usize, None, 0);
    let mut parts = Vec::new();
    for (index, c) in inner.char_indices() {
        match (c, quote) {
            ('"' | '\'', None) => quote = Some(c),
            (c, Some(open)) if c == open => quote = None,
            ('[' | '{', None) => depth += 1,
            (']' | '}', None) => depth = depth.saturating_sub(1),
            (',', None) if depth == 0 => {
                parts.push(&inner[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    parts.push(&inner[start..]);
    parts.into_iter().find_map(|part| {
        let (key, value) = part.split_once('=')?;
        (key.trim().trim_matches(['"', '\'']) == field).then(|| value.trim().to_owned())
    })
}

/// The text of a quoted TOML string value; `None` for anything else.
fn unquote(value: &str) -> Option<String> {
    let rest = value.trim();
    let quote = rest.chars().next().filter(|c| matches!(c, '"' | '\''))?;
    let inner = &rest[1..];
    Some(inner[..inner.find(quote)?].to_owned())
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
/// `workspace = true` dependencies are judged by the root `[workspace.dependencies]` entry
/// they resolve to, not by the key they are written under.
fn dependency_problems(
    rule_crate: &CrateRule,
    manifest_text: &str,
    workspace: &[WorkspaceDependency],
) -> Vec<String> {
    let mut problems = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for declared in declared_dependencies(manifest_text) {
        if declared.inherits && !workspace.iter().any(|entry| entry.key == declared.key) {
            problems.push(format!(
                "{}: `{}` inherits `{}` with `workspace = true`, but {ROOT_MANIFEST} has no [workspace.dependencies] entry for it",
                rule_crate.manifest, rule_crate.name, declared.key
            ));
        }
        let dependency = resolve_dependency(&declared, workspace);
        if seen.contains(&dependency) {
            continue;
        }
        seen.push(dependency.clone());
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

/// True when some crate may depend on `name`.
fn is_allowed_somewhere(name: &str) -> bool {
    CRATE_RULES.iter().any(|rule| rule.allowed.contains(&name))
}

/// Root `[workspace.dependencies]` entries that stand for an allow-listed dependency must be
/// the real thing: no `package` rename, no git or alternative registry, a workspace crate
/// from its own `crates/<name>` directory, everything else from the default registry.
pub(crate) fn workspace_dependency_problems(workspace: &[WorkspaceDependency]) -> Vec<String> {
    let mut problems = Vec::new();
    for entry in workspace {
        let name = entry.package_name();
        if !is_allowed_somewhere(&entry.key) && !is_allowed_somewhere(name) {
            continue;
        }
        let at = format!("{ROOT_MANIFEST}: [workspace.dependencies] `{}`", entry.key);
        if let Some(package) = &entry.package {
            problems.push(format!(
                "{at} is renamed with package = \"{package}\"; an allow-listed dependency must keep its own name ({CHANGE_RULE})"
            ));
        }
        if entry.git {
            problems.push(format!(
                "{at} has a git source; allow-listed dependencies come from crates.io or crates/ ({CHANGE_RULE})"
            ));
        }
        if entry.registry {
            problems.push(format!(
                "{at} names a registry; allow-listed dependencies come from crates.io or crates/ ({CHANGE_RULE})"
            ));
        }
        let path = entry.path.as_deref().map(normalized_path);
        if CRATE_RULES.iter().any(|rule| rule.name == name) {
            let expected = format!("crates/{name}");
            if path.as_deref() != Some(expected.as_str()) {
                problems.push(format!(
                    "{at} must be `path = \"{expected}\"`, found {} ({CHANGE_RULE})",
                    path.map_or("no path".to_owned(), |path| format!("`{path}`"))
                ));
            }
        } else if let Some(path) = path {
            problems.push(format!(
                "{at} is a path dependency on `{path}`; it must come from crates.io ({CHANGE_RULE})"
            ));
        }
    }
    problems
}

/// A manifest path with `\`, a leading `./` and trailing `/` normalised away.
fn normalized_path(path: &str) -> String {
    let path = path.replace('\\', "/");
    path.trim_start_matches("./")
        .trim_end_matches('/')
        .to_owned()
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
