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
    ///
    /// Usable in `const` contexts, so a layout can be asserted at compile time.
    pub const fn is_storage_v1(self) -> bool {
        if self.size == 0 || self.alignment != 4 || self.stride != self.size {
            return false;
        }
        match self.kind {
            TypeLayoutKind::Scalar(_) => self.size == 4,
            TypeLayoutKind::Struct(fields) => {
                let mut offset = 0_u32;
                let mut index = 0;
                while index < fields.len() {
                    let field = &fields[index];
                    if field.offset != offset || !field.ty.is_storage_v1() {
                        return false;
                    }
                    let Some(next) = offset.checked_add(field.ty.size) else {
                        return false;
                    };
                    offset = next;
                    index += 1;
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
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a GUST GPU POD type",
    label = "declare a plain struct inside the same `#[gpu]` module so that the macro implements `GpuPod` for it; `u32`, `i32` and `f32` already implement it",
    note = "an `unsafe impl` written by hand must uphold the layout contract in the safety section of `GpuPod`"
)]
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

    // Evaluated by the compiler: a regression of `is_storage_v1` to a non-const fn, or to a wrong answer on these
    // layouts, fails the build of the test module.
    const U32_FIELDS: &[StructFieldLayout] = &[
        StructFieldLayout {
            name: "a",
            offset: 0,
            ty: TypeLayout::scalar("u32", ScalarKind::U32),
        },
        StructFieldLayout {
            name: "b",
            offset: 4,
            ty: TypeLayout::scalar("u32", ScalarKind::U32),
        },
    ];
    const PADDED_FIELDS: &[StructFieldLayout] = &[
        StructFieldLayout {
            name: "a",
            offset: 0,
            ty: TypeLayout::scalar("u32", ScalarKind::U32),
        },
        StructFieldLayout {
            name: "b",
            offset: 8,
            ty: TypeLayout::scalar("u32", ScalarKind::U32),
        },
    ];
    const SCALAR_OK: bool = <u32 as GpuPod>::LAYOUT.is_storage_v1();
    const PAIR_OK: bool = TypeLayout::structure("Pair", 8, 4, U32_FIELDS).is_storage_v1();
    const NESTED_OK: bool = TypeLayout::structure(
        "Nested",
        16,
        4,
        &[
            StructFieldLayout {
                name: "first",
                offset: 0,
                ty: TypeLayout::structure("Pair", 8, 4, U32_FIELDS),
            },
            StructFieldLayout {
                name: "second",
                offset: 8,
                ty: TypeLayout::structure("Pair", 8, 4, U32_FIELDS),
            },
        ],
    )
    .is_storage_v1();
    const PADDED_OK: bool = TypeLayout::structure("Padded", 12, 4, PADDED_FIELDS).is_storage_v1();
    // Sizes add up to the total (12) but field `c` sits at 12 instead of 8: only the per-field offset check sees it.
    const LATE_PADDING_OK: bool = TypeLayout::structure(
        "Late",
        12,
        4,
        &[
            StructFieldLayout {
                name: "a",
                offset: 0,
                ty: TypeLayout::scalar("u32", ScalarKind::U32),
            },
            StructFieldLayout {
                name: "b",
                offset: 4,
                ty: TypeLayout::scalar("u32", ScalarKind::U32),
            },
            StructFieldLayout {
                name: "c",
                offset: 12,
                ty: TypeLayout::scalar("u32", ScalarKind::U32),
            },
        ],
    )
    .is_storage_v1();

    #[test]
    #[allow(clippy::assertions_on_constants)] // the constants are evaluated by the compiler on purpose
    fn storage_v1_is_decided_in_const_context() {
        assert!(SCALAR_OK);
        assert!(PAIR_OK);
        assert!(NESTED_OK);
        // Padded: field `b` at offset 8 instead of 4, and size 12 instead of 8.
        assert!(!PADDED_OK);
        // Only the per-field offset check rejects this one (the field sizes sum to the total).
        assert!(!LATE_PADDING_OK);
        // The const results agree with the run-time calls.
        assert_eq!(SCALAR_OK, <u32 as GpuPod>::LAYOUT.is_storage_v1());
        assert_eq!(
            PADDED_OK,
            TypeLayout::structure("Padded", 12, 4, PADDED_FIELDS).is_storage_v1()
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
