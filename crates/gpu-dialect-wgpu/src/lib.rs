//! Headless Vulkan/wgpu execution for GPU Dialect kernel descriptors.
//!
//! Storage buffers support 32-bit scalars and nested GPU POD structs.
//! It reflects the descriptor's group-0 bindings, uploads the supplied data,
//! compiles descriptor Slang source to WGSL, dispatches it, and reads writable buffers back. A
//! persistent typed-buffer path separates allocation, upload, dispatch, and
//! readback so allocations can remain resident across repeated work.

use std::{
    collections::HashMap,
    error, fmt,
    fmt::Write as _,
    marker::PhantomData,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

use gpu_dialect::{
    Access, GpuPod, JobStatus, KernelDescriptor, ParameterDescriptor, ParameterKind,
    ResourceLayout, pod_slice_as_bytes, slang,
};
use wgpu::util::{BufferInitDescriptor, DeviceExt};

static NEXT_DEVICE_ID: AtomicU64 = AtomicU64::new(1);

/// Host data for one reflected `Storage<f32>` or `StorageMut<f32>` parameter.
#[derive(Clone, Copy, Debug)]
pub enum F32Binding<'a> {
    ReadOnly(&'a [f32]),
    ReadWrite(&'a [f32]),
}

impl<'a> F32Binding<'a> {
    fn values(self) -> &'a [f32] {
        match self {
            Self::ReadOnly(values) | Self::ReadWrite(values) => values,
        }
    }

    fn access(self) -> Access {
        match self {
            Self::ReadOnly(_) => Access::ReadOnly,
            Self::ReadWrite(_) => Access::ReadWrite,
        }
    }
}

/// Logical shader access assigned to a persistent GPU buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuBufferAccess {
    ReadOnly,
    ReadWrite,
}

impl GpuBufferAccess {
    fn allows(self, access: Access) -> bool {
        matches!(
            (self, access),
            (Self::ReadOnly, Access::ReadOnly)
                | (Self::ReadWrite, Access::ReadOnly | Access::ReadWrite)
        )
    }
}

/// A device-owned, element-typed allocation that remains resident across dispatches.
pub struct GpuBuffer<T> {
    raw: wgpu::Buffer,
    len: usize,
    access: GpuBufferAccess,
    layout: gpu_dialect::TypeLayout,
    device_id: u64,
    marker: PhantomData<T>,
}

impl<T> GpuBuffer<T> {
    pub const fn len(&self) -> usize {
        self.len
    }

    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub const fn access(&self) -> GpuBufferAccess {
        self.access
    }

    pub const fn layout(&self) -> gpu_dialect::TypeLayout {
        self.layout
    }

    /// Expose the underlying allocation for generated or advanced wgpu code.
    pub fn as_wgpu_buffer(&self) -> &wgpu::Buffer {
        &self.raw
    }
}

/// One persistent buffer bound to an `f32` storage parameter.
#[derive(Clone, Copy)]
pub enum F32BufferBinding<'a> {
    ReadOnly(&'a GpuBuffer<f32>),
    ReadWrite(&'a GpuBuffer<f32>),
}

impl<'a> F32BufferBinding<'a> {
    fn buffer(self) -> &'a GpuBuffer<f32> {
        match self {
            Self::ReadOnly(buffer) | Self::ReadWrite(buffer) => buffer,
        }
    }

    fn access(self) -> Access {
        match self {
            Self::ReadOnly(_) => Access::ReadOnly,
            Self::ReadWrite(_) => Access::ReadWrite,
        }
    }
}

/// A checked view of a typed allocation for one heterogeneous storage binding.
/// Construct through `read_only` or `read_write`; the original element ABI is retained.
#[derive(Clone, Copy)]
pub struct BufferBinding<'a> {
    raw: &'a wgpu::Buffer,
    len: usize,
    layout: gpu_dialect::TypeLayout,
    buffer_access: GpuBufferAccess,
    access: Access,
    device_id: u64,
}

impl<'a> BufferBinding<'a> {
    pub fn read_only<T: GpuPod>(buffer: &'a GpuBuffer<T>) -> Self {
        Self::new(buffer, Access::ReadOnly)
    }

    pub fn read_write<T: GpuPod>(buffer: &'a GpuBuffer<T>) -> Self {
        Self::new(buffer, Access::ReadWrite)
    }

    fn new<T: GpuPod>(buffer: &'a GpuBuffer<T>, access: Access) -> Self {
        Self {
            raw: &buffer.raw,
            len: buffer.len,
            layout: buffer.layout,
            buffer_access: buffer.access,
            access,
            device_id: buffer.device_id,
        }
    }
}

impl<'a> From<F32BufferBinding<'a>> for BufferBinding<'a> {
    fn from(binding: F32BufferBinding<'a>) -> Self {
        Self::new(binding.buffer(), binding.access())
    }
}

/// One persistent dispatch whose bindings can have different POD element types.
#[derive(Clone, Copy)]
pub struct BufferDispatch<'a> {
    pub kernel: &'a KernelDescriptor,
    pub element_count: u32,
    pub bindings: &'a [BufferBinding<'a>],
}

impl<'a> BufferDispatch<'a> {
    pub const fn new(
        kernel: &'a KernelDescriptor,
        element_count: u32,
        bindings: &'a [BufferBinding<'a>],
    ) -> Self {
        Self {
            kernel,
            element_count,
            bindings,
        }
    }
}

/// One persistent-buffer dispatch to encode as part of a batch.
#[derive(Clone, Copy)]
pub struct F32BufferDispatch<'a> {
    pub kernel: &'a KernelDescriptor,
    pub element_count: u32,
    pub bindings: &'a [F32BufferBinding<'a>],
}

impl<'a> F32BufferDispatch<'a> {
    pub const fn new(
        kernel: &'a KernelDescriptor,
        element_count: u32,
        bindings: &'a [F32BufferBinding<'a>],
    ) -> Self {
        Self {
            kernel,
            element_count,
            bindings,
        }
    }
}

/// Result of a completed dispatch. Writable buffers are returned in binding order.
#[derive(Debug, PartialEq)]
pub struct DispatchOutput {
    pub writable_buffers: Vec<Vec<f32>>,
}

/// Observable state of one device's compiled-kernel cache.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PipelineCacheStats {
    pub entries: usize,
    pub hits: u64,
    pub misses: u64,
}

/// Host-observed time for an explicit transfer boundary.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TransferTiming {
    /// Number of application bytes transferred.
    pub bytes: u64,
    /// Wall-clock time including encoding, submission, synchronization, and host copies.
    pub elapsed: Duration,
}

/// Host and device measurements for one synchronous persistent submission.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DispatchTiming {
    /// Wall-clock time including bind-group/command encoding, submission, and synchronization.
    pub host_elapsed: Duration,
    /// Hardware timestamp duration around the submitted compute work, when supported.
    pub gpu_elapsed: Option<Duration>,
}

