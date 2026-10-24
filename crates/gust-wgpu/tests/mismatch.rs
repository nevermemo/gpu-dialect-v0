//! The known-mismatch ledger (roadmap M3, slice m3.a1; D22, D38; `docs/SEMANTICS.md`).
//!
//! Every row pins two outcomes of one construct whose result is not portable and that no
//! diagnostic can prevent, because the operands are run-time values: what the GPU does
//! (measured here, through `HeadlessDevice`) and what plain Rust does (a closure with
//! `black_box` operands under `catch_unwind`, in this test crate's own profile). Rust's
//! expectation is a function of the overflow-checks mode detected at run time; no row is
//! ignored in any profile. The CPU twin is not involved yet (slice m3.d adds its column).
//!
//! `DOMAIN` rows are controls inside the defined domain (results that fit, non-zero
//! divisors, shift amounts below 32, normal finite floats): GPU and Rust must agree on
//! both columns in both modes. A disagreement there is not a ledger row, it means the
//! contract's defined domain is wrong (spike S2 kill rule), and the test fails.
//!
//! Labels (docs/SEMANTICS.md): the GPU values pinned here are *measured* on one adapter
//! and are never promoted to *proven* by this file. Each failing row names its id.

include!("../../../tests/fixtures/mismatch.rs");

use std::{
    any::Any,
    collections::BTreeSet,
    hint::black_box,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::OnceLock,
};

use gust::{GpuPod, KernelDescriptor};
use gust_wgpu::{BufferBinding, GpuBufferAccess, HeadlessDevice};

// ---- overflow-checks detection (private copy until `gust::cpu::overflow_checks`, m3.b/m3.d) ----

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OverflowChecks {
    Checked,
    Wrapping,
}

impl OverflowChecks {
    fn pick(self, checked: Outcome, wrapping: Outcome) -> Outcome {
        match self {
            Self::Checked => checked,
            Self::Wrapping => wrapping,
        }
    }
}

/// `Checked` when `u32::MAX + 1` panics in this crate's build. In a checked build the first
/// call prints one panic message to stderr (the default hook); the ledger test calls it first.
fn overflow_checks() -> OverflowChecks {
    static MODE: OnceLock<OverflowChecks> = OnceLock::new();
    *MODE.get_or_init(|| {
        if catch_unwind(|| black_box(u32::MAX) + black_box(1)).is_err() {
            OverflowChecks::Checked
        } else {
            OverflowChecks::Wrapping
        }
    })
}

// ---- outcomes ----

/// A pinned outcome. Values are 32-bit patterns (`i32` and `f32` as their bits).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Outcome {
    Value(u32),
    /// Any NaN bit pattern (the sign and payload of a NaN are not part of the contract).
    Nan,
    /// A panic whose message starts with this text.
    Panic(&'static str),
    /// Set-valued GPU outcomes (KM-09): every observation lies in the inclusive range.
    Within(u32, u32),
}

/// What one side actually did.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Observed {
    Value(u32),
    Panic(String),
}

impl Outcome {
    fn accepts(self, observed: &Observed) -> bool {
        match (self, observed) {
            (Self::Value(want), Observed::Value(got)) => want == *got,
            (Self::Nan, Observed::Value(got)) => f32::from_bits(*got).is_nan(),
            (Self::Panic(prefix), Observed::Panic(message)) => message.starts_with(prefix),
            (Self::Within(low, high), Observed::Value(got)) => (low..=high).contains(got),
            _ => false,
        }
    }
}

fn show(observed: &Observed) -> String {
    match observed {
        Observed::Value(bits) => format!("0x{bits:08x}"),
        Observed::Panic(message) => format!("panic({message})"),
    }
}

fn panic_message(payload: &(dyn Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_owned()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "<non-string panic payload>".to_owned()
    }
}

// ---- cases ----

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    UDiv,
    IDiv,
    UShift,
    IShift,
    UArith,
    IArith,
    CastInt,
    CastUint,
    FloatOp,
    FloatCmp,
    OobLoad,
    OobStoreLast,
    OobStoreElsewhere,
    ContendedExchange,
    ContendedCompareExchange,
}

struct Case {
    /// `KM-nn`, or `DOMAIN` for a control inside the defined domain.
    id: &'static str,
    what: &'static str,
    kind: Kind,
    /// Operation selector, first operand, second operand (bit patterns).
    operands: (u32, u32, u32),
    gpu: Outcome,
    rust: fn(OverflowChecks) -> Outcome,
    /// Whether the pinned GPU value is a requirement the test enforces (see `gpu_is_enforced`).
    enforced: bool,
}

fn case(
    id: &'static str,
    what: &'static str,
    kind: Kind,
    operands: (u32, u32, u32),
    gpu: Outcome,
    rust: fn(OverflowChecks) -> Outcome,
) -> Case {
    Case {
        id,
        what,
        kind,
        operands,
        gpu,
        rust,
        enforced: gpu_is_enforced(id, kind, operands.1),
    }
}

const fn i(value: i32) -> u32 {
    value as u32
}

const fn f(value: f32) -> u32 {
    value.to_bits()
}

const fn v(bits: u32) -> Outcome {
    Outcome::Value(bits)
}

const fn p(prefix: &'static str) -> Outcome {
    Outcome::Panic(prefix)
}

const DIV_ZERO: &str = "attempt to divide by zero";
const REM_ZERO: &str = "attempt to calculate the remainder with a divisor of zero";
const DIV_OVERFLOW: &str = "attempt to divide with overflow";
const REM_OVERFLOW: &str = "attempt to calculate the remainder with overflow";
const SHL_OVERFLOW: &str = "attempt to shift left with overflow";
const SHR_OVERFLOW: &str = "attempt to shift right with overflow";
const ADD_OVERFLOW: &str = "attempt to add with overflow";
const SUB_OVERFLOW: &str = "attempt to subtract with overflow";
const MUL_OVERFLOW: &str = "attempt to multiply with overflow";
const NEG_OVERFLOW: &str = "attempt to negate with overflow";
const OOB: &str = "index out of bounds";

const NAN: u32 = f32::NAN.to_bits();
const INF: u32 = f32::INFINITY.to_bits();
const NEG_INF: u32 = f32::NEG_INFINITY.to_bits();
const DENORMAL: f32 = 1.0e-40;

/// Number of invocations that contend in the KM-09 kernels.
const CONTENDERS: u32 = 257;
/// How many times each contended kernel is dispatched to sample the GPU's order.
const CONTENDED_RUNS: usize = 32;

