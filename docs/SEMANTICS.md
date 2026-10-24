# Semantics contract and known limitations

This page states what a `#[gpu]` kernel means on every GUST target, where the targets are allowed to differ, and
how sure each statement is. It is the executable-semantics contract of D22, narrowed to a defined domain by D38 and
confirmed by the owner in D39. The cases outside the domain are written as limitations for users: the owner's
position is that such differences are expected edge cases that developers avoid anyway, so GUST documents them and
does not try to make the targets match.

The CPU twin does not exist yet. Roadmap M3 delivers it in slices m3.b (`gust::cpu`), m3.c (the macro-emitted twin)
and m3.d (executable atomics and the twin column of the ledger). Every statement about the twin below is in the
future tense and marked **planned**: it describes the intended behaviour and is not evidence.

## 1. Scope

- **GPU target (exists).** A `#[gpu]` module is translated directly from the Rust syntax tree to Slang, `slangc`
  compiles the Slang to WGSL, and wgpu runs the WGSL. The measured adapter is NVIDIA GeForce RTX 5090 on Vulkan,
  driver 610.88, slangc 2026.13.1, 2026-10-24. The WGSL specification is the portable reference for this path. The
  contract does not extend to Slang's other targets: Slang itself leaves, for example, integer division by zero
  undefined. The test `the_emitted_wgsl_uses_plain_operators_and_conversions` checks that division, remainder,
  shifts, signed subtraction and multiplication, negation and the float-to-integer casts reach the WGSL as plain
  operators and `i32()` / `u32()` conversions; it does not check `+`, unsigned arithmetic or the float operators.
- **CPU twin (planned).** In a test build (`cfg(test)` or `--cfg gust_dual`, D26) the macro is planned to emit a CPU
  version that runs the rustc-checked body of each kernel, one invocation at a time in ascending `global_id.x`. A
  production build holds one target (D21). The production CPU target belongs to a later milestone (M5) and is not
  described here.
- **Not executed by the twin (planned).** Kernels with a `Uniform<T>` parameter (they have no typed call), indirect
  dispatch and staged graphs stay GPU-only. A kernel can opt out explicitly with `#[kernel(cpu = "unsupported")]`
  (D26). Examples that use graphs will carry kernel-level twin evidence only.
- **Language subset.** 32-bit `f32`, `i32`, `u32`, `bool` and module structs of them (README, "Current subset"). Float
  arithmetic is `+ - * /`, comparisons and casts: the validator accepts no other method than `.len()`,
  `.global_id()` and the `Option` / `Result` helpers, so there is no `sqrt`, `fma` or trigonometry to bound.
  The validator accepts `%` on floats (it rejects only an integer-literal zero as the right operand of `/` and `%`),
  and no bound is stated for it.
- **Independent references stay.** Tests of the translator keep an independent host reference as the expected value
  (D26); the twin is a third party in those tests, never the oracle.

WGSL section numbers and quotes on this page come from the WGSL editor's draft (gpuweb.github.io/gpuweb/wgsl), copy
read on 2026-10-24. A superscript in the spec is written with `^` here.

## 2. Labels

Every statement about GPU behaviour carries one of these, and they are never merged:

- **spec-allowed**: the WGSL specification defines the result or the allowed set, cited by section. Portable across
  adapters that run the WGSL that slangc emits for GUST. The test enforces the GPU value where the spec fixes one.
- **unspecified**: WGSL leaves the outcome open (indeterminate value, implementation-dependent, or a dynamic error;
  for example NaN converted to an integer). The GPU value in the ledger is an observation and not a requirement;
  the test prints a NOTE when it changes (shown with `--nocapture`, not in a default run) and never fails on it.
- **measured**: one adapter, one driver, one date (the adapter in section 1). Not portable.
- **proven**: a test pins the result on both targets and the pinned value lies inside the spec-allowed set. No
  statement has this label yet: the twin column arrives in m3.d, and one matching observation never promotes a line.
  Until then the closest evidence is a GPU run against an independent host reference, which is labelled measured.
- **planned**: the twin will behave this way; nothing runs yet.