/// Fence for one submitted wgpu command buffer.
#[must_use = "GPU work is asynchronous; poll or wait on the returned job when completion matters"]
pub struct WgpuJob<'device> {
    device: &'device wgpu::Device,
    submission: Option<wgpu::SubmissionIndex>,
}

impl WgpuJob<'_> {
    /// Poll the exact submission once without blocking.
    pub fn status(&self) -> Result<JobStatus, Error> {
        self.poll_with_timeout(Some(Duration::ZERO))
    }

    pub fn is_complete(&self) -> Result<bool, Error> {
        self.status().map(JobStatus::is_complete)
    }

    /// Wait for at most `timeout`.
    pub fn wait_timeout(&self, timeout: Duration) -> Result<JobStatus, Error> {
        self.poll_with_timeout(Some(timeout))
    }

    /// Wait indefinitely for this submission, without waiting for later work.
    pub fn wait(&self) -> Result<(), Error> {
        self.poll_with_timeout(None).map(|_| ())
    }

    fn poll_with_timeout(&self, timeout: Option<Duration>) -> Result<JobStatus, Error> {
        let Some(submission) = &self.submission else {
            return Ok(JobStatus::Complete);
        };
        match self.device.poll(wgpu::PollType::Wait {
            submission_index: Some(submission.clone()),
            timeout,
        }) {
            Ok(status) if status.wait_finished() => Ok(JobStatus::Complete),
            Ok(_) | Err(wgpu::PollError::Timeout) => Ok(JobStatus::Pending),
            Err(error) => Err(Error::Poll(error.to_string())),
        }
    }
}

impl gpu_dialect::Job for WgpuJob<'_> {
    type Error = Error;

    fn status(&self) -> Result<JobStatus, Self::Error> {
        Self::status(self)
    }

    fn wait_timeout(&self, timeout: Duration) -> Result<JobStatus, Self::Error> {
        Self::wait_timeout(self, timeout)
    }

    fn wait(&self) -> Result<(), Self::Error> {
        Self::wait(self)
    }
}

impl gpu_dialect::Device for HeadlessDevice {
    type Error = Error;
    type Dispatch<'resources> = BufferDispatch<'resources>;
    type Job<'device>
        = WgpuJob<'device>
    where
        Self: 'device;