fn cases() -> Vec<Case> {
    use Kind::*;
    // Both modes expect the same outcome.
    let mut rows = Vec::new();
    let mut add = |row: Case| rows.push(row);

    // ---- DOMAIN controls (integers) ----
    add(case("DOMAIN", "u32 7 / 2", UDiv, (0, 7, 2), v(3), |_| v(3)));
    add(case("DOMAIN", "u32 7 % 2", UDiv, (1, 7, 2), v(1), |_| v(1)));
    add(case(
        "DOMAIN",
        "i32 -7 / 2",
        IDiv,
        (0, i(-7), 2),
        v(i(-3)),
        |_| v(i(-3)),
    ));
    add(case(
        "DOMAIN",
        "i32 -7 % 2",
        IDiv,
        (1, i(-7), 2),
        v(i(-1)),
        |_| v(i(-1)),
    ));
    add(case(
        "DOMAIN",
        "i32 MIN / 1",
        IDiv,
        (0, i(i32::MIN), 1),
        v(i(i32::MIN)),
        |_| v(i(i32::MIN)),
    ));
    add(case(
        "DOMAIN",
        "u32 1 << 31",
        UShift,
        (0, 1, 31),
        v(0x8000_0000),
        |_| v(0x8000_0000),
    ));
    add(case(
        "DOMAIN",
        "u32 0x8000_0000 >> 31",
        UShift,
        (1, 0x8000_0000, 31),
        v(1),
        |_| v(1),
    ));
    add(case(
        "DOMAIN",
        "i32 1 << 30",
        IShift,
        (0, 1, 30),
        v(0x4000_0000),
        |_| v(0x4000_0000),
    ));
    add(case(
        "DOMAIN",
        "i32 -8 >> 31",
        IShift,
        (1, i(-8), 31),
        v(i(-1)),
        |_| v(i(-1)),
    ));
    add(case(
        "DOMAIN",
        "i32 -8 >> 1",
        IShift,
        (1, i(-8), 1),
        v(i(-4)),
        |_| v(i(-4)),
    ));
    add(case("DOMAIN", "u32 2 + 3", UArith, (0, 2, 3), v(5), |_| {
        v(5)
    }));
    add(case("DOMAIN", "u32 5 - 3", UArith, (1, 5, 3), v(2), |_| {
        v(2)
    }));
    add(case(
        "DOMAIN",
        "u32 6 * 7",
        UArith,
        (2, 6, 7),
        v(42),
        |_| v(42),
    ));
    add(case(
        "DOMAIN",
        "i32 -5 + 3",
        IArith,
        (0, i(-5), 3),
        v(i(-2)),
        |_| v(i(-2)),
    ));
    add(case(
        "DOMAIN",
        "i32 3 - 5",
        IArith,
        (1, 3, 5),
        v(i(-2)),
        |_| v(i(-2)),
    ));
    add(case(
        "DOMAIN",
        "i32 -6 * 7",
        IArith,
        (2, i(-6), 7),
        v(i(-42)),
        |_| v(i(-42)),
    ));
    add(case(
        "DOMAIN",
        "i32 -(5)",
        IArith,
        (3, 5, 0),
        v(i(-5)),
        |_| v(i(-5)),
    ));
    add(case(
        "DOMAIN",
        "f32 3.99 as i32",
        CastInt,
        (0, f(3.99), 0),
        v(3),
        |_| v(3),
    ));
    add(case(
        "DOMAIN",
        "f32 -3.99 as i32",
        CastInt,
        (0, f(-3.99), 0),
        v(i(-3)),
        |_| v(i(-3)),
    ));
    add(case(
        "DOMAIN",
        "f32 3.99 as u32",
        CastUint,
        (0, f(3.99), 0),
        v(3),
        |_| v(3),
    ));
    add(case(
        "DOMAIN",
        "f32 0.0 as u32",
        CastUint,
        (0, f(0.0), 0),
        v(0),
        |_| v(0),
    ));

    add(case(
        "DOMAIN",
        "f32 -1.0 as i32",
        CastInt,
        (0, f(-1.0), 0),
        v(i(-1)),
        |_| v(i(-1)),
    ));
    // ---- KM-01 division by zero ----
    add(case("KM-01", "u32 7 / 0", UDiv, (0, 7, 0), v(7), |_| {
        p(DIV_ZERO)
    }));
    add(case(
        "KM-01",
        "u32 MAX / 0",
        UDiv,
        (0, u32::MAX, 0),
        v(u32::MAX),
        |_| p(DIV_ZERO),
    ));
    add(case(
        "KM-01",
        "i32 -7 / 0",
        IDiv,
        (0, i(-7), 0),
        v(i(-7)),
        |_| p(DIV_ZERO),
    ));
    // ---- KM-02 remainder by zero ----
    add(case("KM-02", "u32 7 % 0", UDiv, (1, 7, 0), v(0), |_| {
        p(REM_ZERO)
    }));
    add(case(
        "KM-02",
        "i32 -7 % 0",
        IDiv,
        (1, i(-7), 0),
        v(0),
        |_| p(REM_ZERO),
    ));
    // ---- KM-03 MIN / -1 and MIN % -1 ----
    add(case(
        "KM-03",
        "i32 MIN / -1",
        IDiv,
        (0, i(i32::MIN), i(-1)),
        v(i(i32::MIN)),
        |_| p(DIV_OVERFLOW),
    ));
    add(case(
        "KM-03",
        "i32 MIN % -1",
        IDiv,
        (1, i(i32::MIN), i(-1)),
        v(0),
        |_| p(REM_OVERFLOW),
    ));

    // ---- KM-04 shift amounts of 32 or more ----
    add(case(
        "KM-04",
        "u32 1 << 32",
        UShift,
        (0, 1, 32),
        v(1),
        |m| m.pick(p(SHL_OVERFLOW), v(1)),
    ));
    add(case(
        "KM-04",
        "u32 1 << 33",
        UShift,
        (0, 1, 33),
        v(2),
        |m| m.pick(p(SHL_OVERFLOW), v(2)),
    ));
    add(case(
        "KM-04",
        "u32 1 << 64",
        UShift,
        (0, 1, 64),
        v(1),
        |m| m.pick(p(SHL_OVERFLOW), v(1)),
    ));
    add(case(
        "KM-04",
        "u32 0x8000_0000 >> 32",
        UShift,
        (1, 0x8000_0000, 32),
        v(0x8000_0000),
        |m| m.pick(p(SHR_OVERFLOW), v(0x8000_0000)),
    ));
    add(case(
        "KM-04",
        "u32 0x8000_0000 >> 33",
        UShift,
        (1, 0x8000_0000, 33),
        v(0x4000_0000),
        |m| m.pick(p(SHR_OVERFLOW), v(0x4000_0000)),
    ));
    add(case(
        "KM-04",
        "u32 0x8000_0000 >> 64",
        UShift,
        (1, 0x8000_0000, 64),
        v(0x8000_0000),
        |m| m.pick(p(SHR_OVERFLOW), v(0x8000_0000)),
    ));
    add(case(
        "KM-04",
        "i32 1 << 32",
        IShift,
        (0, 1, 32),
        v(1),
        |m| m.pick(p(SHL_OVERFLOW), v(1)),
    ));
    add(case(
        "KM-04",
        "i32 -8 >> 32",
        IShift,
        (1, i(-8), 32),
        v(i(-8)),
        |m| m.pick(p(SHR_OVERFLOW), v(i(-8))),
    ));
    add(case(
        "KM-04",
        "i32 -8 >> 33",
        IShift,
        (1, i(-8), 33),
        v(i(-4)),
        |m| m.pick(p(SHR_OVERFLOW), v(i(-4))),
    ));
    add(case(
        "KM-04",
        "i32 -8 >> 64",
        IShift,
        (1, i(-8), 64),
        v(i(-8)),
        |m| m.pick(p(SHR_OVERFLOW), v(i(-8))),
    ));

    // ---- KM-05 f32 as i32 out of range, NaN ----
    add(case(
        "KM-05",
        "f32 1.0e10 as i32",
        CastInt,
        (0, f(1.0e10), 0),
        v(0x7fff_ff80),
        |_| v(i(i32::MAX)),
    ));
    add(case(
        "KM-05",
        "f32 -1.0e10 as i32",
        CastInt,
        (0, f(-1.0e10), 0),
        v(i(i32::MIN)),
        |_| v(i(i32::MIN)),
    ));
    add(case(
        "KM-05",
        "f32 NaN as i32",
        CastInt,
        (0, NAN, 0),
        v(0x8000_0000),
        |_| v(0),
    ));
    add(case(
        "KM-05",
        "f32 2147483648.0 (2^31) as i32",
        CastInt,
        (0, f(2_147_483_648.0), 0),
        v(0x7fff_ff80),
        |_| v(i(i32::MAX)),
    ));
    // ---- KM-06 f32 as u32 of a negative, NaN or too large value ----
    add(case(
        "KM-06",
        "f32 -1.0 as u32",
        CastUint,
        (0, f(-1.0), 0),
        v(0),
        |_| v(0),
    ));
    add(case(
        "KM-06",
        "f32 -3.99 as u32",
        CastUint,
        (0, f(-3.99), 0),
        v(0),
        |_| v(0),
    ));
    add(case(
        "KM-06",
        "f32 NaN as u32",
        CastUint,
        (0, NAN, 0),
        v(0),
        |_| v(0),
    ));
    add(case(
        "KM-06",
        "f32 1.0e10 as u32",
        CastUint,
        (0, f(1.0e10), 0),
        v(0xffff_ff00),
        |_| v(u32::MAX),
    ));
    add(case(
        "KM-06",
        "f32 -1.0e10 as u32",
        CastUint,
        (0, f(-1.0e10), 0),
        v(0),
        |_| v(0),
    ));
    add(case(
        "KM-06",
        "f32 4294967296.0 (2^32) as u32",
        CastUint,
        (0, f(4_294_967_296.0), 0),
        v(0xffff_ff00),
        |_| v(u32::MAX),
    ));

    // ---- KM-10 overflow of + - * and negation ----
    add(case(
        "KM-10",
        "u32 MAX + 1",
        UArith,
        (0, u32::MAX, 1),
        v(0),
        |m| m.pick(p(ADD_OVERFLOW), v(0)),
    ));
    add(case(
        "KM-10",
        "i32 MAX + 1",
        IArith,
        (0, i(i32::MAX), 1),
        v(i(i32::MIN)),
        |m| m.pick(p(ADD_OVERFLOW), v(i(i32::MIN))),
    ));
    add(case(
        "KM-10",
        "u32 0 - 1",
        UArith,
        (1, 0, 1),
        v(u32::MAX),
        |m| m.pick(p(SUB_OVERFLOW), v(u32::MAX)),
    ));
    add(case(
        "KM-10",
        "i32 MIN - 1",
        IArith,
        (1, i(i32::MIN), 1),
        v(i(i32::MAX)),
        |m| m.pick(p(SUB_OVERFLOW), v(i(i32::MAX))),
    ));
    add(case(
        "KM-10",
        "u32 MAX * 2",
        UArith,
        (2, u32::MAX, 2),
        v(u32::MAX - 1),
        |m| m.pick(p(MUL_OVERFLOW), v(u32::MAX - 1)),
    ));
    add(case(
        "KM-10",
        "i32 MAX * 2",
        IArith,
        (2, i(i32::MAX), 2),
        v(i(-2)),
        |m| m.pick(p(MUL_OVERFLOW), v(i(-2))),
    ));
    add(case(
        "KM-10",
        "-(i32 MIN)",
        IArith,
        (3, i(i32::MIN), 0),
        v(i(i32::MIN)),
        |m| m.pick(p(NEG_OVERFLOW), v(i(i32::MIN))),
    ));

    // ---- DOMAIN controls (floats): exactly representable values, normal results ----
    add(case(
        "DOMAIN",
        "f32 1.5 + 2.25",
        FloatOp,
        (0, f(1.5), f(2.25)),
        v(f(3.75)),
        |_| v(f(3.75)),
    ));
    add(case(
        "DOMAIN",
        "f32 1.0 - 2.0",
        FloatOp,
        (1, f(1.0), f(2.0)),
        v(f(-1.0)),
        |_| v(f(-1.0)),
    ));
    add(case(
        "DOMAIN",
        "f32 2.0 * 3.0",
        FloatOp,
        (2, f(2.0), f(3.0)),
        v(f(6.0)),
        |_| v(f(6.0)),
    ));
    add(case(
        "DOMAIN",
        "f32 1.0 / 4.0",
        FloatOp,
        (3, f(1.0), f(4.0)),
        v(f(0.25)),
        |_| v(f(0.25)),
    ));
    add(case(
        "DOMAIN",
        "f32 1.0 == 1.0",
        FloatCmp,
        (0, f(1.0), f(1.0)),
        v(1),
        |_| v(1),
    ));
    add(case(
        "DOMAIN",
        "f32 1.0 != 1.0",
        FloatCmp,
        (1, f(1.0), f(1.0)),
        v(0),
        |_| v(0),
    ));
    add(case(
        "DOMAIN",
        "f32 1.0 < 2.0",
        FloatCmp,
        (2, f(1.0), f(2.0)),
        v(1),
        |_| v(1),
    ));
    add(case(
        "DOMAIN",
        "f32 3.0 > 2.0",
        FloatCmp,
        (3, f(3.0), f(2.0)),
        v(1),
        |_| v(1),
    ));
    add(case(
        "DOMAIN",
        "f32 !(1.0 < 2.0)",
        FloatCmp,
        (4, f(1.0), f(2.0)),
        v(0),
        |_| v(0),
    ));
    add(case(
        "DOMAIN",
        "f32 0.0 == 0.0",
        FloatCmp,
        (0, f(0.0), f(0.0)),
        v(1),
        |_| v(1),
    ));
    add(case(
        "DOMAIN",
        "f32 0.0 < 1.0",
        FloatCmp,
        (2, f(0.0), f(1.0)),
        v(1),
        |_| v(1),
    ));
    add(case(
        "DOMAIN",
        "f32 !(1.0 < 1.0)",
        FloatCmp,
        (4, f(1.0), f(1.0)),
        v(1),
        |_| v(1),
    ));

    // ---- KM-07 NaN and infinity through arithmetic and comparison ----
    add(case(
        "KM-07",
        "f32 NaN + 1.0",
        FloatOp,
        (0, NAN, f(1.0)),
        Outcome::Nan,
        |_| Outcome::Nan,
    ));
    add(case(
        "KM-07",
        "f32 +inf + 1.0",
        FloatOp,
        (0, INF, f(1.0)),
        v(INF),
        |_| v(INF),
    ));
    add(case(
        "KM-07",
        "f32 -inf + 1.0",
        FloatOp,
        (0, NEG_INF, f(1.0)),
        v(NEG_INF),
        |_| v(NEG_INF),
    ));
    add(case(
        "KM-07",
        "f32 NaN * 0.0",
        FloatOp,
        (2, NAN, f(0.0)),
        Outcome::Nan,
        |_| Outcome::Nan,
    ));
    add(case(
        "KM-07",
        "f32 +inf * 0.0",
        FloatOp,
        (2, INF, f(0.0)),
        Outcome::Nan,
        |_| Outcome::Nan,
    ));
    add(case(
        "KM-07",
        "f32 -inf * 0.0",
        FloatOp,
        (2, NEG_INF, f(0.0)),
        Outcome::Nan,
        |_| Outcome::Nan,
    ));
    add(case(
        "KM-07",
        "f32 NaN - NaN",
        FloatOp,
        (1, NAN, NAN),
        Outcome::Nan,
        |_| Outcome::Nan,
    ));
    add(case(
        "KM-07",
        "f32 +inf - +inf",
        FloatOp,
        (1, INF, INF),
        Outcome::Nan,
        |_| Outcome::Nan,
    ));
    add(case(
        "KM-07",
        "f32 NaN / NaN",
        FloatOp,
        (3, NAN, NAN),
        Outcome::Nan,
        |_| Outcome::Nan,
    ));
    add(case(
        "KM-07",
        "f32 +inf / +inf",
        FloatOp,
        (3, INF, INF),
        Outcome::Nan,
        |_| Outcome::Nan,
    ));
    add(case(
        "KM-07",
        "f32 0.0 / 0.0",
        FloatOp,
        (3, f(0.0), f(0.0)),
        Outcome::Nan,
        |_| Outcome::Nan,
    ));
    add(case(
        "KM-07",
        "f32 1.0 / 0.0",
        FloatOp,
        (3, f(1.0), f(0.0)),
        v(INF),
        |_| v(INF),
    ));
    add(case(
        "KM-07",
        "f32 -1.0 / 0.0",
        FloatOp,
        (3, f(-1.0), f(0.0)),
        v(NEG_INF),
        |_| v(NEG_INF),
    ));
    add(case(
        "KM-07",
        "f32 3.0e38 * 10.0",
        FloatOp,
        (2, f(3.0e38), f(10.0)),
        v(INF),
        |_| v(INF),
    ));
    add(case(
        "KM-07",
        "f32 -inf - -inf",
        FloatOp,
        (1, NEG_INF, NEG_INF),
        Outcome::Nan,
        |_| Outcome::Nan,
    ));
    add(case(
        "KM-07",
        "f32 -inf / -inf",
        FloatOp,
        (3, NEG_INF, NEG_INF),
        Outcome::Nan,
        |_| Outcome::Nan,
    ));
    // The remaining pre-registered inputs (0.0, 1.0, 3.0e38) through the same operations. Their
    // results are finite or (0.0 / 0.0, above) NaN, so they are controls.
    macro_rules! control {
        ($what:expr, $kind:expr, $operands:expr, $result:expr) => {
            add(case("DOMAIN", $what, $kind, $operands, v($result), |_| {
                v($result)
            }))
        };
    }
    control!("f32 0.0 + 1.0", FloatOp, (0, f(0.0), f(1.0)), f(1.0));
    control!("f32 0.0 * 0.0", FloatOp, (2, f(0.0), f(0.0)), f(0.0));
    control!("f32 0.0 - 0.0", FloatOp, (1, f(0.0), f(0.0)), f(0.0));
    control!("f32 0.0 != 0.0", FloatCmp, (1, f(0.0), f(0.0)), 0);
    control!("f32 0.0 > 1.0", FloatCmp, (3, f(0.0), f(1.0)), 0);
    control!("f32 !(0.0 < 1.0)", FloatCmp, (4, f(0.0), f(1.0)), 0);
    control!("f32 1.0 + 1.0", FloatOp, (0, f(1.0), f(1.0)), f(2.0));
    control!("f32 1.0 * 0.0", FloatOp, (2, f(1.0), f(0.0)), f(0.0));
    control!("f32 1.0 - 1.0", FloatOp, (1, f(1.0), f(1.0)), f(0.0));
    control!("f32 1.0 / 1.0", FloatOp, (3, f(1.0), f(1.0)), f(1.0));
    control!("f32 1.0 < 1.0", FloatCmp, (2, f(1.0), f(1.0)), 0);
    control!("f32 1.0 > 1.0", FloatCmp, (3, f(1.0), f(1.0)), 0);
    control!(
        "f32 3.0e38 + 1.0",
        FloatOp,
        (0, f(3.0e38), f(1.0)),
        f(3.0e38)
    );
    control!("f32 3.0e38 * 0.0", FloatOp, (2, f(3.0e38), f(0.0)), f(0.0));
    control!(
        "f32 3.0e38 - 3.0e38",
        FloatOp,
        (1, f(3.0e38), f(3.0e38)),
        f(0.0)
    );
    // Found by S2 (pre-registered input 3.0e38): the divisor is above 2^126, outside WGSL's division
    // accuracy range, and the GPU returned 0, not 1. Unspecified, so a KM-07 observation.
    add(case(
        "KM-07",
        "f32 3.0e38 / 3.0e38 (divisor above 2^126)",
        FloatOp,
        (3, f(3.0e38), f(3.0e38)),
        v(0),
        |_| v(f(1.0)),
    ));
    control!(
        "f32 3.0e38 == 3.0e38",
        FloatCmp,
        (0, f(3.0e38), f(3.0e38)),
        1
    );
    control!(
        "f32 3.0e38 != 3.0e38",
        FloatCmp,
        (1, f(3.0e38), f(3.0e38)),
        0
    );
    control!("f32 3.0e38 < 1.0", FloatCmp, (2, f(3.0e38), f(1.0)), 0);
    control!("f32 3.0e38 > 1.0", FloatCmp, (3, f(3.0e38), f(1.0)), 1);
    control!("f32 !(3.0e38 < 1.0)", FloatCmp, (4, f(3.0e38), f(1.0)), 1);
    // Comparisons: `f == f`, `f != f`, `f < 1.0`, `f > 1.0`, `!(f < 1.0)`.
    add(case(
        "KM-07",
        "f32 NaN == NaN",
        FloatCmp,
        (0, NAN, NAN),
        v(0),
        |_| v(0),
    ));
    add(case(
        "KM-07",
        "f32 NaN != NaN",
        FloatCmp,
        (1, NAN, NAN),
        v(0),
        |_| v(1),
    ));
    add(case(
        "KM-07",
        "f32 NaN < 1.0",
        FloatCmp,
        (2, NAN, f(1.0)),
        v(0),
        |_| v(0),
    ));
    add(case(
        "KM-07",
        "f32 NaN > 1.0",
        FloatCmp,
        (3, NAN, f(1.0)),
        v(0),
        |_| v(0),
    ));
    add(case(
        "KM-07",
        "f32 !(NaN < 1.0)",
        FloatCmp,
        (4, NAN, f(1.0)),
        v(1),
        |_| v(1),
    ));
    add(case(
        "KM-07",
        "f32 +inf == +inf",
        FloatCmp,
        (0, INF, INF),
        v(1),
        |_| v(1),
    ));
    add(case(
        "KM-07",
        "f32 +inf != +inf",
        FloatCmp,
        (1, INF, INF),
        v(0),
        |_| v(0),
    ));
    add(case(
        "KM-07",
        "f32 +inf < 1.0",
        FloatCmp,
        (2, INF, f(1.0)),
        v(0),
        |_| v(0),
    ));
    add(case(
        "KM-07",
        "f32 +inf > 1.0",
        FloatCmp,
        (3, INF, f(1.0)),
        v(1),
        |_| v(1),
    ));
    add(case(
        "KM-07",
        "f32 !(+inf < 1.0)",
        FloatCmp,
        (4, INF, f(1.0)),
        v(1),
        |_| v(1),
    ));
    add(case(
        "KM-07",
        "f32 -inf == -inf",
        FloatCmp,
        (0, NEG_INF, NEG_INF),
        v(1),
        |_| v(1),
    ));
    add(case(
        "KM-07",
        "f32 -inf != -inf",
        FloatCmp,
        (1, NEG_INF, NEG_INF),
        v(0),
        |_| v(0),
    ));
    add(case(
        "KM-07",
        "f32 -inf < 1.0",
        FloatCmp,
        (2, NEG_INF, f(1.0)),
        v(1),
        |_| v(1),
    ));
    add(case(
        "KM-07",
        "f32 -inf > 1.0",
        FloatCmp,
        (3, NEG_INF, f(1.0)),
        v(0),
        |_| v(0),
    ));
    add(case(
        "KM-07",
        "f32 !(-inf < 1.0)",
        FloatCmp,
        (4, NEG_INF, f(1.0)),
        v(0),
        |_| v(0),
    ));

    // ---- KM-08 denormal operands and results (Rust keeps them) ----
    add(case(
        "KM-08",
        "f32 1.0e-40 * 1.0",
        FloatOp,
        (2, f(DENORMAL), f(1.0)),
        v(0),
        |_| v((DENORMAL * 1.0).to_bits()),
    ));
    add(case(
        "KM-08",
        "f32 1.0e-40 + 0.0",
        FloatOp,
        (0, f(DENORMAL), f(0.0)),
        v(0),
        |_| v((DENORMAL + 0.0).to_bits()),
    ));
    add(case(
        "KM-08",
        "f32 1.0e-40 * 2.0",
        FloatOp,
        (2, f(DENORMAL), f(2.0)),
        v(0),
        |_| v((DENORMAL * 2.0).to_bits()),
    ));
    add(case(
        "KM-08",
        "f32 1.0e-40 / 2.0",
        FloatOp,
        (3, f(DENORMAL), f(2.0)),
        v(0),
        |_| v((DENORMAL / 2.0).to_bits()),
    ));
    add(case(
        "KM-08",
        "f32 1.0e-40 == 0.0",
        FloatCmp,
        (0, f(DENORMAL), f(0.0)),
        v(1),
        |_| v(0),
    ));
    // Revision 1 wrote `1.0e-38 / 4.0` as "a normal input"; 1.0e-38 is itself below the
    // smallest normal f32 (1.17549435e-38), so the normal input used here is 4.0e-38.
    add(case(
        "KM-08",
        "f32 4.0e-38 / 4.0 (normal input, denormal result)",
        FloatOp,
        (3, f(4.0e-38), f(4.0)),
        v(0),
        |_| v((4.0e-38f32 / 4.0).to_bits()),
    ));
    assert!(DENORMAL.is_subnormal());

    // ---- KM-11 out-of-bounds index equal to the buffer length ----
    add(case(
        "KM-11",
        "load source[len] (len 4)",
        OobLoad,
        (0, 0, 0),
        v(0),
        |_| p(OOB),
    ));
    add(case(
        "KM-11",
        "store sink[len] = 77: sink[len - 1] afterwards",
        OobStoreLast,
        (0, 0, 0),
        v(0),
        |_| p(OOB),
    ));
    add(case(
        "KM-11",
        "store sink[len] = 77: sink[0..len - 1] afterwards (OR of them)",
        OobStoreElsewhere,
        (0, 0, 0),
        v(0),
        |_| p(OOB),
    ));

    // ---- KM-09 contended exchange and compare-exchange (set-valued on the GPU) ----
    add(case(
        "KM-09",
        "257 x atomic_exchange(cell, id): final cell",
        ContendedExchange,
        (0, 0, 0),
        Outcome::Within(0, CONTENDERS - 1),
        |_| v(CONTENDERS - 1),
    ));
    add(case(
        "KM-09",
        "257 x atomic_compare_exchange(cell, 999, id + 1): final cell",
        ContendedCompareExchange,
        (0, 0, 0),
        Outcome::Within(1, CONTENDERS),
        |_| v(1),
    ));

    rows
}

