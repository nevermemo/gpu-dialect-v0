---
name: rust-gpu-ast-validation
description: "Implement or review the restricted Rust GPU dialect validator, module boundaries, syn traversal, rustc versus macro checks, source spans, or rejected-syntax diagnostics. Trigger when accepting or rejecting Rust constructs, not for ordinary host Rust code."
---

# Rust GPU AST validation

Validate the whole GPU module before direct syn AST -> Slang emission. Read current dialect tests and validator first; never infer supported Rust from what syn can parse.

1. Locate the public GPU boundary and write down accepted syntax, types, calls and intrinsics for this change.
2. Use an allowlist with explicit rejection of unknown variants, tokens and modifiers. Recurse into every child, including types, attributes, signatures, patterns and nested expressions.
3. Separate syntactic checks from name/type/alias checks. A proc macro does not receive rustc's typed AST. Ensure rustc checks the intended preserved/generated Rust, or implement the restricted semantic check explicitly.
4. Emit a spanned diagnostic, not a panic or silently omitted shader code. Add a positive fixture and a targeted rejection fixture.

Read [validation contract](references/validation-contract.md) for traversal and semantic pitfalls. For diagnostic fixtures load `compiler-testing`; do not load lowering/runtime references for a validator-only change.
