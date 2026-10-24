# Semantics: known limitations

This page is the known-mismatch ledger of the executable-semantics contract (D22, D38, D39). It lists the
constructs where the GPU and plain Rust (which the CPU target follows) give different results, or where the GPU's
result is unspecified, and that no diagnostic can prevent because the operands are run-time values. Differences
like these are expected and acceptable (D39): they are documented here, not removed. The contract text (scope,
per-operation float bounds, memory preconditions, atomics) is added in slice m3.a2.

Each row is pinned by `crates/gust-wgpu/tests/mismatch.rs`, which runs every case on the GPU and as plain Rust.
Labels:

- **spec-allowed**: the WGSL specification defines the result or the allowed set (section numbers as read in the
  M3 plan; verbatim quotes arrive in m3.a2). Portable across adapters that run the WGSL that slangc emits for GUST (which uses plain `/`, `%`, `<<`, `>>` and `i32()` / `u32()`; checked by `the_emitted_wgsl_uses_plain_operators_and_conversions`), not across Slang's other targets: Slang itself leaves, for example, integer division by zero undefined. The test enforces the GPU value.
- **unspecified**: WGSL leaves the outcome open (indeterminate, implementation-dependent or a dynamic error; for example NaN converted to an integer). The
  GPU value in the table is an observation and not a requirement; the test prints a NOTE when it changes (shown
  with `--nocapture`, not in a default run) and never fails on it.
- **measured**: one adapter, one driver, one date: NVIDIA GeForce RTX 5090, Vulkan, driver 610.88, slangc
  2026.13.1, 2026-10-24. Not portable.
- **proven** (a test pins both targets and the value lies inside the spec-allowed set): no row has this label
  yet. One observation never makes a row proven, and the CPU twin column arrives in m3.d.

"Rust" is the plain-Rust result in the crate that compiled the test. "Checks on" and "checks off" mean the
consuming build has `overflow-checks` enabled or not (D38); the test detects the mode at run time and asserts the
matching column.

| Id | Case | Rust (and CPU twin) | GPU | Label |
| --- | --- | --- | --- | --- |
| KM-01 | `x / 0` for `u32` and `i32` | Panics "attempt to divide by zero" in every profile. | Returns the dividend: `7 / 0 == 7`, `u32::MAX / 0 == u32::MAX`, `-7 / 0 == -7` (WGSL section 8.8). | spec-allowed, measured |
| KM-02 | `x % 0` for `u32` and `i32` | Panics "attempt to calculate the remainder with a divisor of zero" in every profile. | Returns 0 (WGSL section 8.8). | spec-allowed, measured |
| KM-03 | `i32::MIN / -1` and `i32::MIN % -1` | Panic "attempt to divide with overflow" and "attempt to calculate the remainder with overflow" in every profile. | `MIN / -1 == i32::MIN` and `MIN % -1 == 0` (WGSL section 8.8). | spec-allowed, measured |
| KM-04 | Shift by a run-time amount of 32 or more (`<<`, `>>` on `u32` and `i32`) | Checks on: panics "attempt to shift left with overflow" or "attempt to shift right with overflow". Checks off: the amount is masked to its low 5 bits. | The amount is masked to its low 5 bits (WGSL section 8.10): `1 << 32 == 1`, `1 << 33 == 2`, `1 << 64 == 1`, `0x8000_0000 >> 33 == 0x4000_0000`, `-8i32 >> 33 == -4`. | spec-allowed, measured |
| KM-05 | `f32 as i32` of a value outside the `i32` range, and of NaN | Saturates: `1.0e10 -> i32::MAX`, `-1.0e10 -> i32::MIN`, `NaN -> 0`, in every profile. | Finite values: the `i32` value closest to the truncated source that `f32` represents exactly (WGSL section 15.7.6), which differs from Rust at the top end: `1.0e10 -> 2147483520` and `2^31 -> 2147483520` (not `i32::MAX`), `-1.0e10 -> i32::MIN`. NaN: indeterminate; observed `i32::MIN`. | spec-allowed (finite), unspecified (NaN), measured |
| KM-06 | `f32 as u32` of a negative, too large or NaN value | Saturates: negative and NaN give 0, `1.0e10` and `2^32` give `u32::MAX`, in every profile. | Finite values (WGSL section 15.7.6, as KM-05): negatives such as `-1.0`, `-3.99` and `-1.0e10` give 0; `1.0e10` and `2^32` give 4294967040 (`0xffffff00`), not `u32::MAX`. NaN: indeterminate; observed 0. | spec-allowed (finite), unspecified (NaN), measured |
| KM-07 | NaN and infinity through arithmetic and comparison | IEEE 754: NaN propagates, `NaN != NaN` is true, ordered comparisons with NaN are false, `1.0 / 0.0` is `+inf`. | Unspecified (an operation that would produce NaN or infinity may give an indeterminate value; comparisons with NaN are not guaranteed to be unordered). Observed: NaN and infinity propagate and compare as in IEEE for every case except `NaN != NaN`, which is false. The sign and payload of a NaN are not preserved either: the GPU produced `0x7fffffff` in every case, Rust produces `0x7fc00000` or `0xffc00000`. Division by a divisor above 2^126 is also outside WGSL's division accuracy range: `3.0e38 / 3.0e38` gave 0 on the GPU, Rust gives 1. | unspecified, measured |
| KM-08 | Denormal (subnormal) `f32` operands and results | Kept: `1.0e-40 * 1.0` is `1.0e-40`, `1.0e-40 == 0.0` is false. | Unspecified (flushing to zero is allowed). Observed: operands and results flushed to zero (`1.0e-40 * 1.0`, `+ 0.0`, `* 2.0`, `/ 2.0` and `4.0e-38 / 4.0` give 0), and `1.0e-40 == 0.0` is true. | unspecified, measured |
| KM-09 | Contended `atomic_exchange` and `atomic_compare_exchange` (257 invocations, one cell) | Sequential, ascending invocation id: the exchange ends at 256; the compare-exchange from 999 to `id + 1` ends at 1. | Set-valued: the exchange ends at some id in 0 to 256, the compare-exchange at some value in 1 to 257. Observed over 32 dispatches per run, three runs: 1 to 10 distinct final values per run, and never the sequential result (the exchange ended between 1 and 251, the compare-exchange between 7 and 137). | spec-allowed (the set), measured |
| KM-10 | Overflow of `+`, `-`, `*` and unary `-` on `u32` and `i32` | Checks on: panics ("attempt to add with overflow" and so on). Checks off: wraps modulo 2^32. | Wraps modulo 2^32 (WGSL section 6.2.3): `u32::MAX + 1 == 0`, `i32::MAX + 1 == i32::MIN`, `0u32 - 1 == u32::MAX`, `-i32::MIN == i32::MIN`. | spec-allowed, measured |
| KM-11 | Out-of-bounds index equal to the buffer length | Panics "index out of bounds" in every profile. | Unspecified (a dynamic error, WGSL section 6.5.7). Observed with a 4-element buffer and index 4: the load returned 0 and the store was dropped (the buffer was unchanged). | unspecified, measured |
