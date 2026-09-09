# Task contracts

Each compiler/runtime task gets one compact contract before implementation. Keep
this document current; move narrative completion reports to `../history/`.
Start new tasks from [TEMPLATE.md](TEMPLATE.md).

## Required contract

- Goal and explicitly supported source forms.
- Explicitly rejected forms and their diagnostics.
- Owning files and smallest focused check.
- Acceptance evidence: validator regression, reviewed Slang golden, WGSL, SPIR-V
  plus `spirv-val`, GPU differential at required boundaries, and independent
  review for non-trivial changes.

## Completion checklist

```text
[ ] Positive syntax and single-cause rejection regressions
[ ] Reviewed source-to-Slang golden
[ ] WGSL compilation
[ ] SPIR-V compilation and external spirv-val
[ ] GPU readback against independent host reference
[ ] Focused xtask feature check
[ ] Full validation when compiler/runtime/artifact behavior changed
[ ] Independent verifier result recorded in STATUS
```