"Rust" in the ledger is the plain-Rust result in the crate that compiled the test. "Checks on" and "checks off" mean
the consuming build has `overflow-checks` enabled or not (D38); the test detects the mode at run time and asserts the
matching column. The twin is planned to be compiled in the consumer's crate and so to follow the same mode; GUST pins
no profile.

## 3. The defined domain, and what lies outside it

D22 said integers are bit-exact across targets. D38 narrows that: the guarantee holds on the defined domain below
and nowhere else. Outside it the targets diverge, no diagnostic can prevent it because the operands are run-time
values, and GUST records both outcomes in the ledger (section 7) instead of removing the divergence.

D22 also said that entries of the known-mismatch file fail when an outcome changes. D39 narrows that for outcomes the
WGSL specification leaves open (NaN and infinity, subnormals, NaN conversions, out-of-bounds access), float edge cases
included: they are recorded as one-adapter observations and never fail a test. Outcomes the specification fixes, and
the plain-Rust outcomes, are still pinned and fail the test when they change.

Inside the domain the GPU and the twin (planned) give the same bits for integers, `bool` and control flow, and the
float rules of section 4 apply. The domain is:

1. `+`, `-`, `*` and unary `-` on `u32` and `i32` give a result that fits the type (KM-10).
2. Divisors are not zero, and `i32::MIN / -1` and `i32::MIN % -1` do not occur (KM-01 to KM-03).
3. Shift amounts are below 32 (KM-04). Bits shifted out by an in-range amount, such as `1u32 << 31`, are lost on
   both targets and are inside the domain.
4. `f32 as i32` and `f32 as u32` have a finite operand whose truncated value fits the target type (KM-05, KM-06).
5. Float operands, intermediate values and results are finite, and each is zero or normal (KM-07, KM-08).
6. Every index into a storage buffer is below its `len()`, and no two invocations race (section 5; KM-11).

What happens outside the domain, in one line each (the table has the exact values and the labels):

- Overflow: the GPU wraps modulo 2^32 and never reports it. WGSL section 6.2.3: "Expressions on concrete integer
  types that overflow produce a result that is modulo 2^bitwidth". Rust panics when `overflow-checks` is on and
  wraps when it is off.
- Division and remainder by zero, `MIN / -1`, `MIN % -1`: the GPU returns the dividend, 0, the dividend and 0
  (section 8.8). Rust panics in every profile.
- Shift by 32 or more: the GPU uses the amount modulo 32; section 8.10: "The number of bits to shift is the value of
  e2, modulo the bit width of e1." Rust panics when checks are on and masks when they are off.
- Out-of-range or NaN float-to-integer casts: Rust saturates to the integer's extremes with NaN as 0. The GPU clamps
  to the range that both types represent, so the top end differs (`2147483520`, `4294967040`), and NaN is
  indeterminate (section 15.7.6).
- Out-of-bounds access: the GPU raises a dynamic error and the outcome is open (section 6.5.7); Rust panics.

What users should do: keep run-time operands inside the domain by construction, or guard them in the kernel (test a
divisor before dividing, clamp before casting). A literal zero divisor is rejected at the Rust boundary with a
diagnostic; a run-time zero is not. There are no explicit `wrapping_*` operations in the dialect; they may become a
language feature of their own, not a way to close this gap. Do not rely on the twin to detect overflow on the GPU
path: the GPU wraps silently whatever the twin does.

The ledger test also runs controls inside the domain (integer cases, and float cases on exactly representable values,
so they say nothing about rounding). They must agree between the GPU and plain Rust in both overflow modes; a
disagreement there fails the test, because it would mean the domain is wrong.

## 4. Floats

D22 gives each float operation a documented error bound, because GPU `f32` is not strictly IEEE. The GPU column
below is what WGSL allows; the twin column is planned and is Rust's IEEE 754 binary32 with round-to-nearest-even, and
Rust does not fuse `a * b + c` by itself.

