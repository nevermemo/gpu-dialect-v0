use core::ops::{Deref, Index, IndexMut};

/// Slang-shaped scalar spellings for Rust-side parsing and type checking.
#[allow(non_camel_case_types)]
pub type float = f32;
#[allow(non_camel_case_types)]
pub type int = i32;
#[allow(non_camel_case_types)]
pub type uint = u32;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct UVec3 {
    pub x: u32,
    pub y: u32,
    pub z: u32,
}

/// Rust shadow of Slang's `SV_DispatchThreadID` semantic value.
#[allow(non_camel_case_types)]
pub type SV_DispatchThreadID = UVec3;

/// GPU invocation metadata. The CPU reference runner constructs one per lane.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct Invocation {
    global_id: UVec3,
}

impl Invocation {
    pub const fn new(global_id: UVec3) -> Self {
        Self { global_id }
    }

    pub const fn global_id(self) -> UVec3 {
        self.global_id
    }
}

/// Rust shadow type for Slang's read-only structured buffer.
#[derive(Clone, Copy, Debug)]
pub struct Storage<'a, T> {
    values: &'a [T],
}

/// Slang-shaped spelling for a read-only structured buffer.
pub type StructuredBuffer<'a, T> = Storage<'a, T>;

impl<'a, T> Storage<'a, T> {
    pub const fn new(values: &'a [T]) -> Self {
        Self { values }
    }

    pub fn len(&self) -> u32 {
        u32::try_from(self.values.len()).expect("GPU buffers are limited to u32::MAX elements")
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

impl<T> Index<u32> for Storage<'_, T> {
    type Output = T;

    fn index(&self, index: u32) -> &Self::Output {
        &self.values[index as usize]
    }
}

/// Rust shadow type for Slang's read-write structured buffer.
#[derive(Debug)]
pub struct StorageMut<'a, T> {
    values: &'a mut [T],
}

/// Slang-shaped spelling for a read-write structured buffer.
pub type RWStructuredBuffer<'a, T> = StorageMut<'a, T>;

impl<'a, T> StorageMut<'a, T> {
    pub fn new(values: &'a mut [T]) -> Self {
        Self { values }
    }

    pub fn len(&self) -> u32 {
        u32::try_from(self.values.len()).expect("GPU buffers are limited to u32::MAX elements")
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

impl<T> Index<u32> for StorageMut<'_, T> {
    type Output = T;

    fn index(&self, index: u32) -> &Self::Output {
        &self.values[index as usize]
    }
}

impl<T> IndexMut<u32> for StorageMut<'_, T> {
    fn index_mut(&mut self, index: u32) -> &mut Self::Output {
        &mut self.values[index as usize]
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(transparent)]
pub struct Uniform<T>(pub T);

impl<T> Deref for Uniform<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Shadow stubs for Slang atomic operations. The shadow kernel body is
/// type-checked by rustc but never executed; these stubs exist only to satisfy
/// name resolution and perform no CPU work.
pub fn atomic_add<T>(_target: &mut T, _operand: T) -> T {
    unreachable!()
}
pub fn atomic_min<T>(_target: &mut T, _operand: T) -> T {
    unreachable!()
}
pub fn atomic_max<T>(_target: &mut T, _operand: T) -> T {
    unreachable!()
}
pub fn atomic_exchange<T>(_target: &mut T, _operand: T) -> T {
    unreachable!()
}
pub fn atomic_compare_exchange<T>(_target: &mut T, _compare: T, _value: T) -> bool {
    false
}
