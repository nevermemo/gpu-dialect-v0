#[gust::gpu]
mod numeric {
    pub struct Pair {
        pub first: uint,
        pub second: uint,
    }

    // `+=` is not in the supported subset (only plain `=` lowers), so the
    // `x = x + y` form is required; the fixture intentionally drives several
    // buffer types, hence more than seven parameters.
    #[allow(clippy::too_many_arguments, clippy::assign_op_pattern)]
    #[kernel(workgroup_size(64, 1, 1))]
    pub fn run(
        id: SV_DispatchThreadID,
        a: StructuredBuffer<int>,
        b: StructuredBuffer<uint>,
        f: StructuredBuffer<float>,
        mut signed_out: RWStructuredBuffer<int>,
        mut unsigned_out: RWStructuredBuffer<uint>,
        mut cast_out: RWStructuredBuffer<int>,
        mut pair_out: RWStructuredBuffer<Pair>,
    ) {
        let i: uint = id.x;
        if i < signed_out.len() {
            // Signed division truncates toward zero; signed modulo keeps the
            // dividend's sign. Both match Rust for non-zero divisors.
            signed_out[i] = a[i] / 2 * 100 + a[i] % 3;
            // Unsigned division and modulo.
            unsigned_out[i] = b[i] / 2 * 100 + b[i] % 3;
            // In-range float -> int cast truncates toward zero.
            cast_out[i] = f[i] as int;
            // Field-aware struct construction: a let binding (fields written in
            // source order, identity by name), a reassignment, and a
            // buffer-element assignment.
            let mut p = Pair { second: b[i] + 1u32, first: b[i] + 2u32 };
            p.first = p.first + 1u32;
            p.second = p.second + 1u32;
            pair_out[i] = Pair { first: p.first, second: p.second };
        }
    }
}
