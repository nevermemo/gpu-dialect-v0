use super::{
    CRATE_RULES, CrateRule, WorkspaceDependency, dependency_problems, ir_module_problems,
    ir_path_problems, manifest_dependencies, unlisted_crate_problems, workspace_dependencies,
    workspace_dependency_problems,
};

fn rule(name: &str) -> &'static CrateRule {
    CRATE_RULES.iter().find(|rule| rule.name == name).unwrap()
}

/// A copy of the shape of the root `[workspace.dependencies]` table.
const ROOT_MANIFEST_TEXT: &str = r#"[workspace]
members = ["crates/gust"]

[workspace.dependencies]
gust = { path = "crates/gust" }
gust-macros = { path = "crates/gust-macros" }
gust-wgpu = { path = "crates/gust-wgpu" }
pollster = "0.4"
proc-macro2 = "1.0"
quote = "1.0"
syn = { version = "2.0", features = ["full", "extra-traits", "visit"] }
wgpu = { version = "30.0.1", default-features = false, features = ["std", "vulkan", "wgsl"] }
"#;

fn root() -> Vec<WorkspaceDependency> {
    workspace_dependencies(ROOT_MANIFEST_TEXT)
}

/// The root table with its `[workspace.dependencies]` replaced by `table`.
fn root_with(table: &str) -> Vec<WorkspaceDependency> {
    workspace_dependencies(&format!(
        "[workspace]\nmembers = []\n\n[workspace.dependencies]\n{table}"
    ))
}

const MACROS_MANIFEST: &str = r#"[package]
name = "gust-macros"
version.workspace = true

[lib]
proc-macro = true

[dependencies]
proc-macro2.workspace = true
quote.workspace = true
syn.workspace = true
"#;

#[test]
fn dependencies_are_read_from_every_table_form() {
    let text = r#"[package]
name = "demo"
description = "dependencies = nope"

[dependencies]
plain = "1"
inline = { version = "2", features = ["x"] }
dotted.workspace = true
dotted.features = ["y"]
"quoted" = "3" # trailing comment
renamed = { package = "real-name", version = "1" }
# commented = "4"

[dev-dependencies]
dev-only = "1"

[build-dependencies.table-form]
version = "1"

[dependencies.alias]
package = "hidden"
version = "1"

[target.'cfg(unix)'.dependencies]
unix-only = "1"

[target.'cfg(windows)'.dependencies.win-table]
version = "1"

[features]
extra = ["plain"]

[workspace.dependencies]
not-counted = "1"
"#;
    assert_eq!(
        manifest_dependencies(text, &[]),
        [
            "plain",
            "inline",
            "dotted",
            "quoted",
            "real-name",
            "dev-only",
            "table-form",
            "hidden",
            "unix-only",
            "win-table",
        ]
    );
}

#[test]
fn the_current_dependency_sets_pass() {
    assert!(dependency_problems(rule("gust-macros"), MACROS_MANIFEST, &root()).is_empty());
    let gust = "[dependencies]\ngust-macros.workspace = true\n";
    assert!(dependency_problems(rule("gust"), gust, &root()).is_empty());
    let wgpu =
        "[dependencies]\ngust.workspace = true\npollster.workspace = true\nwgpu.workspace = true\n";
    assert!(dependency_problems(rule("gust-wgpu"), wgpu, &root()).is_empty());
    assert!(dependency_problems(rule("xtask"), "[package]\nname = \"xtask\"\n", &[]).is_empty());
}

#[test]
fn a_new_dependency_names_d02_and_decisions() {
    let manifest = format!("{MACROS_MANIFEST}serde = \"1\"\n");
    let problems = dependency_problems(rule("gust-macros"), &manifest, &root());
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("`serde`"), "{}", problems[0]);
    assert!(problems[0].contains("D02"), "{}", problems[0]);
    assert!(
        problems[0].contains("explain the change in DECISIONS"),
        "{}",
        problems[0]
    );
    assert!(
        problems[0].starts_with("crates/gust-macros/Cargo.toml: "),
        "{}",
        problems[0]
    );
}

#[test]
fn xtask_may_not_gain_any_dependency() {
    let problems = dependency_problems(rule("xtask"), "[dependencies]\nsha2 = \"0.10\"\n", &[]);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("allow-list (none)"), "{}", problems[0]);
}

#[test]
fn a_dev_dependency_counts_like_any_other() {
    let problems = dependency_problems(rule("gust"), "[dev-dependencies]\ntempfile = \"3\"\n", &[]);
    assert_eq!(problems.len(), 1, "{problems:?}");
}