// ---- the plain Rust column ----

fn rust_observed(case: &Case) -> Observed {
    let (op, a, b) = case.operands;
    let kind = case.kind;
    let outcome = catch_unwind(AssertUnwindSafe(move || -> u32 {
        let (op, a, b) = (black_box(op), black_box(a), black_box(b));
        match kind {
            Kind::UDiv => {
                if op == 0 {
                    a / b
                } else {
                    a % b
                }
            }
            Kind::IDiv => {
                let (a, b) = (a as i32, b as i32);
                (if op == 0 { a / b } else { a % b }) as u32
            }
            Kind::UShift => {
                if op == 0 {
                    a << b
                } else {
                    a >> b
                }
            }
            Kind::IShift => {
                let a = a as i32;
                (if op == 0 { a << b } else { a >> b }) as u32
            }
            Kind::UArith => match op {
                0 => a + b,
                1 => a - b,
                _ => a * b,
            },
            Kind::IArith => {
                let (a, b) = (a as i32, b as i32);
                (match op {
                    0 => a + b,
                    1 => a - b,
                    2 => a * b,
                    _ => -a,
                }) as u32
            }
            Kind::CastInt => (f32::from_bits(a) as i32) as u32,
            Kind::CastUint => f32::from_bits(a) as u32,
            Kind::FloatOp => {
                let (a, b) = (f32::from_bits(a), f32::from_bits(b));
                match op {
                    0 => a + b,
                    1 => a - b,
                    2 => a * b,
                    _ => a / b,
                }
                .to_bits()
            }
            #[allow(clippy::neg_cmp_op_on_partial_ord)]
            Kind::FloatCmp => {
                let (a, b) = (f32::from_bits(a), f32::from_bits(b));
                u32::from(match op {
                    0 => a == b,
                    1 => a != b,
                    2 => a < b,
                    3 => a > b,
                    _ => !(a < b),
                })
            }
            Kind::OobLoad => {
                let source = black_box(vec![10u32, 11, 12, 13]);
                source[black_box(source.len())]
            }
            Kind::OobStoreLast | Kind::OobStoreElsewhere => {
                let mut sink = black_box(vec![0u32; 4]);
                let index = black_box(sink.len());
                sink[index] = 77;
                sink[3]
            }
            // The sequential host loop: ascending ids, as the CPU twin will run them.
            Kind::ContendedExchange => {
                let mut cell = black_box(9999u32);
                for id in 0..CONTENDERS {
                    cell = black_box(id);
                }
                cell
            }
            Kind::ContendedCompareExchange => {
                let mut cell = black_box(999u32);
                for id in 0..CONTENDERS {
                    if cell == 999 {
                        cell = black_box(id + 1);
                    }
                }
                cell
            }
        }
    }));
    match outcome {
        Ok(bits) => Observed::Value(bits),
        Err(payload) => Observed::Panic(panic_message(payload.as_ref())),
    }
}

