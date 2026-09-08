#[gust::gpu]
mod atomics {
    // T10 vertical slice: `atomic_add` on a `RWStructuredBuffer<u32>` element.
    // Every thread increments the same shared counter; the final value must equal
    // the number of threads that executed the increment.
    //
    // `workgroup_size(1, 1, 1)` makes the dispatch produce exactly `element_count`
    // threads (one per workgroup), so a 257-element dispatch yields exactly 257
    // contending increments rather than a workgroup-rounded count.
    #[kernel(workgroup_size(1, 1, 1))]
    pub fn run(
        _id: SV_DispatchThreadID,
        mut counter: RWStructuredBuffer<uint>,
    ) {
        atomic_add(&mut counter[0], 1u32);
    }
}