Quotes (WGSL editor's draft, section numbers as read):

- 15.7.4 (accuracy): "the result may be rounded up or down". The table in 15.7.4.1 lists `x + y`, `x - y`, `x * y`
  and `-x` as "Correctly rounded", and `x / y` as "2.5 ULP for |y| in the range [2^-126, 2^126]". Comparisons are
  "Correct result".
- 15.7.5 (fusion and reassociation): "An implementation may fuse operations if the transformed expression is at least
  as accurate as the original formulation." and "An implementation may reassociate operations."
- 15.7.2 (NaN and infinity): "Implementations may assume that overflow, infinities, and NaNs are not present during
  shader execution." A runtime expression that overflows or yields one then gives an indeterminate value (15.7.3),
  which section 8.2 defines as "some arbitrary implementation-chosen value of the static type".
- 15.7.2 (subnormals): inputs or outputs of the operations of 15.7.4 "may be flushed to zero". Other operations "are
  required to preserve subnormal numbers"; the arithmetic of this subset is in the first group. The same section
  lets an implementation ignore the sign of a zero.
- 15.7.6 (conversions): "for non-NaN cases, floating point to integer conversion clamps the value to be within the
  range of the target type, then rounds toward zero." For NaN: "If X is a NaN, the result is an indeterminate value
  in T." For integer to float, when the value is not representable: "WGSL does not specify whether the higher or
  lower representable value is chosen".

Per-operation bounds, with their input domains. "Domain" means finite operands and a finite result, each zero or
normal, unless the row says more. ULP is the unit in the last place of the result (15.7.4).
The last column is a spec-allowed
bound, not an observation.

| Operation | Domain | CPU twin (planned) | GPU, spec-allowed | Twin versus GPU |
| --- | --- | --- | --- | --- |
| `x + y`, `x - y`, `x * y` | domain | Nearest rounding: within 0.5 ULP of the exact result. | "Correctly rounded": either neighbour of the exact result, so within 1 ULP. | At most 1 ULP; identical when the exact result is representable. |
| `-x` | domain | Exact. | Exact ("Correctly rounded"). | Identical, except that the sign of a zero may differ (15.7.2). |
| `x / y` | domain, and `abs(y)` in `[2^-126, 2^126]` | Within 0.5 ULP. | Within 2.5 ULP. | At most 3 ULP. |
| `a * b + c`, any product feeding a sum | domain | Two roundings. | May be fused into one rounding (15.7.5). | No constant: derive it by error propagation (below). |
| a chain of additions or subtractions | domain | Evaluated in the written order. | May be regrouped, with no accuracy condition (15.7.5). | No constant: derive it over the worst grouping (below). |
| `x == y`, `x != y`, `x < y`, `x <= y`, `x > y`, `x >= y` | no NaN operand, no subnormal operand | IEEE 754. | "Correct result". | Identical truth values. |
| `i32 as f32`, `u32 as f32` | any | Nearest, ties to even. | Exact when the value is representable (all `abs(x)` up to 2^24); otherwise either neighbour (15.7.6). | Identical up to 2^24 in magnitude; above it at most 1 ULP, not measured. |
| `f32 as i32`, `f32 as u32` | truncated value fits the target type | Truncates toward zero. | Truncates toward zero (15.7.6). | Identical. Only in-range `f32 as i32` has a test (`numeric`, against a host reference); no test covers `f32 as u32` inside the domain. |
| float `%` | accepted by the validator, not covered | Not stated. | WGSL inherits it from `x - y * trunc(x/y)` (15.7.4.1). | No bound stated. |
| NaN, infinity, subnormal operands or results, out-of-range casts | outside the domain | IEEE 754, kept subnormals, saturating casts. | Indeterminate, flushable, clamped (15.7.2, 15.7.6). | Ledger rows KM-05 to KM-08. |

Nothing in this table has been measured on a GPU except through the ledger rows, and the table is not a claim about
the RTX 5090. Several existing tests compare float results exactly with a host value (for example
`translated_semantics_execute_on_gpu` and two tests in `typed.rs`), but on exactly representable inputs, so no
existing test exercises float rounding. The first such test is planned in m3.c. None of this makes a row above proven.

**Limitation: reassociation.** WGSL 15.7.5 lets an implementation regroup a chain of additions with no accuracy
condition, so the GPU may add in a different order than the source. The twin (planned) adds in the written order.
Two correct programs can therefore differ by more than the per-operation bounds suggest, and a bound must be derived
over the worst grouping.

**Deriving a tolerance for a kernel (planned, slices m3.c and m3.e1).** An expression-level tolerance is derived per
test from these bounds and the test's input domain, and written in the test's doc comment. It is never an observed
maximum. Method: list the kernel's operations in order; mark the exact ones (multiplication by a power of two,
addition of 0, negation); bound each intermediate's magnitude from the input domain; give each inexact operation a
relative error of at most `u = 2^-24` on the twin and at most `2u` on the GPU for `+ - *` (one ULP of a normal
value is at most `2^-23` of its magnitude), and at most `5u` for `/` (2.5 ULP); propagate to first order; for
each chain of additions take the grouping with the largest partial sums on the GPU side (fusion never raises a bound;
reassociation can); the twin-versus-GPU bound is the sum of the two sides.
Observed maxima from measurement runs are recorded separately as observations on the measured adapter and never
become a tolerance. A sign flip near a root that stays inside the derived absolute bound is rounding, not a defect.