// ---- the GPU column ----

trait Bits: GpuPod {
    fn from_bits32(bits: u32) -> Self;
    fn to_bits32(self) -> u32;
}

impl Bits for u32 {
    fn from_bits32(bits: u32) -> Self {
        bits
    }
    fn to_bits32(self) -> u32 {
        self
    }
}

impl Bits for i32 {
    fn from_bits32(bits: u32) -> Self {
        bits as i32
    }
    fn to_bits32(self) -> u32 {
        self as u32
    }
}

impl Bits for f32 {
    fn from_bits32(bits: u32) -> Self {
        f32::from_bits(bits)
    }
    fn to_bits32(self) -> u32 {
        self.to_bits()
    }
}

/// Dispatch `kernel(op, a, b, out)` over every case of one kind; one element per case.
fn run_ops<A: Bits, B: Bits, O: Bits>(
    device: &HeadlessDevice,
    kernel: &KernelDescriptor,
    cases: &[&Case],
) -> Vec<u32> {
    let ops: Vec<u32> = cases.iter().map(|c| c.operands.0).collect();
    let first: Vec<A> = cases.iter().map(|c| A::from_bits32(c.operands.1)).collect();
    let second: Vec<B> = cases.iter().map(|c| B::from_bits32(c.operands.2)).collect();
    let ops = device.create_typed_buffer("mismatch op", &ops, GpuBufferAccess::ReadOnly);
    let first = device.create_typed_buffer("mismatch a", &first, GpuBufferAccess::ReadOnly);
    let second = device.create_typed_buffer("mismatch b", &second, GpuBufferAccess::ReadOnly);
    let out = device.create_typed_buffer(
        "mismatch out",
        &vec![O::from_bits32(0); cases.len()],
        GpuBufferAccess::ReadWrite,
    );
    device
        .dispatch_buffers(
            kernel,
            cases.len() as u32,
            &[
                BufferBinding::read_only(&ops),
                BufferBinding::read_only(&first),
                BufferBinding::read_only(&second),
                BufferBinding::read_write(&out),
            ],
        )
        .unwrap();
    device
        .read_typed_buffer(&out)
        .unwrap()
        .into_iter()
        .map(Bits::to_bits32)
        .collect()
}

