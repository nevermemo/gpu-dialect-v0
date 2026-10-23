use super::{
    CRATE_RULES, CrateRule, dependency_problems, ir_module_problems, ir_path_problems,
    manifest_dependencies, unlisted_crate_problems,
};

fn rule(name: &str) -> &'static CrateRule {
    CRATE_RULES.iter().find(|rule| rule.name == name).unwrap()
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
        manifest_dependencies(text),
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
    assert!(dependency_problems(rule("gust-macros"), MACROS_MANIFEST).is_empty());
    let gust = "[dependencies]\ngust-macros.workspace = true\n";
    assert!(dependency_problems(rule("gust"), gust).is_empty());
    let wgpu =
        "[dependencies]\ngust.workspace = true\npollster.workspace = true\nwgpu.workspace = true\n";
    assert!(dependency_problems(rule("gust-wgpu"), wgpu).is_empty());
    assert!(dependency_problems(rule("xtask"), "[package]\nname = \"xtask\"\n").is_empty());
}

#[test]
fn a_new_dependency_names_d02_and_decisions() {
    let manifest = format!("{MACROS_MANIFEST}serde = \"1\"\n");
    let problems = dependency_problems(rule("gust-macros"), &manifest);
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
    let problems = dependency_problems(rule("xtask"), "[dependencies]\nsha2 = \"0.10\"\n");
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("allow-list (none)"), "{}", problems[0]);
}

#[test]
fn a_dev_dependency_counts_like_any_other() {
    let problems = dependency_problems(rule("gust"), "[dev-dependencies]\ntempfile = \"3\"\n");
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
            let problems = dependency_problems(rule(name), text);
            assert_eq!(problems.len(), 1, "{name}: {text}");
            assert!(problems[0].contains("no SPIR-V emitter"), "{}", problems[0]);
        }
    }
    // The runtime crate gets the generic message; it is not a compiler crate.
    let problems = dependency_problems(rule("gust-wgpu"), "[dependencies]\nrspirv = \"1\"\n");
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
