//! Public contract for the Rust-to-Slang GUST compiler.
//!
//! The `#[gpu]` macro lives in a separate proc-macro crate. This crate owns the
//! small, intentionally Slang-shaped vocabulary used by annotated modules,
//! generated Slang descriptors, and host runtime interfaces.
//!
//! # Unsupported source must fail at the Rust boundary
//!
//! A compute entry point cannot return a value:
//! ```compile_fail
//! #[gust::gpu]
//! mod invalid {
//!     #[kernel]
//!     fn run(id: SV_DispatchThreadID) -> uint { id.x }
//! }
//! ```
//! Conditional attributes inside the shader cannot silently lose their meaning:
//! ```compile_fail
//! #[gust::gpu]
//! mod invalid {
//!     #[kernel]
//!     fn run(id: SV_DispatchThreadID) {
//!         #[cfg(any())]
//!         let excluded = 1u32;
//!     }
//! }
//! ```
//! rustc still checks shadow arithmetic after macro validation:
//! ```compile_fail
//! #[gust::gpu]
//! mod invalid {
//!     #[kernel]
//!     fn run(id: SV_DispatchThreadID, mut out: RWStructuredBuffer<uint>) {
//!         out[id.x] = true + 1u32;
//!     }
//! }
//! ```
//! Types and casts outside the 32-bit scalar subset are rejected by the macro,
//! even though rustc would accept them in the shadow body:
//! ```compile_fail
//! #[gust::gpu]
//! mod invalid {
//!     #[kernel]
//!     fn run(id: SV_DispatchThreadID) {
//!         let wide: f64 = id.x as f64;
//!     }
//! }
//! ```
//!
//! # Supported source
//!
//! Named struct construction is lowered field-aware: Slang has no field-name
//! initializers, so the translator emits the portable construct-then-assign form,
//! assigning fields by name in source order. This preserves field identity even
//! when the literal lists fields in a different order than the declaration:
//! ```
//! #[gust::gpu]
//! mod valid {
//!     struct Pair { first: uint, second: uint }
//!     #[kernel]
//!     fn run(id: SV_DispatchThreadID) {
//!         let pair = Pair { second: 1u32, first: 2u32 };
//!     }
//! }
//! ```

mod abi;
mod descriptor;
mod job;
pub mod reflect;
mod runtime;
pub mod slang;
pub mod spirv;

pub use abi::{
    GpuPod, ResourceLayout, ScalarKind, ShaderLayoutRules, StructFieldLayout, TypeLayout,
    TypeLayoutKind, pod_slice_as_bytes,
};
pub use descriptor::{
    Access, Kernel, KernelDescriptor, ModuleDescriptor, ParameterDescriptor, ParameterKind,
    ResourceBinding,
};
pub use gust_macros::gpu;
pub use job::{Device, Job, JobStatus};
pub use reflect::{
    PodCrossCheck, ReflectedField, ReflectedResource, ReflectedType, ReflectedTypeKind, Reflection,
    cross_check_pod,
};
pub use runtime::{
    Invocation, RWStructuredBuffer, SV_DispatchThreadID, Storage, StorageMut, StructuredBuffer,
    UVec3, Uniform, float, int, uint,
};

/// Names automatically imported into every `#[gpu]` module.
pub mod prelude {
    pub use crate::{
        Invocation, RWStructuredBuffer, SV_DispatchThreadID, Storage, StorageMut, StructuredBuffer,
        UVec3, Uniform, float, int, uint,
    };
}