/// Dispatch `kernel(f, out)` over every case of one kind.
fn run_casts<O: Bits>(
    device: &HeadlessDevice,
    kernel: &KernelDescriptor,
    cases: &[&Case],
) -> Vec<u32> {
    let floats: Vec<f32> = cases.iter().map(|c| f32::from_bits(c.operands.1)).collect();
    let floats = device.create_typed_buffer("mismatch f", &floats, GpuBufferAccess::ReadOnly);
    let out = device.create_typed_buffer(
        "mismatch cast out",
        &vec![O::from_bits32(0); cases.len()],
        GpuBufferAccess::ReadWrite,
    );
    device
        .dispatch_buffers(
            kernel,
            cases.len() as u32,
            &[
                BufferBinding::read_only(&floats),
                BufferBinding::read_write(&out),
            ],
        )
        .unwrap();
    device
        .read_typed_buffer(&out)
        .unwrap()
        .into_iter()
        .map(Bits::to_bits32)
        .collect()
}

/// KM-11: the index equals the length of `source` and `sink` (4). Returns the loaded value
/// (`seen[0]`, 0xAAAA_AAAA if the invocation never reached the store) and the sink contents.
fn run_out_of_bounds(device: &HeadlessDevice) -> (u32, [u32; 4]) {
    let idx = device.create_typed_buffer("oob idx", &[4u32], GpuBufferAccess::ReadOnly);
    let source = device.create_typed_buffer(
        "oob source",
        &[10u32, 11, 12, 13],
        GpuBufferAccess::ReadOnly,
    );
    let sink = device.create_typed_buffer("oob sink", &[0u32; 4], GpuBufferAccess::ReadWrite);
    let seen =
        device.create_typed_buffer("oob seen", &[0xAAAA_AAAAu32], GpuBufferAccess::ReadWrite);
    device
        .dispatch_buffers(
            &mismatch::out_of_bounds::DESCRIPTOR,
            1,
            &[
                BufferBinding::read_only(&idx).independent_length(),
                BufferBinding::read_only(&source).independent_length(),
                BufferBinding::read_write(&sink).independent_length(),
                BufferBinding::read_write(&seen).independent_length(),
            ],
        )
        .unwrap();
    let sink = device.read_typed_buffer(&sink).unwrap();
    (
        device.read_typed_buffer(&seen).unwrap()[0],
        [sink[0], sink[1], sink[2], sink[3]],
    )
}

