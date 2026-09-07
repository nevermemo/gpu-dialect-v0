# Validator contract

The source is a deliberately restricted Rust-shaped GPU module. Existing project tests define the current subset. This bundle does not impose an older prototype's feature list.

Attribute procedural macros receive token streams. If the boundary is `#[gpu] mod ...`, an inline module contains items while an outline declaration does not contain another file's contents. Reject outline modules unless the project has an explicit source-loading design. Do not add filesystem probing to macro expansion as an incidental fix.

Keep a small capability table in project documentation: source construct, validation rule, direct emission rule, CPU behavior and tests. Accept a feature only when each relevant column is supported. Start from an allowlist rather than a blacklist of CPU library names.

Inspect all module items and function signatures before bodies. Collect allowed helper names and signatures; resolve scope/shadowing for locals and calls; detect recursion if unsupported. Reject unresolved paths, arbitrary method calls, nested macros and unknown attributes rather than hoping slangc catches them. Validate writable buffer targets, index types and explicit widths. Treat allocation, references, unsafe, async, generics, traits and arbitrary host calls as unsupported unless the dialect explicitly implements their semantics.

syn parses syntax; it does not provide type inference, borrow checking or resolved symbols. rustc checks the Rust remaining after expansion, so deleting an invalid input body can erase the intended diagnostic. A validated source AST plus scoped type/name/binding tables is compatible with direct emission; it is not a new custom instruction IR. If rustc_private integration already exists, honor its pinned nightly; do not introduce it for an ordinary syn change.

Use the locked syn major's APIs. Enable `full` for item/expression trees and `visit` only when used. Recursive visitors alone do not reject unsupported constructs. Default/non-exhaustive and Verbatim cases must reject. On syn versions with modifier containers, reject unrecognized modifiers too. Aggregate independent errors with the pinned syn error API and preserve source spans.

Sources checked 2026-09-07: [Rust procedural macros](https://doc.rust-lang.org/reference/procedural-macros.html), [syn](https://docs.rs/syn/latest/syn/). The live syn docs currently describe 3.x; do not upgrade a 2.x project to copy an example.
