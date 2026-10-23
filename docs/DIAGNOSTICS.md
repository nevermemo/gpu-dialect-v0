# GUST diagnostic codes

Every rejection `#[gpu]` reports starts with a stable code:

```text
error: [GUST-E0401] `loop` is not supported: its trip count is not fixed at entry, ...
```

**Codes are stable; messages are not.** A code keeps its meaning and is never reused for
something else. The message text after it may be reworded, so match on the code, not on
the words. Renumbering or retiring a code needs a row in [DECISIONS.md](DECISIONS.md).
Search this page for the code to find what it rejects and what to write instead.

The subset is deliberately small: the macro translates Rust syntax directly to Slang
without an intermediate representation (D02), and rejects what it cannot lower with a
proven meaning rather than guessing. Each entry names the decision or task that
records why. The same code is used wherever the same rule is enforced, whether the
validator or the emitter finds it.

| Family | Area |
| --- | --- |
| E01xx | Module and item shape: inline module, allowed items, structs, signatures, attributes |
| E02xx | Types: unsupported primitives, casts |
| E03xx | Expressions and calls: names, built-ins, paths, bindings |
| E04xx | Control flow and loops |
| E05xx | `Option` and `Result` |
| E06xx | Atomics |
| E07xx | Macro entry and `#[kernel]` options |
| E08xx | Emitter-only rejections |
| E09xx | Reserved names and shadowing |

## E01xx: module and item shape

### GUST-E0101 inline module required

`#[gpu]` needs the module body in the attribute's own item.
Rejected: `#[gpu] mod shaders;`.
Write instead: `#[gpu] mod shaders { ... }`.
Decision: D02.

### GUST-E0102 unsupported module item

A GPU module holds functions and plain structs only.
Rejected: `const`, `static`, `enum`, `impl`, `trait`, `use`, nested `mod`, type aliases.
Write instead: pass constants as uniform or value parameters, or write them inline.
Decision: D02.

### GUST-E0103 generic struct

GPU structs cannot take generic parameters or a `where` clause.
Rejected: `struct Pair<T> { first: T }`.
Write instead: one concrete struct per payload type.
Decision: D02.

### GUST-E0104 struct needs named fields

Rejected: `struct Pair(f32, f32);` and `struct Marker;`.
Write instead: `struct Pair { first: f32, second: f32 }`.
Decision: D17.

### GUST-E0105 empty struct

A GPU struct needs at least one field to have a layout.
Rejected: `struct Empty {}`.
Write instead: give it a field, or remove it.
Decision: D17.

### GUST-E0106 manual repr

GPU structs receive `repr(C)` from the macro.
Rejected: `#[repr(C)] struct Params { ... }`.
Write instead: remove the attribute.
Decision: D17.

### GUST-E0107 manual Clone or Copy

The macro derives `Clone` and `Copy` for GPU structs.
Rejected: `#[derive(Clone, Copy)] struct Params { ... }`.
Write instead: remove those two derives; others such as `Debug` and `PartialEq` are allowed.
Decision: D17.

### GUST-E0108 malformed derive

The argument list of `#[derive(..)]` on a GPU struct does not parse. The message after
the code is `syn`'s own wording.
Rejected: `#[derive = "Debug"]`.
Write instead: `#[derive(Debug)]`.
Decision: D02.

### GUST-E0109 kernel dispatch parameter

A `#[kernel]` function takes exactly one dispatch-thread parameter.
Rejected: `#[kernel] fn run() {}` and a kernel with two `SV_DispatchThreadID` parameters.
Write instead: `#[kernel] fn run(id: SV_DispatchThreadID) { ... }`.
Decision: D02.

### GUST-E0110 kernel return type

Kernels return unit.
Rejected: `#[kernel] fn run(id: SV_DispatchThreadID) -> u32 { ... }`.
Write instead: write results to a `RWStructuredBuffer` parameter.
Decision: D02.

### GUST-E0111 helper resource parameter

Helper functions take value parameters only.
Rejected: `fn helper(b: StructuredBuffer<u32>) -> u32 { ... }`.
Write instead: read the buffer in the kernel and pass the value to the helper.
Decision: D02.

