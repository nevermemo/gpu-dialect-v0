# Direct emission rules

These are design constraints for extending the existing dialect, not a claim that every construct is currently supported.

| Rust source | Emission rule / rejection condition |
|---|---|
| `u32`, `i32`, `f32` | Use deliberate target-width mapping (`uint`, `int`, `float`); reject unsupported widths. |
| `usize`, unsuffixed literals | Require a documented restricted typing/range rule; host pointer width is not GPU index width. |
| Binary/unary expressions | Parenthesize structurally or use an explicit precedence table; preserve signed comparisons, short circuit and unary minus. |
| Casts | Check Rust versus Slang behavior, especially float-to-int, truncation and overflow. Reject unsupported semantics. |
| Locals and shadowing | Maintain lexical scopes; mangle collisions and reserved target names deterministically. |
| Function calls | Resolve allowlisted module helpers/intrinsics, arity and types; do not stringify an arbitrary Rust path. |
| Method syntax | Map only known receiver/intrinsic pairs; `.len()` is not an arbitrary shader method. |
| Index read/write | Enforce buffer access, index type and host length contract; write only to writable resources. |
| `if`, blocks, tail expressions | Rust expressions may yield values; emit a compatible value/temporary or reject. Do not lose the final expression or branch. |
| Loops, break/continue, returns | Support only explicitly implemented forms; preserve exits and return values. |

Use a typed emission function per node family returning an error for unsupported nodes. Passing syn tokens through Display/ToTokens and replacing strings is not a lowering algorithm. Preserve evaluation order with temporaries where target rules differ; do not duplicate an expression with side effects. Separate Rust host expansion from Slang text emission.

Keep a source-span association for diagnostics using the project's existing mapping or emitted comments; generated line numbers must not be presented as original Rust locations without a mapping. Keep helper ordering and bindings stable.

For numeric changes test integer boundaries, division/shift edge cases and floats with an explicit NaN/infinity/rounding policy. CPU oracle arithmetic can differ from GPU arithmetic; do not use tolerance to conceal a semantic error. Layout comes from the host/shader ABI, not Rust field spelling.

Source guidance checked 2026-09-07: [Rust expressions](https://doc.rust-lang.org/reference/expressions.html), [Slang compilation](https://github.com/shader-slang/slang/blob/master/docs/user-guide/08-compiling.md). Project-specific mapping rules above must be confirmed against existing tests.
