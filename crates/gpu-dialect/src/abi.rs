use core::{mem, slice};

/// Scalar formats that have an identical Rust and GPU storage representation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScalarKind {
    U32,
    I32,
    F32,
}

/// Structural category for one host-shareable type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeLayoutKind {
    Scalar(ScalarKind),
    Struct(&'static [StructFieldLayout]),
}

/// Complete storage-buffer ABI for one element type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TypeLayout {
    pub name: &'static str,
    pub size: u32,
    pub alignment: u32,
    pub stride: u32,
    pub kind: TypeLayoutKind,
}

impl TypeLayout {
    /// Check that this description follows the supported four-byte scalar/struct ABI.
    pub fn is_storage_v1(self) -> bool {
        if self.size == 0 || self.alignment != 4 || self.stride != self.size {
            return false;
        }
        match self.kind {
            TypeLayoutKind::Scalar(_) => self.size == 4,
            TypeLayoutKind::Struct(fields) => {
                let mut offset = 0_u32;
                for field in fields {
                    if field.offset != offset || !field.ty.is_storage_v1() {
                        return false;
                    }
                    let Some(next) = offset.checked_add(field.ty.size) else {
                        return false;
                    };
                    offset = next;
                }
                offset == self.size
            }
        }
    }

    pub const fn scalar(name: &'static str, kind: ScalarKind) -> Self {
        Self {
            name,
            size: 4,
            alignment: 4,
            stride: 4,
            kind: TypeLayoutKind::Scalar(kind),
        }
    }

    pub const fn structure(
        name: &'static str,
        size: u32,
        alignment: u32,
        fields: &'static [StructFieldLayout],
    ) -> Self {
        Self {
            name,
            size,
            alignment,
            stride: align_up(size, alignment),
            kind: TypeLayoutKind::Struct(fields),
        }
    }
}

/// One field in a macro-described host/GPU structure layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StructFieldLayout {
    pub name: &'static str,
    pub offset: u32,
    pub ty: TypeLayout,
}

/// Layout rules recorded for a shader resource (not Slang compiler reflection).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShaderLayoutRules {
    /// The v1 storage ABI: 32-bit scalars and recursively padding-free structs.
    StorageV1,
}

/// Element ABI attached to one shader resource binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceLayout {
    pub rules: ShaderLayoutRules,
    pub element: TypeLayout,
    pub element_stride: u32,
}

impl ResourceLayout {
    pub const fn storage_v1(element: TypeLayout) -> Self {
        Self {
            rules: ShaderLayoutRules::StorageV1,
            element_stride: element.stride,
            element,
        }
    }
}

/// A bitwise-copyable Rust type whose representation exactly matches the v1
/// GPU storage ABI.
///
/// # Safety
///
/// Implementors must be `repr(C)` or `repr(transparent)`, contain no padding or
/// invalid bit patterns, have a non-zero size, and make `LAYOUT` exactly match
/// Rust's size, alignment, field order, and offsets.
pub unsafe trait GpuPod: Copy + 'static {
    const LAYOUT: TypeLayout;
}

unsafe impl GpuPod for u32 {
    const LAYOUT: TypeLayout = TypeLayout::scalar("u32", ScalarKind::U32);
}

unsafe impl GpuPod for i32 {
    const LAYOUT: TypeLayout = TypeLayout::scalar("i32", ScalarKind::I32);
}

unsafe impl GpuPod for f32 {
    const LAYOUT: TypeLayout = TypeLayout::scalar("f32", ScalarKind::F32);
}

/// View a POD slice as its storage-buffer bytes without allocating.
pub fn pod_slice_as_bytes<T: GpuPod>(values: &[T]) -> &[u8] {
    let byte_len = mem::size_of_val(values);
    // SAFETY: `GpuPod` guarantees every byte in every value is initialized and
    // accepts arbitrary bitwise copies. The returned slice shares `values`' lifetime.
    unsafe { slice::from_raw_parts(values.as_ptr().cast::<u8>(), byte_len) }
}

const fn align_up(value: u32, alignment: u32) -> u32 {
    let remainder = value % alignment;
    if remainder == 0 {
        value
    } else {
        value + (alignment - remainder)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalar_layouts_are_explicit() {
        assert_eq!(<f32 as GpuPod>::LAYOUT.size, 4);
        assert_eq!(<f32 as GpuPod>::LAYOUT.alignment, 4);
        assert_eq!(<f32 as GpuPod>::LAYOUT.stride, 4);
        assert_eq!(
            ResourceLayout::storage_v1(<f32 as GpuPod>::LAYOUT).element_stride,
            4
        );
    }

    #[test]
    fn pod_bytes_preserve_scalar_bits() {
        let values = [1.0_f32, -2.5];
        let bytes = pod_slice_as_bytes(&values);
        assert_eq!(bytes.len(), 8);
        assert_eq!(&bytes[..4], &1.0_f32.to_ne_bytes());
        assert_eq!(&bytes[4..], &(-2.5_f32).to_ne_bytes());
    }
}