### GUST-E0112 async function

Rejected: `async fn f() {}`. Async is CPU-only.
Write instead: a plain `fn`.
Decision: D02.

### GUST-E0113 unsafe function

Rejected: `unsafe fn f() {}`.
Write instead: a plain `fn`.
Decision: D02.

### GUST-E0114 extern or generic function

Rejected: `extern "C" fn f() {}` and `fn f<T>(x: T) {}`.
Write instead: a plain, non-generic `fn`.
Decision: D02.

### GUST-E0115 unsupported attribute

An attribute with no proven Rust-to-Slang meaning cannot be silently dropped.
Allowed attributes: `doc`, `allow`, `warn`, `deny`, `forbid`, `derive` (on structs),
and `kernel`.
Rejected: `#[cfg(..)]`, `#[inline]`, `#[must_use]` and every other attribute.
Write instead: remove the attribute.
Decision: D02.

## E02xx: types

### GUST-E0201 unsupported primitive type

GPU code uses the four-byte scalars only.
Rejected: `i8`, `i16`, `i64`, `i128`, `isize`, `u8`, `u16`, `u64`, `u128`, `usize`,
`f16`, `f64`, `f128`, `char`, `str`.
Write instead: `f32`/`float`, `i32`/`int`, `u32`/`uint`, `bool`.
Decision: D06.

### GUST-E0202 unsupported cast target

`as` casts are proven only between the 32-bit scalars.
Rejected: `x as f64`, `x as usize`, `x as bool`, `x as Pair`.
Write instead: cast to `f32`/`float`, `i32`/`int` or `u32`/`uint`.
Decision: D22 (numeric semantics).

## E03xx: expressions and calls

### GUST-E0301 CPU-only name

Standard-library and thread names have no GPU meaning.
Rejected: `Vec`, `String`, `Box`, `Rc`, `Arc`, `HashMap`, `HashSet`, `std::..`,
`core::..`, `alloc::..`, `println`, `dbg`, `panic`, `thread`, `spawn`, `File`,
`TcpStream`.
Write instead: fixed-size values and buffers.
Decision: D02.

### GUST-E0302 call to something other than a helper

Only helpers declared in the same GPU module can be called.
Rejected: `foo(x)` where `foo` is outside the module, `(f)(x)`.
Write instead: declare `fn foo` inside the module.
Decision: D02.

### GUST-E0303 type arguments on a built-in

Rejected: `ctx.global_id::<u32>()`.
Write instead: `ctx.global_id()`.
Decision: D02.

### GUST-E0304 arguments on a built-in

`.global_id()` and `.len()` take no arguments.
Rejected: `out.len(1)`.
Write instead: `out.len()`.
Decision: D02.

### GUST-E0305 unsupported method

Methods other than `.global_id()`, `.len()`, `.is_some()`, `.is_none()`, `.is_ok()`,
`.is_err()` and `.unwrap_or(default)` have no lowering.
Rejected: `x.abs()`, `v.iter()`.
Write instead: write the operation with operators and helpers.
Decision: D02, D14.

### GUST-E0306 `.len()` receiver

`.len()` works on a storage parameter named directly.
Rejected: `id.len()`, `buffers.out.len()`.
Write instead: `out.len()` where `out` is a buffer parameter.
Decision: D02.

### GUST-E0307 `.global_id()` receiver

`.global_id()` works on the dispatch-thread parameter named directly.
Rejected: `other.global_id()`.
Write instead: `id.global_id()` where `id` is the kernel's dispatch-thread parameter.
Decision: D02.

### GUST-E0308 associated constant or foreign path

A path used as a value must be one local, parameter or helper name.
Rejected: `f32::INFINITY`, `u32::MAX`, `core::f32::consts::PI`.
Write instead: write the literal, for example `4294967295u32`.
Decision: D06.

### GUST-E0309 const block

Rejected: `const { 1u32 }`.
Write instead: write the value directly.
Decision: D02.

### GUST-E0310 struct update syntax

Rejected: `Pair { first: 9u32, ..old }`.
Write instead: list every field: `Pair { first: 9u32, second: old.second }`.
Decision: D07.

### GUST-E0311 `?` operator

