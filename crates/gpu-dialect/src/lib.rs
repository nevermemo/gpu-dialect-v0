//! Public contract for the Rust-to-Slang GPU Dialect prototype.
//!
//! The `#[gpu]` macro lives in a separate proc-macro crate. This crate owns the
//! small, intentionally Slang-shaped vocabulary used by annotated modules,
//! generated Slang descriptors, and host runtime interfaces.
//!
//! # Unsupported source must fail at the Rust boundary
//!
//! A compute entry point cannot return a value:
//! ```compile_fail
//! #[gpu_dialect::gpu]
//! mod invalid {
//!     #[kernel]
//!     fn run(id: SV_DispatchThreadID) -> uint { id.x }
//! }
//! ```
//! Conditional attributes inside the shader cannot silently lose their meaning:
//! ```compile_fail
//! #[gpu_dialect::gpu]
//! mod invalid {
//!     #[kernel]
//!     fn run(id: SV_DispatchThreadID) {
//!         #[cfg(any())]
//!         let excluded = 1u32;
//!     }
//! }
//! ```
//! Named struct construction needs a field-aware lowering before it can be enabled:
//! ```compile_fail
//! #[gpu_dialect::gpu]
//! mod invalid {
//!     struct Pair { first: uint, second: uint }
//!     #[kernel]
//!     fn run(id: SV_DispatchThreadID) {
//!         let pair = Pair { second: 1u32, first: 2u32 };
//!     }
//! }
//! ```
//! rustc still checks shadow arithmetic after macro validation:
//! ```compile_fail
//! #[gpu_dialect::gpu]
//! mod invalid {
//!     #[kernel]
//!     fn run(id: SV_DispatchThreadID, mut out: RWStructuredBuffer<uint>) {
//!         out[id.x] = true + 1u32;
//!     }
//! }
//! ```

mod abi;
mod descriptor;
mod job;
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
pub use gpu_dialect_macros::gpu;
pub use job::{Device, Job, JobStatus};
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