    fn submit<'device, 'resources>(
        &'device self,
        dispatches: &[Self::Dispatch<'resources>],
    ) -> Result<Self::Job<'device>, Self::Error> {
        self.submit_batch(dispatches)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct KernelCacheKey {
    entry_point: &'static str,
    slang_address: usize,
    slang_bytes: usize,
    parameters_address: usize,
    parameters: usize,
    workgroup_size: [u32; 3],
}

impl KernelCacheKey {
    fn new(kernel: &KernelDescriptor) -> Self {
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

struct CachedKernel {
    _shader: wgpu::ShaderModule,
    bind_group_layout: wgpu::BindGroupLayout,
    _pipeline_layout: wgpu::PipelineLayout,
    pipeline: wgpu::ComputePipeline,
}

struct PreparedDispatch {
    cached_kernel: Arc<CachedKernel>,
    bind_group: wgpu::BindGroup,
    workgroup_count: u32,
}

#[derive(Default)]
struct PipelineCache {
    kernels: HashMap<KernelCacheKey, Arc<CachedKernel>>,
    hits: u64,
    misses: u64,
}

const TIMESTAMP_RESULT_BYTES: u64 = 2 * size_of::<u64>() as u64;

struct TimestampProfiler {
    query_set: wgpu::QuerySet,
    resolve_buffer: wgpu::Buffer,
    readback_buffer: wgpu::Buffer,
}

impl TimestampProfiler {
    fn new(device: &wgpu::Device) -> Self {
        let query_set = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("GPU Dialect dispatch timestamps"),
            ty: wgpu::QueryType::Timestamp,
            count: 2,
        });
        let resolve_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GPU Dialect timestamp resolve"),
            size: TIMESTAMP_RESULT_BYTES,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GPU Dialect timestamp readback"),
            size: TIMESTAMP_RESULT_BYTES,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Self {
            query_set,
            resolve_buffer,
            readback_buffer,
        }
    }
}

/// Render the descriptor-specific wgpu host code used to encode a dispatch.
///
/// The generated source intentionally starts with already-created `wgpu::Buffer`
/// parameters so the binding layout, shader/pipeline construction, and dispatch
/// mechanics remain easy to inspect without including upload/readback policy.
pub fn render_wgpu_source(kernel: &KernelDescriptor) -> Result<String, Error> {
    let resources = validate_kernel(kernel)?;
    let pipeline_type = format!(
        "{}{}Pipeline",
        rust_type_identifier(kernel.module),
        rust_type_identifier(kernel.name)
    );
    let mut source = String::new();
    write_line(
        &mut source,
        format_args!(
            "// @generated by gpu-dialect-wgpu for {}",
            kernel.qualified_name()
        ),
    );
    write_line(
        &mut source,
        format_args!("// Shader source: Rust -> Slang -> WGSL"),
    );
    for parameter in &resources {
        let binding = parameter.binding.expect("validated resource binding");
        let layout = parameter
            .resource_layout
            .expect("validated resource layout");
        write_line(
            &mut source,
            format_args!(
                "// @group({}) @binding({}) {}: {} [{}; size={}, align={}, stride={}]",
                binding.group,
                binding.binding,
                parameter.name,
                layout.element.name,
                match layout.rules {
                    gpu_dialect::ShaderLayoutRules::StorageV1 => "storage-v1",
                },
                layout.element.size,
                layout.element.alignment,
                layout.element_stride,
            ),
        );
        render_struct_fields(&mut source, parameter.name, layout.element, 0);
    }
    write_line(
        &mut source,
        format_args!("// Construct this pipeline once, then reuse encode() for every dispatch."),
    );
    write_line(
        &mut source,
        format_args!("// Call encode() repeatedly on one command encoder to batch dispatches."),
    );
    source.push('\n');
    write_line(&mut source, format_args!("pub struct {pipeline_type} {{"));
    source.push_str("    _shader: wgpu::ShaderModule,\n");
    source.push_str("    bind_group_layout: wgpu::BindGroupLayout,\n");
    source.push_str("    _pipeline_layout: wgpu::PipelineLayout,\n");
    source.push_str("    pipeline: wgpu::ComputePipeline,\n");
    source.push_str("}\n\n");
    write_line(&mut source, format_args!("impl {pipeline_type} {{"));
    source.push_str(
        "    pub fn new(device: &wgpu::Device, descriptor: &gpu_dialect::KernelDescriptor) -> Self {\n",
    );
    write_line(
        &mut source,
        format_args!(
            "        let label = \"{}::{}\";",
            kernel.module, kernel.name
        ),
    );
    source.push_str(
        "        let wgsl = gpu_dialect::slang::compile_wgsl(descriptor).expect(\"compile generated Slang\");\n",
    );
    source.push_str(
        "        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {\n",
    );
    source.push_str("            label: Some(label),\n");
    source.push_str("            source: wgpu::ShaderSource::Wgsl(wgsl.into()),\n");
    source.push_str("        });\n\n");
    source.push_str("        let layout_entries = [\n");
    for parameter in &resources {
        let binding = parameter.binding.expect("validated resource binding");
        source.push_str("            wgpu::BindGroupLayoutEntry {\n");
        write_line(
            &mut source,
            format_args!("                binding: {},", binding.binding),
        );
        source.push_str("                visibility: wgpu::ShaderStages::COMPUTE,\n");
        source.push_str("                ty: wgpu::BindingType::Buffer {\n");
        write_line(
            &mut source,
            format_args!(
                "                    ty: wgpu::BufferBindingType::Storage {{ read_only: {} }},",
                parameter.access == Access::ReadOnly
            ),
        );
        source.push_str("                    has_dynamic_offset: false,\n");
        source.push_str("                    min_binding_size: None,\n");
        source.push_str("                },\n");
        source.push_str("                count: None,\n");
        source.push_str("            },\n");
    }
    source.push_str("        ];\n");
    source.push_str(
        "        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {\n            label: Some(label),\n            entries: &layout_entries,\n        });\n",
    );
    source.push_str(
        "        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {\n            label: Some(label),\n            bind_group_layouts: &[Some(&bind_group_layout)],\n            immediate_size: 0,\n        });\n",
    );
    source.push_str(
        "        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {\n            label: Some(label),\n            layout: Some(&pipeline_layout),\n            module: &shader,\n            entry_point: Some(descriptor.entry_point),\n            compilation_options: Default::default(),\n            cache: None,\n        });\n        Self {\n            _shader: shader,\n            bind_group_layout,\n            _pipeline_layout: pipeline_layout,\n            pipeline,\n        }\n    }\n\n",
    );
    source.push_str("    pub fn encode(\n");
    source.push_str("        &self,\n");
    source.push_str("        device: &wgpu::Device,\n");
    source.push_str("        encoder: &mut wgpu::CommandEncoder,\n");
    source.push_str("        element_count: u32,\n");
    for parameter in &resources {
        write_line(
            &mut source,
            format_args!("        {}: &wgpu::Buffer,", parameter.name),
        );
    }
    source.push_str("    ) {\n");
    write_line(
        &mut source,
        format_args!(
            "        let label = \"{}::{}\";",
            kernel.module, kernel.name
        ),
    );
    source.push_str("        let bind_group_entries = [\n");
    for parameter in &resources {
        let binding = parameter.binding.expect("validated resource binding");
        source.push_str("            wgpu::BindGroupEntry {\n");
        write_line(
            &mut source,
            format_args!("                binding: {},", binding.binding),
        );
        write_line(
            &mut source,
            format_args!(
                "                resource: {}.as_entire_binding(),",
                parameter.name
            ),
        );
        source.push_str("            },\n");
    }
    source.push_str("        ];\n");
    source.push_str(
        "        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {\n            label: Some(label),\n            layout: &self.bind_group_layout,\n            entries: &bind_group_entries,\n        });\n\n",
    );
    source.push_str(
        "        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {\n            label: Some(label),\n            timestamp_writes: None,\n        });\n        pass.set_pipeline(&self.pipeline);\n        pass.set_bind_group(0, &bind_group, &[]);\n",
    );
    write_line(
        &mut source,
        format_args!(
            "        pass.dispatch_workgroups(element_count.div_ceil({}), 1, 1);",
            kernel.workgroup_size[0]
        ),
    );
    source.push_str("    }\n}\n");
    Ok(source)
}

/// A logical device selected without creating a window or presentation surface.
pub struct HeadlessDevice {
    id: u64,
    device: wgpu::Device,
    queue: wgpu::Queue,
    adapter_info: wgpu::AdapterInfo,
    pipeline_cache: Mutex<PipelineCache>,
    timestamp_profiler: Option<Mutex<TimestampProfiler>>,
}

impl HeadlessDevice {
    /// Select a high-performance Vulkan adapter and open a logical device.
    pub fn new() -> Result<Self, Error> {
        pollster::block_on(Self::new_async())
    }

    async fn new_async() -> Result<Self, Error> {
        let mut instance_descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        instance_descriptor.backends = wgpu::Backends::VULKAN;
        let instance = wgpu::Instance::new(instance_descriptor);
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                ..Default::default()
            })
            .await
            .map_err(|error| Error::NoAdapter(error.to_string()))?;
        let adapter_info = adapter.get_info();
        let timestamp_queries = adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY);
        let required_features = if timestamp_queries {
            wgpu::Features::TIMESTAMP_QUERY
        } else {
            wgpu::Features::empty()
        };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("GPU Dialect headless device"),
                required_features: required_features
                    | (adapter.features() & wgpu::Features::MEMORY_DECORATION_COHERENT),
                ..Default::default()
            })
            .await
            .map_err(|error| Error::RequestDevice(error.to_string()))?;
        let timestamp_profiler =
            timestamp_queries.then(|| Mutex::new(TimestampProfiler::new(&device)));

        Ok(Self {
            id: NEXT_DEVICE_ID.fetch_add(1, Ordering::Relaxed),
            device,
            queue,
            adapter_info,
            pipeline_cache: Mutex::new(PipelineCache::default()),
            timestamp_profiler,
        })
    }

    pub fn adapter_info(&self) -> &wgpu::AdapterInfo {
        &self.adapter_info
    }

    pub fn supports_coherent_storage(&self) -> bool {
        self.device
            .features()
            .contains(wgpu::Features::MEMORY_DECORATION_COHERENT)
    }

    /// Whether this device can measure compute passes with hardware timestamp queries.
    pub const fn supports_gpu_timestamps(&self) -> bool {
        self.timestamp_profiler.is_some()
    }

    /// Return hit/miss counts for cached shader modules, layouts, and pipelines.
    pub fn pipeline_cache_stats(&self) -> PipelineCacheStats {
        let cache = self.lock_pipeline_cache();
        PipelineCacheStats {
            entries: cache.kernels.len(),
            hits: cache.hits,
            misses: cache.misses,
        }
    }

    /// Drop every compiled kernel owned by this device and reset its counters.
    pub fn clear_pipeline_cache(&self) {
        *self.lock_pipeline_cache() = PipelineCache::default();
    }

    /// Allocate and initialize a persistent storage buffer for any GPU POD type.
    pub fn create_typed_buffer<T: GpuPod>(
        &self,
        label: &str,
        values: &[T],
        access: GpuBufferAccess,
    ) -> GpuBuffer<T> {
        assert!(
            T::LAYOUT.is_storage_v1()
                && T::LAYOUT.size as usize == size_of::<T>()
                && T::LAYOUT.alignment as usize == align_of::<T>(),
            "GpuPod implementation must match the Rust StorageV1 representation"
        );
        let mut usage = wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST;
        if access == GpuBufferAccess::ReadWrite {
            usage |= wgpu::BufferUsages::COPY_SRC;
        }
        let raw = if values.is_empty() {
            self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: u64::from(T::LAYOUT.stride),
                usage,
                mapped_at_creation: false,
            })
        } else {
            self.device.create_buffer_init(&BufferInitDescriptor {
                label: Some(label),
                contents: pod_slice_as_bytes(values),
                usage,
            })
        };
        GpuBuffer {
            raw,
            len: values.len(),
            access,
            layout: T::LAYOUT,
            device_id: self.id,
            marker: PhantomData,
        }
    }

    pub fn create_f32_buffer(
        &self,
        label: &str,
        values: &[f32],
        access: GpuBufferAccess,
    ) -> GpuBuffer<f32> {
        self.create_typed_buffer(label, values, access)
    }

    /// Replace the complete contents of an existing persistent POD buffer.
    pub fn write_typed_buffer<T: GpuPod>(
        &self,
        buffer: &GpuBuffer<T>,
        values: &[T],
    ) -> Result<(), Error> {
        self.validate_buffer_device(buffer)?;
        if buffer.len != values.len() {
            return Err(Error::PersistentBufferLength {
                expected: buffer.len,
                actual: values.len(),
            });
        }
        if !values.is_empty() {
            self.queue
                .write_buffer(&buffer.raw, 0, pod_slice_as_bytes(values));
        }
        Ok(())
    }

    pub fn write_f32_buffer(&self, buffer: &GpuBuffer<f32>, values: &[f32]) -> Result<(), Error> {
        self.write_typed_buffer(buffer, values)
    }

    /// Replace a persistent POD buffer and wait until the upload has completed.
    pub fn write_typed_buffer_timed<T: GpuPod>(
        &self,
        buffer: &GpuBuffer<T>,
        values: &[T],
    ) -> Result<TransferTiming, Error> {
        let started = Instant::now();
        self.write_typed_buffer(buffer, values)?;
        if !values.is_empty() {
            let submission = self.queue.submit([]);
            self.wait_for_submission(submission)?;
        }
        Ok(TransferTiming {
            bytes: values.len() as u64 * size_of::<T>() as u64,
            elapsed: started.elapsed(),
        })
    }

    pub fn write_f32_buffer_timed(
        &self,
        buffer: &GpuBuffer<f32>,
        values: &[f32],
    ) -> Result<TransferTiming, Error> {
        self.write_typed_buffer_timed(buffer, values)
    }

    /// Copy a persistent read-write POD buffer back to host memory.
    pub fn read_typed_buffer<T: GpuPod>(&self, buffer: &GpuBuffer<T>) -> Result<Vec<T>, Error> {
        self.read_typed_buffer_timed(buffer)
            .map(|(values, _)| values)
    }

    pub fn read_f32_buffer(&self, buffer: &GpuBuffer<f32>) -> Result<Vec<f32>, Error> {
        self.read_typed_buffer(buffer)
    }

    /// Copy a persistent read-write POD buffer back and report its complete host time.
    pub fn read_typed_buffer_timed<T: GpuPod>(
        &self,
        buffer: &GpuBuffer<T>,
    ) -> Result<(Vec<T>, TransferTiming), Error> {
        let started = Instant::now();
        self.validate_buffer_device(buffer)?;
        if buffer.access != GpuBufferAccess::ReadWrite {
            return Err(Error::PersistentBufferNotReadable);
        }
        if buffer.is_empty() {
            return Ok((
                Vec::new(),
                TransferTiming {
                    bytes: 0,
                    elapsed: started.elapsed(),
                },
            ));
        }

        let byte_size = buffer.len as u64 * size_of::<T>() as u64;
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GPU Dialect persistent buffer readback"),
            size: byte_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("GPU Dialect persistent buffer readback encoder"),
            });
        encoder.copy_buffer_to_buffer(&buffer.raw, 0, &readback, 0, byte_size);
        self.queue.submit([encoder.finish()]);

        let (sender, receiver) = mpsc::channel();
        readback.map_async(wgpu::MapMode::Read, .., move |result| {
            let _ = sender.send(result.map_err(|error| error.to_string()));
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|error| Error::Poll(error.to_string()))?;
        receiver
            .recv()
            .map_err(|error| Error::Map(error.to_string()))?
            .map_err(Error::Map)?;
        let view = readback
            .get_mapped_range(..)
            .map_err(|error| Error::Map(error.to_string()))?;
        let values = pod_vec_from_bytes(&view)?;
        drop(view);
        readback.unmap();
        Ok((
            values,
            TransferTiming {
                bytes: byte_size,
                elapsed: started.elapsed(),
            },
        ))
    }

    pub fn read_f32_buffer_timed(
        &self,
        buffer: &GpuBuffer<f32>,
    ) -> Result<(Vec<f32>, TransferTiming), Error> {
        self.read_typed_buffer_timed(buffer)
    }

    /// Compatibility wrapper for an all-f32 persistent dispatch.
    pub fn submit_f32_buffers<'device>(
        &'device self,
        kernel: &KernelDescriptor,
        element_count: u32,
        bindings: &[F32BufferBinding<'_>],
    ) -> Result<WgpuJob<'device>, Error> {
        self.submit_f32_batch(&[F32BufferDispatch::new(kernel, element_count, bindings)])
    }

    pub fn dispatch_f32_buffers(
        &self,
        kernel: &KernelDescriptor,
        element_count: u32,
        bindings: &[F32BufferBinding<'_>],
    ) -> Result<(), Error> {
        self.submit_f32_buffers(kernel, element_count, bindings)?
            .wait()
    }

    pub fn dispatch_f32_buffers_timed(
        &self,
        kernel: &KernelDescriptor,
        element_count: u32,
        bindings: &[F32BufferBinding<'_>],
    ) -> Result<DispatchTiming, Error> {
        self.dispatch_f32_batch_timed(&[F32BufferDispatch::new(kernel, element_count, bindings)])
    }

    pub fn submit_f32_batch<'device>(
        &'device self,
        dispatches: &[F32BufferDispatch<'_>],
    ) -> Result<WgpuJob<'device>, Error> {
        with_f32_dispatches(dispatches, |typed| self.submit_batch(typed))
    }

    pub fn dispatch_f32_batch(&self, dispatches: &[F32BufferDispatch<'_>]) -> Result<(), Error> {
        self.submit_f32_batch(dispatches)?.wait()
    }

    pub fn dispatch_f32_batch_timed(
        &self,
        dispatches: &[F32BufferDispatch<'_>],
    ) -> Result<DispatchTiming, Error> {
        with_f32_dispatches(dispatches, |typed| self.dispatch_batch_timed(typed))
    }

    /// Submit one persistent-buffer dispatch without waiting for completion.
    pub fn submit_buffers<'device>(
        &'device self,
        kernel: &KernelDescriptor,
        element_count: u32,
        bindings: &[BufferBinding<'_>],
    ) -> Result<WgpuJob<'device>, Error> {
        self.submit_batch(&[BufferDispatch::new(kernel, element_count, bindings)])
    }

    /// Dispatch using already-resident typed buffers, without upload or readback.
    ///
    /// This convenience wrapper calls [`Self::submit_buffers`] and waits on
    /// its exact submission fence.
    pub fn dispatch_buffers(
        &self,
        kernel: &KernelDescriptor,
        element_count: u32,
        bindings: &[BufferBinding<'_>],
    ) -> Result<(), Error> {
        self.submit_buffers(kernel, element_count, bindings)?.wait()
    }

    /// Dispatch using persistent buffers and report host time plus an optional GPU timestamp.
    ///
    /// `gpu_elapsed` covers only the compute pass. It is `None` when the
    /// adapter does not expose timestamp queries or when `element_count` is zero.
    pub fn dispatch_buffers_timed(
        &self,
        kernel: &KernelDescriptor,
        element_count: u32,
        bindings: &[BufferBinding<'_>],
    ) -> Result<DispatchTiming, Error> {
        self.dispatch_batch_timed(&[BufferDispatch::new(kernel, element_count, bindings)])
    }

    /// Encode and submit a batch, returning before its GPU work completes.
    ///
    /// Every request is validated before any pipeline, bind group, or command
    /// encoding work begins. Empty requests are valid and do not create GPU work.
    pub fn submit_batch<'device>(
        &'device self,
        dispatches: &[BufferDispatch<'_>],
    ) -> Result<WgpuJob<'device>, Error> {
        let prepared = self.prepare_batch(dispatches)?;
        if prepared.is_empty() {
            return Ok(WgpuJob {
                device: &self.device,
                submission: None,
            });
        }

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("GPU Dialect typed batch encoder"),
            });
        for dispatch in &prepared {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("GPU Dialect typed batched compute pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&dispatch.cached_kernel.pipeline);
            pass.set_bind_group(0, &dispatch.bind_group, &[]);
            pass.dispatch_workgroups(dispatch.workgroup_count, 1, 1);
        }
        let submission = self.queue.submit([encoder.finish()]);
        Ok(WgpuJob {
            device: &self.device,
            submission: Some(submission),
        })
    }

    /// Submit a batch and synchronously wait for its exact submission fence.
    pub fn dispatch_batch(&self, dispatches: &[BufferDispatch<'_>]) -> Result<(), Error> {
        self.submit_batch(dispatches)?.wait()
    }

    /// Submit a batch once and measure its total host and GPU time.
    ///
    /// `gpu_elapsed` brackets the entire compute pass containing every
    /// non-empty dispatch. Divide it by the batch length when the dispatches
    /// represent equivalent repeated work.
    pub fn dispatch_batch_timed(
        &self,
        dispatches: &[BufferDispatch<'_>],
    ) -> Result<DispatchTiming, Error> {
        let started = Instant::now();
        let prepared = self.prepare_batch(dispatches)?;
        if prepared.is_empty() {
            return Ok(DispatchTiming {
                host_elapsed: started.elapsed(),
                gpu_elapsed: None,
            });
        }

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("GPU Dialect profiled typed batch encoder"),
            });

        let Some(timestamp_profiler) = &self.timestamp_profiler else {
            for dispatch in &prepared {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("GPU Dialect profiled typed batched compute pass"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(&dispatch.cached_kernel.pipeline);
                pass.set_bind_group(0, &dispatch.bind_group, &[]);
                pass.dispatch_workgroups(dispatch.workgroup_count, 1, 1);
            }
            let submission = self.queue.submit([encoder.finish()]);
            self.wait_for_submission(submission)?;
            return Ok(DispatchTiming {
                host_elapsed: started.elapsed(),
                gpu_elapsed: None,
            });
        };

        let profiler = timestamp_profiler
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let last_dispatch = prepared.len() - 1;
        for (index, dispatch) in prepared.iter().enumerate() {
            let timestamp_writes = (index == 0 || index == last_dispatch).then_some(
                wgpu::ComputePassTimestampWrites {
                    query_set: &profiler.query_set,
                    beginning_of_pass_write_index: (index == 0).then_some(0),
                    end_of_pass_write_index: (index == last_dispatch).then_some(1),
                },
            );
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("GPU Dialect profiled typed batched compute pass"),
                timestamp_writes,
            });
            pass.set_pipeline(&dispatch.cached_kernel.pipeline);
            pass.set_bind_group(0, &dispatch.bind_group, &[]);
            pass.dispatch_workgroups(dispatch.workgroup_count, 1, 1);
        }
        encoder.resolve_query_set(&profiler.query_set, 0..2, &profiler.resolve_buffer, 0);
        encoder.copy_buffer_to_buffer(
            &profiler.resolve_buffer,
            0,
            &profiler.readback_buffer,
            0,
            TIMESTAMP_RESULT_BYTES,
        );
        let submission = self.queue.submit([encoder.finish()]);
        let gpu_elapsed = self.read_timestamp_duration(&profiler, submission)?;
        Ok(DispatchTiming {
            host_elapsed: started.elapsed(),
            gpu_elapsed: Some(gpu_elapsed),
        })
    }

    /// Dispatch an executable kernel over `element_count` one-dimensional invocations.
    ///
    /// This v1 slice accepts one host binding for each storage parameter and reads
    /// every read-write binding back after the dispatch completes.
    pub fn dispatch_f32(
        &self,
        kernel: &KernelDescriptor,
        element_count: u32,
        bindings: &[F32Binding<'_>],
    ) -> Result<DispatchOutput, Error> {
        let resources = validate_dispatch(kernel, element_count, bindings)?;
        if element_count == 0 {
            return Ok(DispatchOutput {
                writable_buffers: bindings
                    .iter()
                    .copied()
                    .filter_map(|binding| match binding {
                        F32Binding::ReadWrite(values) => Some(values.to_vec()),
                        F32Binding::ReadOnly(_) => None,
                    })
                    .collect(),
            });
        }

        let label = kernel.qualified_name().to_string();
        let cached_kernel = self.cached_kernel(kernel, &resources)?;

        let gpu_buffers = bindings
            .iter()
            .copied()
            .enumerate()
            .map(|(index, binding)| {
                let bytes = f32_as_bytes(binding.values());
                let mut usage = wgpu::BufferUsages::STORAGE;
                if binding.access() == Access::ReadWrite {
                    usage |= wgpu::BufferUsages::COPY_SRC;
                }
                self.device.create_buffer_init(&BufferInitDescriptor {
                    label: Some(&format!("{label} binding {index}")),
                    contents: bytes,
                    usage,
                })
            })
            .collect::<Vec<_>>();
        let bind_group_entries = resources
            .iter()
            .zip(&gpu_buffers)
            .map(|(parameter, buffer)| wgpu::BindGroupEntry {
                binding: parameter
                    .binding
                    .expect("validated resource binding")
                    .binding,
                resource: buffer.as_entire_binding(),
            })
            .collect::<Vec<_>>();
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&format!("{label} bind group")),
            layout: &cached_kernel.bind_group_layout,
            entries: &bind_group_entries,
        });

        let byte_size = u64::from(element_count) * size_of::<f32>() as u64;
        let readback_buffers = bindings
            .iter()
            .copied()
            .map(|binding| match binding {
                F32Binding::ReadOnly(_) => None,
                F32Binding::ReadWrite(_) => {
                    Some(self.device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some(&format!("{label} readback")),
                        size: byte_size,
                        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                        mapped_at_creation: false,
                    }))
                }
            })
            .collect::<Vec<_>>();

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some(&format!("{label} command encoder")),
            });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some(&format!("{label} compute pass")),
                timestamp_writes: None,
            });
            pass.set_pipeline(&cached_kernel.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(element_count.div_ceil(kernel.workgroup_size[0]), 1, 1);
        }
        for (gpu_buffer, readback) in gpu_buffers.iter().zip(&readback_buffers) {
            if let Some(readback) = readback {
                encoder.copy_buffer_to_buffer(gpu_buffer, 0, readback, 0, byte_size);
            }
        }
        self.queue.submit([encoder.finish()]);

        let mut completions = Vec::new();
        for readback in readback_buffers.iter().flatten() {
            let (sender, receiver) = mpsc::channel();
            readback.map_async(wgpu::MapMode::Read, .., move |result| {
                let _ = sender.send(result.map_err(|error| error.to_string()));
            });
            completions.push(receiver);
        }
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|error| Error::Poll(error.to_string()))?;

        let mut writable_buffers = Vec::with_capacity(completions.len());
        for (readback, completion) in readback_buffers.iter().flatten().zip(completions) {
            completion
                .recv()
                .map_err(|error| Error::Map(error.to_string()))?
                .map_err(Error::Map)?;
            let view = readback
                .get_mapped_range(..)
                .map_err(|error| Error::Map(error.to_string()))?;
            let values = view
                .chunks_exact(size_of::<f32>())
                .map(|chunk| f32::from_ne_bytes(chunk.try_into().expect("four-byte f32")))
                .collect();
            drop(view);
            readback.unmap();
            writable_buffers.push(values);
        }

        Ok(DispatchOutput { writable_buffers })
    }

    fn prepare_batch(
        &self,
        dispatches: &[BufferDispatch<'_>],
    ) -> Result<Vec<PreparedDispatch>, Error> {
        let validated = dispatches
            .iter()
            .map(|dispatch| {
                validate_persistent_dispatch(
                    self.id,
                    dispatch.kernel,
                    dispatch.element_count,
                    dispatch.bindings,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        let mut prepared = Vec::with_capacity(dispatches.len());
        for (dispatch, resources) in dispatches.iter().zip(validated) {
            if dispatch.element_count == 0 {
                continue;
            }
            let cached_kernel = self.cached_kernel(dispatch.kernel, &resources)?;
            let label = dispatch.kernel.qualified_name().to_string();
            let bind_group_entries = resources
                .iter()
                .zip(dispatch.bindings)
                .map(|(parameter, binding)| wgpu::BindGroupEntry {
                    binding: parameter
                        .binding
                        .expect("validated resource binding")
                        .binding,
                    resource: binding.raw.as_entire_binding(),
                })
                .collect::<Vec<_>>();
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(&format!("{label} batched bind group")),
                layout: &cached_kernel.bind_group_layout,
                entries: &bind_group_entries,
            });
            prepared.push(PreparedDispatch {
                cached_kernel,
                bind_group,
                workgroup_count: dispatch
                    .element_count
                    .div_ceil(dispatch.kernel.workgroup_size[0]),
            });
        }
        Ok(prepared)
    }

    fn cached_kernel(
        &self,
        kernel: &KernelDescriptor,
        resources: &[&ParameterDescriptor],
    ) -> Result<Arc<CachedKernel>, Error> {
        let key = KernelCacheKey::new(kernel);
        {
            let mut cache = self.lock_pipeline_cache();
            if let Some(cached) = cache.kernels.get(&key).cloned() {
                cache.hits += 1;
                return Ok(cached);
            }
        }

        let compiled = Arc::new(self.compile_kernel(kernel, resources)?);
        let mut cache = self.lock_pipeline_cache();
        if let Some(cached) = cache.kernels.get(&key).cloned() {
            cache.hits += 1;
            return Ok(cached);
        }
        cache.misses += 1;
        cache.kernels.insert(key, Arc::clone(&compiled));
        Ok(compiled)
    }

    fn compile_kernel(
        &self,
        kernel: &KernelDescriptor,
        resources: &[&ParameterDescriptor],
    ) -> Result<CachedKernel, Error> {
        let label = kernel.qualified_name().to_string();
        let layout_entries = resources
            .iter()
            .map(|parameter| wgpu::BindGroupLayoutEntry {
                binding: parameter
                    .binding
                    .expect("validated resource binding")
                    .binding,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage {
                        read_only: parameter.access == Access::ReadOnly,
                    },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            })
            .collect::<Vec<_>>();
        let bind_group_layout =
            self.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some(&format!("{label} bind group layout")),
                    entries: &layout_entries,
                });
        let pipeline_layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(&format!("{label} pipeline layout")),
                bind_group_layouts: &[Some(&bind_group_layout)],
                immediate_size: 0,
            });
        let wgsl = slang::compile_wgsl(kernel).map_err(Error::Slang)?;
        let shader = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(&format!("{label} Slang-generated WGSL")),
                source: wgpu::ShaderSource::Wgsl(wgsl.into()),
            });
        let pipeline = self
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(&format!("{label} pipeline")),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some(kernel.entry_point),
                compilation_options: Default::default(),
                cache: None,
            });
        Ok(CachedKernel {
            _shader: shader,
            bind_group_layout,
            _pipeline_layout: pipeline_layout,
            pipeline,
        })
    }

    fn lock_pipeline_cache(&self) -> MutexGuard<'_, PipelineCache> {
        self.pipeline_cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn validate_buffer_device<T>(&self, buffer: &GpuBuffer<T>) -> Result<(), Error> {
        if buffer.device_id != self.id {
            return Err(Error::ForeignPersistentBuffer);
        }
        Ok(())
    }

    fn wait_for_submission(&self, submission: wgpu::SubmissionIndex) -> Result<(), Error> {
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: None,
            })
            .map_err(|error| Error::Poll(error.to_string()))?;
        Ok(())
    }

    fn read_timestamp_duration(
        &self,
        profiler: &TimestampProfiler,
        submission: wgpu::SubmissionIndex,
    ) -> Result<Duration, Error> {
        let (sender, receiver) = mpsc::channel();
        profiler
            .readback_buffer
            .map_async(wgpu::MapMode::Read, .., move |result| {
                let _ = sender.send(result.map_err(|error| error.to_string()));
            });
        self.wait_for_submission(submission)?;
        receiver
            .recv()
            .map_err(|error| Error::Map(error.to_string()))?
            .map_err(Error::Map)?;
        let view = profiler
            .readback_buffer
            .get_mapped_range(..)
            .map_err(|error| Error::Map(error.to_string()))?;
        let mut timestamps = view
            .chunks_exact(size_of::<u64>())
            .map(|chunk| u64::from_ne_bytes(chunk.try_into().expect("eight-byte timestamp")));
        let beginning = timestamps.next().expect("beginning timestamp");
        let end = timestamps.next().expect("end timestamp");
        drop(view);
        profiler.readback_buffer.unmap();
        let nanoseconds =
            end.wrapping_sub(beginning) as f64 * f64::from(self.queue.get_timestamp_period());
        Ok(Duration::from_secs_f64(nanoseconds / 1_000_000_000.0))
    }
}