/// KM-09: final cell values of repeated 257-way contended dispatches.
fn run_contended(device: &HeadlessDevice, kernel: &KernelDescriptor, initial: u32) -> Vec<u32> {
    (0..CONTENDED_RUNS)
        .map(|_| {
            let cell = device.create_typed_buffer("cell", &[initial], GpuBufferAccess::ReadWrite);
            device
                .dispatch_buffers(
                    kernel,
                    CONTENDERS,
                    &[BufferBinding::read_write(&cell).independent_length()],
                )
                .unwrap();
            device.read_typed_buffer(&cell).unwrap()[0]
        })
        .collect()
}

/// All GPU observations, aligned with `cases` (several per case for the set-valued KM-09).
fn gpu_observed(device: &HeadlessDevice, cases: &[Case]) -> Vec<Vec<u32>> {
    let mut out: Vec<Vec<u32>> = vec![Vec::new(); cases.len()];
    let of_kind = |kind: Kind| -> Vec<usize> {
        (0..cases.len())
            .filter(|&n| cases[n].kind == kind)
            .collect()
    };
    let mut put = |indices: &[usize], values: Vec<u32>| {
        for (&n, value) in indices.iter().zip(values) {
            out[n] = vec![value];
        }
    };
    let pick = |indices: &[usize]| -> Vec<&Case> { indices.iter().map(|&n| &cases[n]).collect() };

    let n = of_kind(Kind::UDiv);
    put(
        &n,
        run_ops::<u32, u32, u32>(device, &mismatch::udivide::DESCRIPTOR, &pick(&n)),
    );
    let n = of_kind(Kind::IDiv);
    put(
        &n,
        run_ops::<i32, i32, i32>(device, &mismatch::idivide::DESCRIPTOR, &pick(&n)),
    );
    let n = of_kind(Kind::UShift);
    put(
        &n,
        run_ops::<u32, u32, u32>(device, &mismatch::ushift::DESCRIPTOR, &pick(&n)),
    );
    let n = of_kind(Kind::IShift);
    put(
        &n,
        run_ops::<i32, u32, i32>(device, &mismatch::ishift::DESCRIPTOR, &pick(&n)),
    );
    let n = of_kind(Kind::UArith);
    put(
        &n,
        run_ops::<u32, u32, u32>(device, &mismatch::uarith::DESCRIPTOR, &pick(&n)),
    );
    let n = of_kind(Kind::IArith);
    put(
        &n,
        run_ops::<i32, i32, i32>(device, &mismatch::iarith::DESCRIPTOR, &pick(&n)),
    );
    let n = of_kind(Kind::CastInt);
    put(
        &n,
        run_casts::<i32>(device, &mismatch::cast_int::DESCRIPTOR, &pick(&n)),
    );
    let n = of_kind(Kind::CastUint);
    put(
        &n,
        run_casts::<u32>(device, &mismatch::cast_uint::DESCRIPTOR, &pick(&n)),
    );
    let n = of_kind(Kind::FloatOp);
    put(
        &n,
        run_ops::<f32, f32, f32>(device, &mismatch::float_op::DESCRIPTOR, &pick(&n)),
    );
    let n = of_kind(Kind::FloatCmp);
    put(
        &n,
        run_ops::<f32, f32, u32>(device, &mismatch::float_cmp::DESCRIPTOR, &pick(&n)),
    );

    let (seen, sink) = run_out_of_bounds(device);
    eprintln!("S2 raw KM-11: seen[0] = 0x{seen:08x}, sink = {sink:08x?}");
    for n in of_kind(Kind::OobLoad) {
        out[n] = vec![seen];
    }
    for n in of_kind(Kind::OobStoreLast) {
        out[n] = vec![sink[3]];
    }
    for n in of_kind(Kind::OobStoreElsewhere) {
        out[n] = vec![sink[0] | sink[1] | sink[2]];
    }
    for n in of_kind(Kind::ContendedExchange) {
        out[n] = run_contended(device, &mismatch::contend_exchange::DESCRIPTOR, 9999);
    }
    for n in of_kind(Kind::ContendedCompareExchange) {
        out[n] = run_contended(device, &mismatch::contend_compare_exchange::DESCRIPTOR, 999);
    }
    out
}