#[test]
fn spirv_libraries_are_rejected_in_the_compiler_crates_by_name_and_by_rename() {
    for text in [
        "[dependencies]\nrspirv = \"0.12\"\n",
        "[dependencies]\nspirv = \"0.3\"\n",
        "[dependencies]\nspirv-std = \"0.9\"\n",
        "[dependencies]\nrspirv-reflect = \"0.8\"\n",
        "[dependencies]\nfriendly = { package = \"rspirv\", version = \"0.12\" }\n",
        "[dependencies.friendly]\npackage = \"spirv\"\n",
    ] {
        for name in ["gust-macros", "gust"] {
            let problems = dependency_problems(rule(name), text, &[]);
            assert_eq!(problems.len(), 1, "{name}: {text}");
            assert!(problems[0].contains("no SPIR-V emitter"), "{}", problems[0]);
        }
    }
    // The runtime crate gets the generic message; it is not a compiler crate.
    let problems = dependency_problems(rule("gust-wgpu"), "[dependencies]\nrspirv = \"1\"\n", &[]);
    assert_eq!(problems.len(), 1);
    assert!(problems[0].contains("allow-list"), "{}", problems[0]);
}

#[test]
fn a_crate_without_a_rule_is_reported() {
    let files = [
        "crates/gust/Cargo.toml",
        "crates/gust-macros/Cargo.toml",
        "crates/gust-wgpu/Cargo.toml",
        "crates/gust-new/Cargo.toml",
        "crates/gust/tests/Cargo.toml",
        "examples/demo/Cargo.toml",
        "xtask/Cargo.toml",
    ]
    .map(str::to_owned);
    let problems = unlisted_crate_problems(&files);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(
        problems[0].starts_with("crates/gust-new/Cargo.toml: "),
        "{}",
        problems[0]
    );
}

#[test]
fn ir_files_and_directories_are_rejected_outside_future() {
    for path in [
        "crates/gust/src/ir.rs",
        "crates/gust/src/ir/mod.rs",
        "crates/gust-macros/src/shader_ir.rs",
        "crates/gust-macros/src/shader_ir/emit.rs",
        "crates/gust/src/slang/IR.rs",
        "crates/gust/src/slang/ir",
        "crates/gust/src/lower_ir.md",
    ] {
        assert_eq!(ir_path_problems(path).len(), 1, "{path}");
    }
}

#[test]
fn lookalike_paths_and_future_paths_are_allowed() {
    for path in [
        "crates/gust/src/air.rs",
        "crates/gust/src/irq.rs",
        "crates/gust/src/ir_notes.rs",
        "crates/gust/src/first/mod.rs",
        "crates/gust/src/future/ir.rs",
        "crates/gust/future/shader_ir/mod.rs",
        "docs/ir.md",
        "xtask/src/ir.rs",
        "crates/gust/src/slang/mod.rs",
    ] {
        assert!(ir_path_problems(path).is_empty(), "{path}");
    }
}

#[test]
fn ir_module_declarations_are_rejected_outside_future() {
    let text = "use x;\nmod ir;\npub mod shader_ir {\n}\npub(crate) mod IR;\n    mod emit_ir { }\n";
    let problems = ir_module_problems("crates/gust/src/lib.rs", text);
    assert_eq!(problems.len(), 4, "{problems:?}");
    assert!(
        problems[0].starts_with("crates/gust/src/lib.rs:2: "),
        "{}",
        problems[0]
    );
    assert!(problems[1].contains("`shader_ir`"), "{}", problems[1]);
    assert!(problems[3].contains("D02"), "{}", problems[3]);
    assert!(ir_module_problems("crates/gust/src/future/lib.rs", text).is_empty());
}

#[test]
fn other_module_names_and_comments_are_allowed() {
    let text = "mod air;\nmod irq;\nmod ir_notes;\n// mod ir;\n/// mod x_ir;\nlet ir = 1;\nfn ir() {}\nmod modir;\n";
    assert!(ir_module_problems("crates/gust/src/lib.rs", text).is_empty());
}

#[test]
fn the_workspace_table_is_read_in_every_form() {
    let entries = root_with(
        "plain = \"1\"\n\
         inline = { version = \"2\", features = [\"package\", \"git\"] }\n\
         renamed = { package = \"real\", version = \"1\" }\n\
         local = { path = \"crates/local\" }\n\
         remote = { git = \"https://example.com/x.git\" }\n\
         other = { version = \"1\", registry = \"mine\" }\n\
         dotted.path = \"vendor/dotted\"\n\
         dotted.package = \"under\"\n\
         \n\
         [workspace.dependencies.table-form]\n\
         package = \"hidden\"\n\
         version = \"1\"\n",
    );
    let by_key = |key: &str| entries.iter().find(|entry| entry.key == key).unwrap();
    assert_eq!(entries.len(), 8, "{entries:?}");
    assert_eq!(
        by_key("plain"),
        &WorkspaceDependency {
            key: "plain".to_owned(),
            ..WorkspaceDependency::default()
        }
    );
    // A feature named like a field is not the field.
    assert_eq!(by_key("inline").package, None);
    assert!(!by_key("inline").git);
    assert_eq!(by_key("renamed").package.as_deref(), Some("real"));
    assert_eq!(by_key("local").path.as_deref(), Some("crates/local"));
    assert!(by_key("remote").git);
    assert!(by_key("other").registry);
    assert_eq!(by_key("dotted").path.as_deref(), Some("vendor/dotted"));
    assert_eq!(by_key("dotted").package.as_deref(), Some("under"));
    assert_eq!(by_key("table-form").package.as_deref(), Some("hidden"));
}