**Worked example (arithmetic on the kernel text, not a measurement).** The `evaluate` kernel of
`examples/polynomial` computes `squared = v * v`, `cubed = squared * v`, `curved = cubed - (2.0 * squared)`,
`linear = 0.5 * v` and `(curved + linear) + 1.0`, with inputs `k / 2048 - 1`, so `abs(v)` is at most 1. Multiplying by a
power of two is exact. Twin, in the written order: `squared` u, `cubed` 2u, `2.0 * squared` 2u, `curved` 7u (2u + 2u
plus u times its magnitude bound 3), plus `linear` 10.5u, plus 1.0 15u. GPU, with the worst grouping of the four
terms (magnitudes 1, 2, 0.5, 1): `squared` 2u, `cubed` 4u and `2.0 * squared` 4u from the products, then three
additions whose partial sums have magnitude at most 3, 4 and 4.5, costing 2u each time that magnitude, so 8u +
23u = 31u. The twin-versus-GPU bound is 15u + 31u = 46u, about 2.74e-6 absolute. Slice m3.c (planned) will assert
48u = 48 * 2^-24, about 2.86e-6, where the extra 2u is the margin for second-order terms.

## 5. Memory preconditions

A kernel is inside the contract only if both of these hold. A sequential CPU twin hides violations of the second.

- **In bounds.** Every index into a storage parameter is below its `len()`. A dispatch is rounded up to whole
  workgroups, so a kernel must guard the tail (the fixtures write `if i < out.len()`). An out-of-bounds access on the
  GPU is a dynamic error: WGSL section 6.5.7: "Executing an out-of-bounds access generates a dynamic error." The
  outcome is open: the invocation may trap, the load may return any value of the buffer or zero, the store may be
  dropped or may land in another location of the buffer. Memory accesses stay within the module's variables and the
  bound resources (section 2.2). Rust panics. KM-11 pins the observation (load returned 0, store dropped, with a
  4-element buffer and index 4).
- **Race-free.** Within one dispatch no two invocations write the same location, and no invocation reads a location
  that another writes, except through the atomic functions. On the GPU a race is a dynamic error, which section 2.2
  says "may or may not be detectable"; after a dynamic error the program "may not behave as described in the rest of
  this specification". Parameter alias rejection (D34) is a different guarantee and does not prevent this.
- **Why the twin cannot help (planned).** The twin runs invocations one at a time in ascending `global_id.x`, so it
  produces one definite order. A racy kernel gives a clean twin result that matches a reference computed in the same
  order and proves nothing about the GPU, where the order is unspecified. Every reference in a fixture or example is
  therefore order-independent by construction: each written index has exactly one writer, or the kernel uses
  atomics.
- **Visibility.** The writes of a dispatch are visible after the dispatch completes. Within a dispatch, an invocation
  must not depend on another invocation's non-atomic writes.

## 6. Atomics

- **GPU (exists).** `atomic_add`, `atomic_min`, `atomic_max`, `atomic_exchange` and `atomic_compare_exchange` are
  statement-only calls on `RWStructuredBuffer<u32>` or `<i32>` elements (`i32` has no min or max). WGSL section 17.8:
  "All atomic built-in functions use a relaxed memory ordering." and "No synchronization or ordering guarantees apply
  between atomic and non-atomic memory accesses". Ordering holds only among atomic operations on the same location,
  and GUST offers no stronger ordering. Do not use an atomic to publish non-atomic data to another invocation in the
  same dispatch.