Rejected: `value?`.
Write instead: `if let Some(x) = value { .. }` or `unwrap_or`.
Decision: D14.

### GUST-E0312 unsafe block

Rejected: `unsafe { .. }`.
Write instead: safe code only.
Decision: D02.

### GUST-E0313 macro invocation

Macros cannot run inside a GPU module.
Rejected: `vec![..]`, `println!(..)`, `asm!(..)`.
Write instead: plain expressions.
Decision: D02.

### GUST-E0314 closure

Rejected: `|x| x + 1u32`.
Write instead: a helper function in the module.
Decision: D02.

### GUST-E0315 await

Rejected: `value.await`.
Write instead: GPU code is synchronous; do async work on the host.
Decision: D02.

### GUST-E0316 reference

References are not part of the value model.
Rejected: `&x`, `&mut x`. The one exception is the first argument of an atomic call,
`&mut buffer[index]` (see E06xx).
Write instead: pass and return values.
Decision: D02.

### GUST-E0317 division by a literal zero

Slang makes integer division by a literal zero a compile error while rustc treats it as
a runtime panic, so the boundary rejects it. Float division by zero is allowed.
Rejected: `x / 0u32`, `x % 0`.
Write instead: guard the divisor before dividing.
Decision: D22.

### GUST-E0318 reference or `@` binding

Rejected: `let ref x = v;`, `let x @ _ = v;`, `for ref i in ..`, a `ref` parameter.
Write instead: a plain `let x = v;`.
Decision: D02.

### GUST-E0319 `let` pattern must be one identifier

Rejected: `let (a, b) = pair;`, `let Pair { first, .. } = p;`.
Write instead: `let a = pair.first;`.
Decision: D02.

### GUST-E0320 let-else

Rejected: `let Some(x) = o else { .. };`.
Write instead: `if let Some(x) = o { .. }`.
Decision: D02.

## E04xx: control flow and loops

A GPU thread must always terminate and must never wait on another workgroup, so the
only loop is `for i in start..end` (D18).

### GUST-E0401 `loop`

Rejected: `loop { .. }`.
Write instead: `for i in 0u32..n { .. break; }`.
Decision: D18.

### GUST-E0402 `while`

Rejected: `while cond { .. }`.
Write instead: `for i in 0u32..n { if !cond { break; } .. }`.
Decision: D18.

### GUST-E0403 loop label

Rejected: `'outer: for ..`, `break 'outer;`, `continue 'x;`.
Write instead: a flag variable or a helper function that returns early.
Decision: D18.

### GUST-E0404 `break` or `continue` outside a loop

Rejected: `break;` or `continue;` with no enclosing `for`.
Write instead: remove it, or move it inside the loop.
Decision: D18.

### GUST-E0405 `break` or `continue` as a value

They are statements only.
Rejected: `let v = break;`.
Write instead: `break;` on its own line.
Decision: D18.

### GUST-E0406 `break` with a value

Rejected: `break 3u32;`.
Write instead: assign to a `let mut` local, then `break;`.
Decision: D18.

### GUST-E0407 mutable loop variable

Slang's counter would diverge from Rust's fresh per-iteration binding.
Rejected: `for mut i in 0u32..n { i += 1u32; }`.
Write instead: `for i in 0u32..n { let mut j = i; j += 1u32; }`.
Decision: D18.

### GUST-E0408 loop variable pattern

Rejected: `for (a, b) in 0u32..n`.
Write instead: a single identifier or `_`.
Decision: D18.

### GUST-E0409 loop iterable

Only an exclusive integer range is iterable.
Rejected: `for v in input`, `for v in input.iter()`.
Write instead: `for i in 0u32..input.len() { let v = input[i]; .. }`.
Decision: D18.

### GUST-E0410 inclusive range

When `end` is the type maximum the Slang counter never exceeds it and the loop would
not terminate.
Rejected: `for i in 0u32..=n`.
Write instead: `for i in 0u32..n + 1u32` (mind overflow at the type maximum).
Decision: D18.

### GUST-E0411 open range

The trip count must be fixed at loop entry.
Rejected: `for i in 0u32..`, `for i in ..n`.
Write instead: give both bounds.
Decision: D18.

