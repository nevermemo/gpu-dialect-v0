# D19: Sidecar diagnostic maps

## Goal

Emit a deterministic `kernel.map.json` beside compiler-temporary `kernel.slang`
and use it to enrich Slang errors with a stable generated construct label.

## Contract

- The map is derived directly from emitted Slang source; it is not a shader IR.
- Each segment records inclusive Slang line range, kernel name, construct kind,
  and stable ordinal.
- Initial kinds: kernel, local, conditional, loop, match, assignment, return,
  and expression.
- The compiler bridge writes `kernel.map.json` into its exclusively owned
  temporary directory before invoking `slangc`.
- Error mapping selects the segment containing the reported Slang line, then
  falls back to the nearest preceding kernel marker.
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
