use core::fmt;

/// Marker implemented by the zero-sized host handle generated for each kernel.
pub trait Kernel {
    const DESCRIPTOR: KernelDescriptor;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModuleDescriptor {
    pub name: &'static str,
    pub kernels: &'static [KernelDescriptor],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KernelDescriptor {
    pub module: &'static str,
    pub name: &'static str,
    /// Prefixed entry point used in generated Slang and target binaries.
    pub entry_point: &'static str,
    pub workgroup_size: [u32; 3],
    pub parameters: &'static [ParameterDescriptor],
    /// Slang source emitted directly from the annotated Rust syntax tree.
    /// Backends compile this source to their preferred target on demand.
    pub slang_source: &'static str,
}

impl KernelDescriptor {
    pub const fn qualified_name(&self) -> QualifiedName<'_> {
        QualifiedName(self)
    }
}

pub struct QualifiedName<'a>(&'a KernelDescriptor);

impl fmt::Display for QualifiedName<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}::{}", self.0.module, self.0.name)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParameterDescriptor {
    pub name: &'static str,
    pub rust_type: &'static str,
    pub kind: ParameterKind,
    pub access: Access,
    /// Host/GPU element ABI for resource parameters supported by the runtime.
    pub resource_layout: Option<crate::ResourceLayout>,
    /// Shader resource location. Invocation and plain value parameters do not
    /// occupy a resource binding.
    pub binding: Option<ResourceBinding>,
}

/// Backend-neutral shader resource address assigned from a kernel parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceBinding {
    pub group: u32,
    pub binding: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParameterKind {
    Invocation,
    Storage,
    Uniform,
    Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    ReadOnly,
    ReadWrite,
    NotApplicable,
}
