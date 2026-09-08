use std::{error, fmt};

use gust::{Access, ParameterKind, ResourceLayout};

use crate::GpuBufferAccess;

#[derive(Debug)]
pub enum Error {
    NoAdapter(String),
    RequestDevice(String),
    MissingSlangSource(String),
    Slang(gust::slang::Error),
    Reflection(gust::reflect::Error),
    UnsupportedWorkgroupSize([u32; 3]),
    UnsupportedParameter {
        name: String,
        kind: ParameterKind,
    },
    InvalidResourceLayout {
        parameter: String,
        actual: Option<Box<ResourceLayout>>,
    },
    UnsupportedResourceLayout {
        parameter: String,
        expected: Box<ResourceLayout>,
        actual: Option<Box<ResourceLayout>>,
    },
    UnsupportedBindGroup(u32),
    BindingCount {
        expected: usize,
        actual: usize,
    },
    BindingAccess {
        parameter: String,
        expected: Access,
        actual: Access,
    },
    BufferLength {
        parameter: String,
        expected: usize,
        actual: usize,
    },
    ForeignPersistentBuffer,
    PersistentBufferAccess {
        parameter: String,
        buffer: GpuBufferAccess,
        binding: Access,
    },
    PersistentBufferLength {
        expected: usize,
        actual: usize,
    },
    PersistentBufferNotReadable,
    IndependentLengthEmpty {
        parameter: String,
    },
    GraphDependencyOrder {
        node: usize,
        dependency: usize,
    },
    GraphMissingDependency {
        node: usize,
        producer: usize,
    },
    GraphUnknownReadback(usize),
    GraphReadbackType {
        expected: &'static str,
        actual: &'static str,
    },
    IndirectArgsLayout(&'static str),
    IndirectBindingLength {
        parameter: String,
    },
    PoolTruncateGrows {
        len: usize,
        requested: usize,
    },
    Poll(String),
    Map(String),
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoAdapter(error) => {
                write!(formatter, "no Vulkan compute adapter is available: {error}")
            }
            Self::RequestDevice(error) => {
                write!(formatter, "could not open the GPU device: {error}")
            }
            Self::MissingSlangSource(kernel) => {
                write!(formatter, "kernel `{kernel}` has no generated Slang source")
            }
            Self::Slang(error) => write!(formatter, "could not compile kernel with Slang: {error}"),
            Self::Reflection(error) => {
                write!(formatter, "could not validate kernel reflection: {error}")
            }
            Self::UnsupportedWorkgroupSize(size) => {
                write!(
                    formatter,
                    "the storage backend requires a one-dimensional workgroup, got {size:?}"
                )
            }
            Self::UnsupportedParameter { name, kind } => {
                write!(
                    formatter,
                    "parameter `{name}` has unsupported resource kind {kind:?}"
                )
            }
            Self::InvalidResourceLayout { parameter, actual } => write!(
                formatter,
                "parameter `{parameter}` requires a valid StorageV1 resource layout, got {actual:?}"
            ),
            Self::UnsupportedResourceLayout {
                parameter,
                expected,
                actual,
            } => write!(
                formatter,
                "parameter `{parameter}` has resource layout {actual:?}, but the f32 backend requires {expected:?}",
            ),
            Self::UnsupportedBindGroup(group) => {
                write!(
                    formatter,
                    "only bind group 0 is supported, got group {group}"
                )
            }
            Self::BindingCount { expected, actual } => {
                write!(
                    formatter,
                    "kernel expects {expected} resource bindings, got {actual}"
                )
            }
            Self::BindingAccess {
                parameter,
                expected,
                actual,
            } => write!(
                formatter,
                "parameter `{parameter}` expects {expected:?} access, got {actual:?}",
            ),
            Self::BufferLength {
                parameter,
                expected,
                actual,
            } => write!(
                formatter,
                "parameter `{parameter}` expects {expected} elements, got {actual}",
            ),
            Self::ForeignPersistentBuffer => {
                write!(
                    formatter,
                    "persistent buffer belongs to a different GPU device"
                )
            }
            Self::PersistentBufferAccess {
                parameter,
                buffer,
                binding,
            } => write!(
                formatter,
                "parameter `{parameter}` was bound as {binding:?}, which the {buffer:?} persistent buffer does not allow",
            ),
            Self::PersistentBufferLength { expected, actual } => write!(
                formatter,
                "persistent buffer expects {expected} elements, got {actual}",
            ),
            Self::PersistentBufferNotReadable => write!(
                formatter,
                "readback requires a persistent buffer created with read-write access",
            ),
            Self::IndependentLengthEmpty { parameter } => write!(
                formatter,
                "parameter `{parameter}` was bound with an independent length but the buffer is empty; the GPU would observe a one-element placeholder",
            ),
            Self::GraphDependencyOrder { node, dependency } => write!(
                formatter,
                "graph node {node} depends on node {dependency}, which is not an earlier node of the same graph",
            ),
            Self::GraphMissingDependency { node, producer } => write!(
                formatter,
                "graph node {node} touches a buffer last written or read by node {producer} without declaring a dependency on it",
            ),
            Self::GraphUnknownReadback(node) => write!(
                formatter,
                "graph node {node} is not a readback node of this execution",
            ),
            Self::GraphReadbackType { expected, actual } => write!(
                formatter,
                "graph readback holds `{expected}` elements, but `{actual}` was requested",
            ),
            Self::IndirectArgsLayout(name) => write!(
                formatter,
                "indirect dispatch arguments must come from `create_indirect_buffer` with a non-empty struct of exactly three `u32` fields (x, y, z workgroups); got `{name}`",
            ),
            Self::IndirectBindingLength { parameter } => write!(
                formatter,
                "parameter `{parameter}` of an indirect dispatch must be bound with `independent_length()` and a non-empty buffer; the element count is decided on the GPU",
            ),
            Self::PoolTruncateGrows { len, requested } => write!(
                formatter,
                "pool truncate to {requested} exceeds the logical length {len}; use push or reserve to grow",
            ),
            Self::Poll(error) => write!(formatter, "GPU synchronization failed: {error}"),
            Self::Map(error) => write!(formatter, "GPU readback failed: {error}"),
        }
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::Slang(error) => Some(error),
            Self::Reflection(error) => Some(error),
            _ => None,
        }
    }
}
