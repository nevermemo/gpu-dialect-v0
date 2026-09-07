---
name: rust-to-slang-lowering
description: "Implement or debug direct validated syn AST to Slang emission, including expressions, control flow, helper functions, intrinsics and binding metadata. Trigger on generated-code semantic mismatches or adding supported dialect constructs; do not introduce a custom IR."
---

# Rust to Slang lowering

Emit Slang directly from the validated syn AST. Retain the module boundary. Small scoped symbol/type/binding tables and an emitter are appropriate; a custom instruction IR is future/optional and requires an explicit architecture request.

1. Read the validator, emitter and closest accepted fixture. State the source-to-target rule before editing.
2. Preserve evaluation order, scope, signedness, width, precedence and control flow. Map only validated intrinsics/calls.
3. Keep shader names, resource bindings and runtime metadata deterministic and generated from one contract.
4. Add a focused source -> Slang assertion, compile affected targets, and execute a nontrivial semantic case when behavior changes.

Read [semantic mappings](references/semantic-mappings.md) for risky translations. Use `slang-language` only for target compilation issues and `gpu-vertical-slice-verification` for an end-to-end claim. Report frontend, shader and runtime evidence separately.