### GUST-E0412 unsuffixed integer-literal bound

Slang would type a bare literal `int` where rustc may infer `u32`.
Rejected: `for i in 0..n`, `for i in 0u32..(-4)`.
Write instead: `for i in 0u32..n`.
Decision: D18.

### GUST-E0413 range as a value

Ranges exist only as the iterable of a `for`.
Rejected: `let r = 0u32..4u32;`.
Write instead: use the bounds directly.
Decision: D18.

## E05xx: Option and Result

`Option<T>` lowers to Slang `Optional<T>` and `Result<T, T>` to a tagged struct, bounded
to locals and helper signatures (D14).

### GUST-E0501 Option layout

`Option` has no proven storage layout, so it cannot sit in a struct field or a buffer
element type.
Rejected: `struct S { slot: Option<u32> }`, `RWStructuredBuffer<Option<u32>>`.
Write instead: keep `Option` values in locals and helper functions.
Decision: D14, D09.

### GUST-E0502 Option payload

The payload is a single supported scalar or module struct.
Rejected: `Option<Option<u32>>`, `Option<(u32, u32)>`, `Option<Storage<u32>>`.
Write instead: `Option<u32>` or `Option<MyStruct>`.
Decision: D14.

### GUST-E0503 Result layout

As E0501, for `Result`.
Rejected: `struct S { r: Result<u32, u32> }`, `RWStructuredBuffer<Result<u32, u32>>`.
Write instead: keep `Result` values in locals and helper functions.
Decision: D14, D09.

### GUST-E0504 Result shape

Slang cannot infer an absent type parameter, so both payloads must be the same type.
Rejected: `Result<u32, i32>`, `Result<Option<u32>, Option<u32>>`.
Write instead: `Result<u32, u32>`.
Decision: D14.

### GUST-E0505 `Some` arguments

Rejected: `Some(1u32, 2u32)`.
Write instead: `Some(1u32)`.
Decision: D14.

### GUST-E0506 `Ok` or `Err` arguments

Rejected: `Ok()`, `Err(1u32, 2u32)`.
Write instead: `Ok(1u32)`.
Decision: D14.

### GUST-E0507 variant used as a value

Rejected: `let f = Some;`, `Ok` or `Err` without a call.
Write instead: call it with one argument.
Decision: D14.

### GUST-E0508 predicate arguments

Rejected: `o.is_some(1u32)`.
Write instead: `o.is_some()`.
Decision: D14.

### GUST-E0509 `unwrap_or` arguments

Rejected: `o.unwrap_or(1u32, 2u32)`.
Write instead: `o.unwrap_or(1u32)`.
Decision: D14.

### GUST-E0510 `unwrap` and `expect`

GPU code cannot panic.
Rejected: `o.unwrap()`, `r.expect("msg")`.
Write instead: `o.unwrap_or(default)` or `if let Some(x) = o { .. }`.
Decision: D14.

### GUST-E0511 `if let` pattern

Only `Some(identifier)` and `Ok(identifier)` have a lowering.
Rejected: `if let Some(_) = o`, `if let None = o`, `if let Some(1) = o`.
Write instead: `if let Some(x) = o { .. }` or test with `is_none()`.
Decision: D14.

### GUST-E0512 `if let` position

`if let` is the whole condition of an `if`, not part of a larger expression.
Rejected: `if let Some(v) = o && v > 0u32 { .. }`.
Write instead: nest an `if` inside the `if let`.
Decision: D14.

### GUST-E0513 Result match as a value

Rejected: `let x = match r { Ok(v) => { v } Err(e) => { e } };`.
Write instead: assign inside the arms to a `let mut` local, and end the match with `;`.
Decision: D14.

### GUST-E0514 Result match needs a terminating semicolon

Rejected: `match r { .. }` without a `;` in statement position.
Write instead: `match r { .. };`.
Decision: D14.

### GUST-E0515 match scrutinee

Only a known `Result<T, T>` local, parameter or same-module helper call can be matched.
Rejected: `match 1u32 { .. }`, a match on an `Option`.
Write instead: bind the value to a `Result<T, T>` local first, or use `if let` for `Option`.
Decision: D14.

