use std::{collections::HashMap, sync::Arc};

use gpu_dialect::KernelDescriptor;

/// Observable state of one device's compiled-kernel cache.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PipelineCacheStats {
    pub entries: usize,
    pub hits: u64,
    pub misses: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct KernelCacheKey {
    entry_point: &'static str,
    slang_address: usize,
    slang_bytes: usize,
    parameters_address: usize,
    parameters: usize,
    workgroup_size: [u32; 3],
}

impl KernelCacheKey {
    pub(crate) fn new(kernel: &KernelDescriptor) -> Self {
        Self {
            entry_point: kernel.entry_point,
            slang_address: kernel.slang_source.as_ptr() as usize,
            slang_bytes: kernel.slang_source.len(),
            parameters_address: kernel.parameters.as_ptr() as usize,
            parameters: kernel.parameters.len(),
            workgroup_size: kernel.workgroup_size,
        }
    }
}

pub(crate) struct CachedKernel {
    pub(crate) _shader: wgpu::ShaderModule,
    pub(crate) bind_group_layout: wgpu::BindGroupLayout,
    pub(crate) _pipeline_layout: wgpu::PipelineLayout,
    pub(crate) pipeline: wgpu::ComputePipeline,
}

#[derive(Default)]
pub(crate) struct PipelineCache {
    pub(crate) kernels: HashMap<KernelCacheKey, Arc<CachedKernel>>,
    pub(crate) hits: u64,
    pub(crate) misses: u64,
}
