# DX19: Sidecar diagnostic maps

## Goal

Emit a deterministic `kernel.map.json` beside compiler-temporary `kernel.slang`
and use it to enrich Slang errors with a stable generated construct label.

## Contract

- The map is derived directly from emitted Slang source; it is not a shader IR.
- Each segment records inclusive Slang line range, kernel name, construct kind,
  and stable ordinal.
- Kinds: kernel, prologue, local, conditional, loop, match, assignment, return,
  and expression.
- A construct that opens a block (`if`, `for`, a lowered `match`) is one range
  that ends at its closing brace; `else` and `else if` continue the preceding
  conditional. A `match` marker also ends at the next marker at the same depth
  or the end of the enclosing block. Lowered `match` scaffolding (scrutinee
  temporary, branch test, arm bindings) belongs to the `match` range; user
  statements inside arms get their own nested segments.
- The `kernel` segment covers the marker through the entry point's closing
  brace. The generated resource-length prologue (`__gpu_len_*`,
  `__gpu_stride_*`, `GetDimensions`) is one `prologue` segment per kernel and
  does not consume construct ordinals; construct ordinals restart at 0 for each
  kernel.
- The compiler bridge writes `kernel.map.json` into its exclusively owned
  temporary directory before invoking `slangc`.
- Error mapping selects the innermost segment containing the reported Slang
  line, then falls back to the nearest preceding kernel marker.
- The sidecar is written for inspection and is never parsed back; the lookup
  uses the same map held in memory. "Malformed or missing map" therefore means
  a line with no containing segment, which falls back to kernel attribution.
- The sidecar is temporary diagnostics metadata, not a committed generated
  artifact or runtime ABI field.

## Checks

```sh
cargo test -p gust --lib slang
cargo xtask check-feature core
cargo xtask check-full
```

## Completion

- Unit tests cover map schema, line ranges, construct selection, malformed/no
  map fallback, and existing kernel attribution.
- An intentional compiler failure reports both kernel and construct label.
- Full validation and independent review pass; record tool/adapter limits in
  `STATUS.md`.