### GUST-E0516 match arm attribute

Rejected: `#[allow(unused)] Ok(x) => { .. }`.
Write instead: remove the attribute.
Decision: D14.

### GUST-E0517 match guard

Rejected: `Ok(x) if x > 0u32 => { .. }`.
Write instead: test inside the arm body.
Decision: D14.

### GUST-E0518 match arm block

Rejected: `Ok(x) => x`.
Write instead: `Ok(x) => { .. }`.
Decision: D14.

### GUST-E0519 match binding

Bindings are plain identifiers.
Rejected: `Ok(mut x)`, `Ok(ref x)`, `Ok(x @ _)`.
Write instead: `Ok(x)` and copy into a `let mut` local when needed.
Decision: D14.

### GUST-E0520 match arm count

Exactly one `Ok` arm and one `Err` arm.
Rejected: a match with only `Ok`, or two `Ok` arms.
Write instead: one arm each.
Decision: D14.

### GUST-E0521 match pattern

Rejected: `_ => { .. }`, `Ok((x, y)) => { .. }`.
Write instead: `Ok(x) => { .. }` and `Err(e) => { .. }`.
Decision: D14.

## E06xx: atomics

Atomics are statement calls on `&mut buffer[index]` of a `RWStructuredBuffer<u32>` or
`RWStructuredBuffer<i32>` (T10, see [the task](development/tasks/T10-atomics.md)).

### GUST-E0601 atomic argument count

`atomic_add`, `atomic_min`, `atomic_max` and `atomic_exchange` take 2 arguments;
`atomic_compare_exchange` takes 3.
Rejected: `atomic_add(&mut c[0])`, `atomic_compare_exchange(&mut c[0], 1u32)`.
Write instead: pass the right number of arguments.
Decision: T10.

### GUST-E0602 atomic first argument shape

The first argument is `&mut buffer[index]` with a buffer named directly.
Rejected: `atomic_add(&c[0], 1u32)`, `atomic_add(&mut c, 1u32)`,
`atomic_add(&mut local, 1u32)`, `atomic_add(&mut s.items[0], 1u32)`.
Write instead: `atomic_add(&mut c[0], 1u32)`.
Decision: T10.

### GUST-E0603 atomic buffer parameter

The buffer must be a read-write buffer parameter.
Rejected: an atomic on a `StructuredBuffer` parameter or an unknown name.
Write instead: declare the parameter as `mut c: RWStructuredBuffer<u32>`.
Decision: T10.

### GUST-E0604 atomic element type

Rejected: an atomic on `RWStructuredBuffer<f32>`.
Write instead: use a `u32` or `i32` buffer.
Decision: T10.

### GUST-E0605 signed `atomic_min` or `atomic_max`

Slang and WGSL support them for unsigned elements only.
Rejected: `atomic_min(&mut c[0], 1i32)`.
Write instead: use a `u32` buffer.
Decision: T10.

### GUST-E0606 atomic as a value

Atomics are statements; the old value is not returned.
Rejected: `let old = atomic_add(&mut c[0], 1u32);`, `if atomic_compare_exchange(..) { .. }`.
Write instead: call it as a statement on its own line.
Decision: T10.

### GUST-E0607 direct access to an atomic buffer

In a kernel that uses a buffer atomically, ordinary reads and writes of it are rejected.
Rejected: `out[id.x] = c[0];` after `atomic_add(&mut c[0], 1u32);`.
Write instead: use a separate non-atomic buffer, or read after the dispatch on the host.
Decision: T10.

## E09xx: reserved names and shadowing

### GUST-E0901 reserved prefix

Names starting with `__gust_` or `__gpu_` are generated shader symbols.
Rejected: `fn __gust_not(x: u32) -> u32`, a parameter named `__gpu_len_out`.
Write instead: any other name.
Decision: D02.

### GUST-E0902 shadowed parameter

A local cannot reuse the name of a resource or dispatch-thread parameter, because the
emitted prologue and length symbols are keyed by it.
Rejected: `let out = 1u32;` when `out` is a buffer parameter, `for id in 0u32..n`.
Write instead: pick a different local name.
Decision: D02.