#[test]
fn the_current_root_table_passes() {
    assert_eq!(workspace_dependency_problems(&root()), Vec::<String>::new());
    // The real root manifest, so that a drift of this copy is noticed.
    let real = workspace_dependencies(include_str!("../../../Cargo.toml"));
    assert_eq!(real, root());
}

#[test]
fn inherited_dependencies_resolve_to_the_root_entry() {
    let workspace = root_with("alias = { package = \"serde\", version = \"1\" }\nsyn = \"2\"\n");
    for manifest in [
        "[dependencies]\nalias.workspace = true\n",
        "[dependencies]\nalias = { workspace = true, features = [\"x\"] }\n",
        "[dependencies.alias]\nworkspace = true\n",
        "[target.'cfg(unix)'.dev-dependencies]\nalias = { workspace = true }\n",
    ] {
        assert_eq!(
            manifest_dependencies(manifest, &workspace),
            ["serde"],
            "{manifest}"
        );
        let problems = dependency_problems(rule("gust-macros"), manifest, &workspace);
        assert_eq!(problems.len(), 1, "{manifest}: {problems:?}");
        assert!(
            problems[0].contains("dependency `serde`"),
            "{}",
            problems[0]
        );
    }
    // Without `workspace = true` the root entry is not consulted.
    assert_eq!(
        manifest_dependencies("[dependencies]\nsyn = \"2\"\nalias = \"1\"\n", &workspace),
        ["syn", "alias"]
    );
}

#[test]
fn a_renamed_workspace_dependency_fails_even_when_it_resolves_to_an_allowed_name() {
    // `syn` inherited from a root entry that is really another package.
    let workspace = root_with("syn = { package = \"evil-syn\", version = \"2\" }\n");
    let problems = dependency_problems(
        rule("gust-macros"),
        "[dependencies]\nsyn.workspace = true\n",
        &workspace,
    );
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("`evil-syn`"), "{}", problems[0]);
    let root_problems = workspace_dependency_problems(&workspace);
    assert_eq!(root_problems.len(), 1, "{root_problems:?}");
    assert!(
        root_problems[0].starts_with("Cargo.toml: "),
        "{}",
        root_problems[0]
    );
    assert!(
        root_problems[0].contains("renamed with package = \"evil-syn\""),
        "{}",
        root_problems[0]
    );
    // The reverse: an innocent key that is really an allowed package passes the crate check
    // but not the root check.
    let workspace = root_with("shim = { package = \"syn\", version = \"2\" }\n");
    assert!(
        dependency_problems(
            rule("gust-macros"),
            "[dependencies]\nshim.workspace = true\n",
            &workspace
        )
        .is_empty()
    );
    assert_eq!(workspace_dependency_problems(&workspace).len(), 1);
}

#[test]
fn an_inherited_dependency_without_a_root_entry_is_reported() {
    let problems = dependency_problems(
        rule("gust-macros"),
        "[dependencies]\nsyn.workspace = true\nmissing.workspace = true\n",
        &root(),
    );
    assert_eq!(problems.len(), 2, "{problems:?}");
    assert!(
        problems[0].contains("no [workspace.dependencies] entry"),
        "{}",
        problems[0]
    );
    assert!(
        problems[1].contains("dependency `missing`"),
        "{}",
        problems[1]
    );
}

#[test]
fn root_entries_with_a_foreign_source_fail() {
    let bad = [
        (
            "syn = { path = \"../syn\", version = \"2\" }\n",
            "path dependency on `../syn`",
        ),
        (
            "syn = { git = \"https://example.com/syn\" }\n",
            "git source",
        ),
        (
            "wgpu = { version = \"30\", registry = \"mine\" }\n",
            "names a registry",
        ),
        (
            "gust = { path = \"vendor/gust\" }\n",
            "must be `path = \"crates/gust\"`, found `vendor/gust`",
        ),
        ("gust-macros = \"0.1\"\n", "found no path"),
        (
            "gust = { path = \"crates/gust-macros\" }\n",
            "found `crates/gust-macros`",
        ),
        (
            "[workspace.dependencies.quote]\ngit = \"https://example.com/quote\"\n",
            "git source",
        ),
    ];
    for (table, expected) in bad {
        let problems = workspace_dependency_problems(&root_with(table));
        assert_eq!(problems.len(), 1, "{table}: {problems:?}");
        assert!(problems[0].contains(expected), "{table}: {}", problems[0]);
    }
    // Spelling variants of the right path are fine; unrelated entries are not checked.
    for table in [
        "gust = { path = \"./crates/gust/\" }\n",
        "gust = { path = \"crates\\gust\" }\n",
        "unrelated = { git = \"https://example.com/x\" }\n",
    ] {
        assert!(
            workspace_dependency_problems(&root_with(table)).is_empty(),
            "{table}"
        );
    }
}