// ---- the tests ----

/// Whether the pinned GPU value of a case is a requirement the test enforces (per case). WGSL
/// defines the result of integer division and remainder by zero, `MIN / -1`, shift amounts of
/// 32 or more and integer overflow (KM-01 to KM-04, KM-10), bounds the set-valued KM-09 outcome,
/// and defines every finite float-to-integer conversion (section 15.7.6: the value in the target
/// type closest to the truncated source that the source float type represents exactly, e.g.
/// `2147483520i`, `4294967040u`, `-1e20f` gives `i32::MIN`): those are enforced, including the
/// finite KM-05 and KM-06 cases. Only the NaN conversions are indeterminate, and NaN or infinity
/// results and comparisons (KM-07), denormal handling (KM-08) and out-of-bounds access (KM-11)
/// are unspecified or implementation-dependent: the pinned GPU value there is a measured
/// one-adapter observation, and a different value is reported as a NOTE, never a failure (owner
/// ruling D39). The plain Rust column is always enforced.
fn gpu_is_enforced(id: &str, kind: Kind, first: u32) -> bool {
    match id {
        "KM-07" | "KM-08" | "KM-11" => false,
        "KM-05" | "KM-06" => {
            !(matches!(kind, Kind::CastInt | Kind::CastUint) && f32::from_bits(first).is_nan())
        }
        _ => true,
    }
}