fn with_f32_dispatches<R>(
    dispatches: &[F32BufferDispatch<'_>],
    run: impl FnOnce(&[BufferDispatch<'_>]) -> R,
) -> R {
    let bindings = dispatches
        .iter()
        .map(|dispatch| {
            dispatch
                .bindings
                .iter()
                .copied()
                .map(BufferBinding::from)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let typed = dispatches
        .iter()
        .zip(&bindings)
        .map(|(dispatch, bindings)| {
            BufferDispatch::new(dispatch.kernel, dispatch.element_count, bindings)
        })
        .collect::<Vec<_>>();
    run(&typed)
}

fn validate_binding_layout(
    parameter: &ParameterDescriptor,
    actual: ResourceLayout,
) -> Result<(), Error> {
    let expected = parameter
        .resource_layout
        .expect("validated resource layout");
    if expected != actual {
        return Err(Error::UnsupportedResourceLayout {
            parameter: parameter.name.to_owned(),
            expected: Box::new(expected),
            actual: Some(Box::new(actual)),
        });
    }
    Ok(())
}

fn validate_dispatch(
    kernel: &KernelDescriptor,
    element_count: u32,
    bindings: &[F32Binding<'_>],
) -> Result<Vec<&'static ParameterDescriptor>, Error> {
    let resources = validate_kernel(kernel)?;
    if resources.len() != bindings.len() {
        return Err(Error::BindingCount {
            expected: resources.len(),
            actual: bindings.len(),
        });
    }
    for (parameter, binding) in resources.iter().copied().zip(bindings.iter().copied()) {
        validate_binding_layout(
            parameter,
            ResourceLayout::storage_v1(<f32 as GpuPod>::LAYOUT),
        )?;
        if parameter.access != binding.access() {
            return Err(Error::BindingAccess {
                parameter: parameter.name.to_owned(),
                expected: parameter.access,
                actual: binding.access(),
            });
        }
        if binding.values().len() != element_count as usize {
            return Err(Error::BufferLength {
                parameter: parameter.name.to_owned(),
                expected: element_count as usize,
                actual: binding.values().len(),
            });
        }
    }
    Ok(resources)
}

fn validate_persistent_dispatch(
    device_id: u64,
    kernel: &KernelDescriptor,
    element_count: u32,
    bindings: &[BufferBinding<'_>],
) -> Result<Vec<&'static ParameterDescriptor>, Error> {
    let resources = validate_kernel(kernel)?;
    if resources.len() != bindings.len() {
        return Err(Error::BindingCount {
            expected: resources.len(),
            actual: bindings.len(),
        });
    }
    for (parameter, binding) in resources.iter().copied().zip(bindings.iter().copied()) {
        let buffer = binding;
        validate_binding_layout(parameter, ResourceLayout::storage_v1(buffer.layout))?;
        if buffer.device_id != device_id {
            return Err(Error::ForeignPersistentBuffer);
        }
        if parameter.access != binding.access {
            return Err(Error::BindingAccess {
                parameter: parameter.name.to_owned(),
                expected: parameter.access,
                actual: binding.access,
            });
        }
        if !buffer.buffer_access.allows(binding.access) {
            return Err(Error::PersistentBufferAccess {
                parameter: parameter.name.to_owned(),
                buffer: buffer.buffer_access,
                binding: binding.access,
            });
        }
        if buffer.len != element_count as usize {
            return Err(Error::BufferLength {
                parameter: parameter.name.to_owned(),
                expected: element_count as usize,
                actual: buffer.len,
            });
        }
    }
    Ok(resources)
}

fn validate_kernel(kernel: &KernelDescriptor) -> Result<Vec<&'static ParameterDescriptor>, Error> {
    if kernel.slang_source.trim().is_empty() {
        return Err(Error::MissingSlangSource(
            kernel.qualified_name().to_string(),
        ));
    }
    if kernel.workgroup_size[0] == 0
        || kernel.workgroup_size[1] != 1
        || kernel.workgroup_size[2] != 1
    {
        return Err(Error::UnsupportedWorkgroupSize(kernel.workgroup_size));
    }

    let resources = kernel
        .parameters
        .iter()
        .filter(|parameter| parameter.binding.is_some())
        .collect::<Vec<_>>();
    for parameter in resources.iter().copied() {
        if parameter.kind != ParameterKind::Storage {
            return Err(Error::UnsupportedParameter {
                name: parameter.name.to_owned(),
                kind: parameter.kind,
            });
        }
        if !parameter.resource_layout.is_some_and(|layout| {
            layout.element.is_storage_v1() && layout.element_stride == layout.element.stride
        }) {
            return Err(Error::InvalidResourceLayout {
                parameter: parameter.name.to_owned(),
                actual: parameter.resource_layout.map(Box::new),
            });
        }
        let resource_binding = parameter.binding.expect("filtered resource binding");
        if resource_binding.group != 0 {
            return Err(Error::UnsupportedBindGroup(resource_binding.group));
        }
    }
    Ok(resources)
}

fn render_struct_fields(
    target: &mut String,
    path: &str,
    layout: gpu_dialect::TypeLayout,
    base_offset: u32,
) {
    if let gpu_dialect::TypeLayoutKind::Struct(fields) = layout.kind {
        for field in fields {
            let path = format!("{path}.{}", field.name);
            let offset = base_offset + field.offset;
            write_line(
                target,
                format_args!(
                    "//   {path}: {} @ byte {offset} (size={})",
                    field.ty.name, field.ty.size
                ),
            );
            render_struct_fields(target, &path, field.ty, offset);
        }
    }
}

fn rust_type_identifier(value: &str) -> String {
    let mut result = String::new();
    let mut uppercase_next = true;
    for character in value.chars() {
        if !character.is_ascii_alphanumeric() {
            uppercase_next = true;
        } else if uppercase_next {
            result.push(character.to_ascii_uppercase());
            uppercase_next = false;
        } else {
            result.push(character);
        }
    }
    result
}

fn write_line(target: &mut String, arguments: fmt::Arguments<'_>) {
    target
        .write_fmt(arguments)
        .expect("writing into a String cannot fail");
    target.push('\n');
}

fn f32_as_bytes(values: &[f32]) -> &[u8] {
    pod_slice_as_bytes(values)
}

fn pod_vec_from_bytes<T: GpuPod>(bytes: &[u8]) -> Result<Vec<T>, Error> {
    let element_size = size_of::<T>();
    if element_size == 0 || !bytes.len().is_multiple_of(element_size) {
        return Err(Error::Map(format!(
            "{} mapped bytes cannot be decoded as `{}` elements",
            bytes.len(),
            T::LAYOUT.name
        )));
    }
    let len = bytes.len() / element_size;
    let mut values = Vec::<T>::with_capacity(len);
    // SAFETY: `values` owns enough correctly aligned storage for `len` values.
    // `GpuPod` guarantees all copied bit patterns are valid and contain no padding.
    unsafe {
        std::ptr::copy_nonoverlapping(
            bytes.as_ptr(),
            values.as_mut_ptr().cast::<u8>(),
            bytes.len(),
        );
        values.set_len(len);
    }
    Ok(values)
}

#[derive(Debug)]
pub enum Error {
    NoAdapter(String),
    RequestDevice(String),
    MissingSlangSource(String),
    Slang(gpu_dialect::slang::Error),
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
            Self::Poll(error) => write!(formatter, "GPU synchronization failed: {error}"),
            Self::Map(error) => write!(formatter, "GPU readback failed: {error}"),
        }
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::Slang(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpu_dialect::ResourceBinding;

    const PARAMETERS: &[ParameterDescriptor] = &[ParameterDescriptor {
        name: "out",
        rust_type: "StorageMut<f32>",
        kind: ParameterKind::Storage,
        access: Access::ReadWrite,
        resource_layout: Some(ResourceLayout::storage_v1(<f32 as GpuPod>::LAYOUT)),
        binding: Some(ResourceBinding {
            group: 0,
            binding: 0,
        }),
    }];

    const DESCRIPTOR: KernelDescriptor = KernelDescriptor {
        module: "test",
        name: "kernel",
        entry_point: "gpu_test_kernel",
        workgroup_size: [64, 1, 1],
        parameters: PARAMETERS,
        slang_source: r#"
            [[vk::binding(0, 0)]] RWStructuredBuffer<float> out;
            [shader("compute")]
            [numthreads(64, 1, 1)]
            void gpu_test_kernel(uint3 id : SV_DispatchThreadID) { if (id.x < 1) out[id.x] = 0.0; }
        "#,
    };

    #[test]
    fn pipeline_cache_identity_includes_entrypoint() {
        let other = KernelDescriptor {
            entry_point: "gpu_other_entry",
            ..DESCRIPTOR
        };
        assert_ne!(
            KernelCacheKey::new(&DESCRIPTOR),
            KernelCacheKey::new(&other)
        );
        assert_eq!(
            KernelCacheKey::new(&DESCRIPTOR),
            KernelCacheKey::new(&DESCRIPTOR)
        );
    }

    #[test]
    fn shared_source_entrypoints_execute_distinct_pipelines() {
        const SOURCE: &str = r#"
            [[vk::binding(0, 0)]] RWStructuredBuffer<float> out;
            [shader("compute")] [numthreads(64, 1, 1)]
            void first(uint3 id : SV_DispatchThreadID) { if (id.x == 0) out[0] = 11.0; }
            [shader("compute")] [numthreads(64, 1, 1)]
            void second(uint3 id : SV_DispatchThreadID) { if (id.x == 0) out[0] = 22.0; }
        "#;
        let device = HeadlessDevice::new().expect("Vulkan adapter required");
        let buffer =
            device.create_f32_buffer("entrypoint identity", &[0.0], GpuBufferAccess::ReadWrite);
        for (entry, expected) in [
            ("first", 11.0),
            ("second", 22.0),
            ("first", 11.0),
            ("second", 22.0),
        ] {
            let descriptor = KernelDescriptor {
                entry_point: entry,
                slang_source: SOURCE,
                ..DESCRIPTOR
            };
            device
                .dispatch_buffers(&descriptor, 1, &[BufferBinding::read_write(&buffer)])
                .unwrap();
            assert_eq!(device.read_f32_buffer(&buffer).unwrap(), [expected]);
        }
        let stats = device.pipeline_cache_stats();
        assert_eq!((stats.entries, stats.misses, stats.hits), (2, 2, 2));
    }

    #[test]
    fn rejects_incorrect_binding_count_before_gpu_work() {
        assert!(matches!(
            validate_dispatch(&DESCRIPTOR, 0, &[]),
            Err(Error::BindingCount {
                expected: 1,
                actual: 0,
            })
        ));
    }

    #[test]
    fn rejects_resource_layout_that_does_not_match_f32_backend() {
        let parameters = Box::leak(Box::new([ParameterDescriptor {
            resource_layout: Some(ResourceLayout::storage_v1(<u32 as GpuPod>::LAYOUT)),
            ..PARAMETERS[0]
        }]));
        let descriptor = KernelDescriptor {
            parameters,
            ..DESCRIPTOR
        };
        assert!(matches!(
            validate_dispatch(&descriptor, 0, &[F32Binding::ReadWrite(&[])]),
            Err(Error::UnsupportedResourceLayout { parameter, .. }) if parameter == "out"
        ));
    }

    #[test]
    fn renders_descriptor_specific_wgpu_source() {
        let source = render_wgpu_source(&DESCRIPTOR).expect("source should render");
        assert!(source.contains("pub struct TestKernelPipeline"));
        assert!(source.contains("impl TestKernelPipeline"));
        assert!(source.contains("pub fn encode("));
        assert!(source.contains("binding: 0,"));
        assert!(source.contains("read_only: false"));
        assert!(source.contains("gpu_dialect::slang::compile_wgsl"));
        assert!(source.contains("out: f32 [storage-v1; size=4, align=4, stride=4]"));
        assert!(source.contains("pass.dispatch_workgroups(element_count.div_ceil(64), 1, 1);"));
    }
}