- **The ordering rule.** An atomic whose final result does not depend on the order of the contending invocations
  gives the same result on both targets: `atomic_add` (it wraps modulo 2^32 on both targets by definition, its
  specification body in 17.8.3.1 is `old + v`, and the twin's version is planned to use `wrapping_add`; no test
  drives an atomic add past the limit), `atomic_min` and `atomic_max`. A result that depends on which invocation goes
  first (`atomic_exchange`, a contended `atomic_compare_exchange`) is set-valued on the GPU: any invocation may win. The
  twin (planned) runs invocations in ascending order and so yields one member of that set (KM-09). Tests of such
  kernels must accept the whole set, not the twin's value.
- **Compare-exchange may fail spuriously.** WGSL's only compare-exchange is `atomicCompareExchangeWeak` (17.8.5):
  "The equality comparison may spuriously fail on some implementations." So even an uncontended compare-exchange
  that should succeed is not guaranteed to, and in the contended case the unchanged value (999 in the ledger case)
  is also a spec-allowed result. That the success case succeeds is a measured observation, not a guarantee.
- **Tested today (GPU only, measured).** `crates/gust-wgpu/tests/atomics.rs` checks a contended `atomic_add` sum, the
  contended minimum and maximum, the compare-exchange success and failure cases, and signed add, exchange and
  compare-exchange exactly, and checks that the contended exchange and compare-exchange end in a valid value. The
  returned old value of an atomic is never observable in GPU code (statement-only); the twin is planned to return it
  anyway.
- **Known limitation of that test.** It asserts that the contended compare-exchange ends in 1 to 257, a narrower set
  than the specification allows (it excludes 999), and it asserts the success case as if it were guaranteed. To be
  revisited in m3.d (planned).

## 7. Known-mismatch ledger

The ledger lists the constructs where the GPU and plain Rust (which the CPU target follows) give different results,
or where the GPU's result is unspecified, and that no diagnostic can prevent because the operands are run-time
values. Differences like these are expected and acceptable (D39): they are documented here, not removed.

Each row is pinned by `crates/gust-wgpu/tests/mismatch.rs`, which runs every case on the GPU and as plain Rust; an
id guard in that file keeps this table and the test in step. The column "Rust (and CPU twin)" holds the plain-Rust
result today; the twin assertion (planned, m3.d) must equal it. Labels are defined in section 2. No row is
**proven**.

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
| KM-09 | Contended `atomic_exchange` and `atomic_compare_exchange` (257 invocations, one cell) | Sequential, ascending invocation id: the exchange ends at 256; the compare-exchange from 999 to `id + 1` ends at 1. | Set-valued: the exchange ends at some id in 0 to 256, the compare-exchange at some value in 1 to 257, or at 999 (unchanged) because the weak compare-exchange may fail spuriously (WGSL section 17.8.5); the test accepts 1 to 257 only. Observed over 32 dispatches per run, three runs: 1 to 10 distinct final values per run, and never the sequential result (the exchange ended between 1 and 251, the compare-exchange between 7 and 137). | spec-allowed (the set), measured |
| KM-10 | Overflow of `+`, `-`, `*` and unary `-` on `u32` and `i32` | Checks on: panics ("attempt to add with overflow" and so on). Checks off: wraps modulo 2^32. | Wraps modulo 2^32 (WGSL section 6.2.3): `u32::MAX + 1 == 0`, `i32::MAX + 1 == i32::MIN`, `0u32 - 1 == u32::MAX`, `-i32::MIN == i32::MIN`. | spec-allowed, measured |
| KM-11 | Out-of-bounds index equal to the buffer length | Panics "index out of bounds" in every profile. | Unspecified (a dynamic error, WGSL section 6.5.7). Observed with a 4-element buffer and index 4: the load returned 0 and the store was dropped (the buffer was unchanged). | unspecified, measured |

Reading the table: the Rust column and every GPU value that the specification fixes (labelled spec-allowed) are
enforced, so the test fails when one changes and the row is edited in the same change. A value marked unspecified is
an observation: the test prints a NOTE when it changes and never fails (D39). Adding a construct to the ledger means
adding
its `KM-nn` row here and its case in `crates/gust-wgpu/tests/mismatch.rs`; the id guard fails if only one is done.