fn gpu_summary(values: &[u32]) -> String {
    let distinct: BTreeSet<u32> = values.iter().copied().collect();
    if distinct.len() == 1 {
        format!("0x{:08x}", values[0])
    } else {
        format!(
            "{} runs, {} distinct values, min {}, max {}",
            values.len(),
            distinct.len(),
            distinct.first().unwrap(),
            distinct.last().unwrap()
        )
    }
}

#[test]
fn known_mismatches_pin_the_gpu_and_the_plain_rust_outcome() {
    let mode = overflow_checks();
    let device =
        HeadlessDevice::new().expect("Vulkan adapter required; this test must not silently skip");
    let info = device.adapter_info();
    eprintln!(
        "S2 adapter: name={:?} backend={:?} device_type={:?} driver={:?} driver_info={:?}",
        info.name, info.backend, info.device_type, info.driver, info.driver_info
    );
    eprintln!("S2 overflow checks detected in this build: {mode:?}");

    let cases = cases();
    let observed = gpu_observed(&device, &cases);
    let mut failures = Vec::new();
    for (case, gpu) in cases.iter().zip(&observed) {
        let rust = rust_observed(case);
        let rust_expected = (case.rust)(mode);
        let gpu_ok = gpu
            .iter()
            .all(|&bits| case.gpu.accepts(&Observed::Value(bits)));
        let rust_ok = rust_expected.accepts(&rust);
        let enforced = case.enforced;
        eprintln!(
            "S2 | {} | {} | gpu expected {:?} observed {} {} | rust[{mode:?}] expected {:?} observed {} {}",
            case.id,
            case.what,
            case.gpu,
            gpu_summary(gpu),
            if gpu_ok { "ok" } else { "DEVIATES" },
            rust_expected,
            show(&rust),
            if rust_ok { "ok" } else { "DEVIATES" },
        );
        if !gpu_ok && !enforced {
            // Owner ruling D39: an unspecified outcome is a one-adapter observation, not a
            // requirement. A change is reported, never failed.
            eprintln!(
                "NOTE {} `{}`: the one-adapter observation changed (was {:?}, now {}); unspecified by WGSL, not enforced",
                case.id,
                case.what,
                case.gpu,
                gpu_summary(gpu)
            );
        }
        if !gpu_ok && enforced {
            failures.push(format!(
                "{} `{}`: GPU expected {:?}, observed {}",
                case.id,
                case.what,
                case.gpu,
                gpu_summary(gpu)
            ));
        }
        if !rust_ok {
            failures.push(format!(
                "{} `{}`: plain Rust [{mode:?}] expected {:?}, observed {}",
                case.id,
                case.what,
                rust_expected,
                show(&rust)
            ));
        }
        if case.id == "DOMAIN" {
            // Inside the defined domain nothing may differ: the pins themselves must agree.
            for each in [OverflowChecks::Checked, OverflowChecks::Wrapping] {
                if (case.rust)(each) != case.gpu {
                    failures.push(format!(
                        "DOMAIN `{}`: the pinned GPU and Rust outcomes differ in {each:?} mode",
                        case.what
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} ledger expectation(s) failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// The ids of the table rows of `docs/SEMANTICS.md`: lines that start with `| KM-nn |`.
fn documented_ids() -> BTreeSet<String> {
    include_str!("../../../docs/SEMANTICS.md")
        .lines()
        .filter_map(|line| line.strip_prefix("| KM-"))
        .map(|rest| format!("KM-{}", rest.split(' ').next().unwrap_or_default()))
        .collect()
}

fn pinned_ids() -> BTreeSet<String> {
    cases()
        .iter()
        .map(|case| case.id)
        .filter(|id| id.starts_with("KM-"))
        .map(str::to_owned)
        .collect()
}

#[test]
fn every_pinned_ledger_id_is_documented_and_every_documented_id_is_pinned() {
    let documented = documented_ids();
    let pinned = pinned_ids();
    assert_eq!(
        documented, pinned,
        "docs/SEMANTICS.md ledger ids and the ids pinned in tests/mismatch.rs differ"
    );
    assert_eq!(pinned.len(), 11, "KM-01 to KM-11");
    // Every row has at least one case that is not a control, and ids follow KM-nn.
    for id in &pinned {
        assert!(
            id.len() == 5 && id[3..].chars().all(|c| c.is_ascii_digit()),
            "{id}"
        );
    }
}

#[test]
fn the_emitted_wgsl_uses_plain_operators_and_conversions() {
    // The enforced KM rows rest on WGSL's defined results (sections 6.2.3, 8.8, 8.10, 15.7.6),
    // which hold for the WGSL that slangc emits only if it keeps the plain operators. Slang's
    // own language leaves integer division by zero undefined, so this is checked, not assumed.
    let wgsl = |kernel: &KernelDescriptor| gust::slang::compile_wgsl(kernel).unwrap();
    let divide = wgsl(&mismatch::udivide::DESCRIPTOR);
    assert!(divide.contains("a_0[i_0] / b_0[i_0]"), "{divide}");
    assert!(divide.contains("a_0[i_0] % b_0[i_0]"), "{divide}");
    let signed = wgsl(&mismatch::idivide::DESCRIPTOR);
    assert!(
        signed.contains("var _S2 : i32 = a_0[i_0] / b_0[i_0]"),
        "{signed}"
    );
    assert!(
        signed.contains("var _S3 : i32 = a_0[i_0] % b_0[i_0]"),
        "{signed}"
    );
    for kernel in [&mismatch::ushift::DESCRIPTOR, &mismatch::ishift::DESCRIPTOR] {
        let shift = wgsl(kernel);
        assert!(shift.contains("a_0[i_0] << (n_0[i_0])"), "{shift}");
        assert!(shift.contains("a_0[i_0] >> (n_0[i_0])"), "{shift}");
    }
    assert!(wgsl(&mismatch::cast_int::DESCRIPTOR).contains("i32(f_0[i_0])"));
    assert!(wgsl(&mismatch::cast_uint::DESCRIPTOR).contains("u32(f_0[i_0])"));
    let signed_arithmetic = wgsl(&mismatch::iarith::DESCRIPTOR);
    assert!(signed_arithmetic.contains("a_0[i_0] - b_0[i_0]"));
    assert!(signed_arithmetic.contains("a_0[i_0] * b_0[i_0]"));
    assert!(signed_arithmetic.contains("- a_0[i_0]"));
}
