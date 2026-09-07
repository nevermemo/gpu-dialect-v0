//! Explicit staged execution graph: upload, dispatch, and readback nodes with
//! host-declared dependencies, executed as one ordered submission.
//!
//! This is the first bounded proof from `docs/EXECUTION_GRAPH.md`. Nothing is
//! inferred: the host declares every edge, and execution rejects a graph whose
//! declared edges do not cover the buffer hazards its nodes actually create.
//! Uploads are encoded as buffer copies so they stay ordered relative to the
//! dispatches around them; `Queue::write_buffer` would run before the whole
//! submission.

use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

use gpu_dialect::{Access, GpuPod, TypeLayout, pod_slice_as_bytes};
use wgpu::util::{BufferInitDescriptor, DeviceExt};

use crate::{
    BufferBinding, BufferDispatch, Error, GpuBuffer, GpuBufferAccess, HeadlessDevice,
    pod_vec_from_bytes,
};

/// Handle to one node of a [`StagedGraph`], in insertion order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeId(usize);

impl NodeId {
    pub const fn index(self) -> usize {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferDirection {
    Upload,
    Readback,
}

/// One host/GPU transfer performed by an executed graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransferRecord {
    pub node: NodeId,
    pub direction: TransferDirection,
    pub bytes: u64,
}

/// What an execution moved across the host boundary and what stayed resident.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphReport {
    pub transfers: Vec<TransferRecord>,
    pub upload_bytes: u64,
    pub readback_bytes: u64,
    /// Dispatch nodes that produced GPU work (zero-element dispatches are skipped).
    pub dispatches: usize,
    /// Dispatches whose workgroup count was read from a GPU buffer; the host
    /// never learned that count, so they are excluded from `workgroups`.
    pub indirect_dispatches: usize,
    pub workgroups: u64,
    /// Bytes of buffers touched by dispatches but by no transfer in this execution.
    pub resident_bytes: u64,
    pub host_elapsed: Duration,
}

enum NodeKind<'a> {
    Upload {
        target: BufferBinding<'a>,
        bytes: &'a [u8],
        element_count: usize,
    },
    Dispatch(BufferDispatch<'a>),
    /// Workgroup counts come from `args` on the GPU; no host element count exists.
    DispatchIndirect {
        dispatch: BufferDispatch<'a>,
        args: BufferBinding<'a>,
    },
    Readback {
        source: BufferBinding<'a>,
        layout: TypeLayout,
    },
}

struct Node<'a> {
    kind: NodeKind<'a>,
    dependencies: Vec<NodeId>,
}

/// An ordered set of explicit execution nodes over resident typed buffers.
#[derive(Default)]
pub struct StagedGraph<'a> {
    nodes: Vec<Node<'a>>,
}

impl<'a> StagedGraph<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Replace the complete contents of a resident buffer from host memory.
    pub fn upload<T: GpuPod>(
        &mut self,
        buffer: &'a GpuBuffer<T>,
        values: &'a [T],
        dependencies: &[NodeId],
    ) -> NodeId {
        self.push(
            NodeKind::Upload {
                target: BufferBinding::new(buffer, Access::ReadWrite),
                bytes: pod_slice_as_bytes(values),
                element_count: values.len(),
            },
            dependencies,
        )
    }

    /// Run one kernel over resident buffers.
    pub fn dispatch(&mut self, dispatch: BufferDispatch<'a>, dependencies: &[NodeId]) -> NodeId {
        self.push(NodeKind::Dispatch(dispatch), dependencies)
    }

    /// Run one kernel with workgroup counts read from `args` on the GPU.
    ///
    /// `dispatch.element_count` is ignored; every binding must opt in with
    /// `independent_length()` and be non-empty, and `args` must come from
    /// `HeadlessDevice::create_indirect_buffer`. The runtime cannot bound the
    /// count: wgpu silently zeroes a dispatch that exceeds the device's
    /// per-dimension workgroup limit, so the kernel that writes `args` must clamp.
    pub fn dispatch_indirect<T: GpuPod>(
        &mut self,
        dispatch: BufferDispatch<'a>,
        args: &'a GpuBuffer<T>,
        dependencies: &[NodeId],
    ) -> NodeId {
        self.push(
            NodeKind::DispatchIndirect {
                dispatch,
                args: BufferBinding::new(args, Access::ReadOnly),
            },
            dependencies,
        )
    }

    /// Copy a resident read-write buffer back to host memory after execution.
    pub fn readback<T: GpuPod>(
        &mut self,
        buffer: &'a GpuBuffer<T>,
        dependencies: &[NodeId],
    ) -> NodeId {
        self.push(
            NodeKind::Readback {
                source: BufferBinding::new(buffer, Access::ReadOnly),
                layout: T::LAYOUT,
            },
            dependencies,
        )
    }

    fn push(&mut self, kind: NodeKind<'a>, dependencies: &[NodeId]) -> NodeId {
        self.nodes.push(Node {
            kind,
            dependencies: dependencies.to_vec(),
        });
        NodeId(self.nodes.len() - 1)
    }

    /// Buffers a node touches, with the access the hazard check must assume.
    fn accesses(node: &Node<'a>) -> Vec<(&'a wgpu::Buffer, Access)> {
        match &node.kind {
            NodeKind::Upload { target, .. } => vec![(target.raw, Access::ReadWrite)],
            NodeKind::Dispatch(dispatch) => dispatch
                .bindings
                .iter()
                .map(|binding| (binding.raw, binding.access))
                .collect(),
            NodeKind::DispatchIndirect { dispatch, args } => dispatch
                .bindings
                .iter()
                .map(|binding| (binding.raw, binding.access))
                .chain([(args.raw, Access::ReadOnly)])
                .collect(),
            NodeKind::Readback { source, .. } => vec![(source.raw, Access::ReadOnly)],
        }
    }

    /// Check declared edges before any GPU object is created.
    fn validate(&self, device_id: u64) -> Result<(), Error> {
        let mut ancestors: Vec<Vec<bool>> = Vec::with_capacity(self.nodes.len());
        for (index, node) in self.nodes.iter().enumerate() {
            let mut reachable = vec![false; index];
            for dependency in &node.dependencies {
                if dependency.0 >= index {
                    return Err(Error::GraphDependencyOrder {
                        node: index,
                        dependency: dependency.0,
                    });
                }
                reachable[dependency.0] = true;
                for (earlier, &covered) in ancestors[dependency.0].iter().enumerate() {
                    reachable[earlier] |= covered;
                }
            }

            match &node.kind {
                NodeKind::Upload {
                    target,
                    element_count,
                    ..
                } => {
                    if target.device_id != device_id {
                        return Err(Error::ForeignPersistentBuffer);
                    }
                    if target.len != *element_count {
                        return Err(Error::PersistentBufferLength {
                            expected: target.len,
                            actual: *element_count,
                        });
                    }
                }
                NodeKind::Dispatch(dispatch) => {
                    crate::validate_persistent_dispatch(
                        device_id,
                        dispatch.kernel,
                        dispatch.element_count,
                        dispatch.bindings,
                    )?;
                }
                NodeKind::DispatchIndirect { dispatch, args } => {
                    if args.device_id != device_id {
                        return Err(Error::ForeignPersistentBuffer);
                    }
                    if !args.indirect
                        || !crate::is_indirect_args_layout(args.layout)
                        || args.len == 0
                    {
                        return Err(Error::IndirectArgsLayout(args.layout.name));
                    }
                    let resources = crate::validate_persistent_dispatch(
                        device_id,
                        dispatch.kernel,
                        // Placeholder count: every binding must be independent anyway.
                        1,
                        dispatch.bindings,
                    )?;
                    for (parameter, binding) in resources.iter().zip(dispatch.bindings) {
                        if !binding.independent_length || binding.len == 0 {
                            return Err(Error::IndirectBindingLength {
                                parameter: parameter.name.to_owned(),
                            });
                        }
                    }
                }
                NodeKind::Readback { source, .. } => {
                    if source.device_id != device_id {
                        return Err(Error::ForeignPersistentBuffer);
                    }
                    if source.buffer_access != GpuBufferAccess::ReadWrite {
                        return Err(Error::PersistentBufferNotReadable);
                    }
                }
            }

            // Every earlier conflicting access to the same buffer must be an ancestor.
            for (buffer, access) in Self::accesses(node) {
                for (earlier, previous) in self.nodes[..index].iter().enumerate() {
                    let conflict =
                        Self::accesses(previous)
                            .into_iter()
                            .any(|(other, other_access)| {
                                other == buffer
                                    && (access == Access::ReadWrite
                                        || other_access == Access::ReadWrite)
                            });
                    if conflict && !reachable[earlier] {
                        return Err(Error::GraphMissingDependency {
                            node: index,
                            producer: earlier,
                        });
                    }
                }
            }
            ancestors.push(reachable);
        }
        Ok(())
    }

    fn resident_bytes(&self) -> u64 {
        let transferred: Vec<&wgpu::Buffer> = self
            .nodes
            .iter()
            .filter_map(|node| match &node.kind {
                NodeKind::Upload { target, .. } => Some(target.raw),
                NodeKind::Readback { source, .. } => Some(source.raw),
                NodeKind::Dispatch(_) | NodeKind::DispatchIndirect { .. } => None,
            })
            .collect();
        let mut seen: Vec<&wgpu::Buffer> = Vec::new();
        let mut bytes = 0;
        for node in &self.nodes {
            let bindings = match &node.kind {
                NodeKind::Dispatch(dispatch) | NodeKind::DispatchIndirect { dispatch, .. } => {
                    dispatch.bindings
                }
                _ => continue,
            };
            for binding in bindings {
                if transferred.contains(&binding.raw) || seen.contains(&binding.raw) {
                    continue;
                }
                seen.push(binding.raw);
                bytes += binding.byte_len();
            }
        }
        bytes
    }
}

struct PendingReadback {
    node: NodeId,
    layout: TypeLayout,
    staging: Option<wgpu::Buffer>,
}

/// Readbacks and the transfer report of one executed graph.
pub struct GraphOutput {
    pub report: GraphReport,
    readbacks: Vec<(NodeId, TypeLayout, Vec<u8>)>,
}

impl GraphOutput {
    /// Decode the readback produced by `node`; the element type must match the
    /// buffer the node was created from.
    pub fn readback<T: GpuPod>(&self, node: NodeId) -> Result<Vec<T>, Error> {
        let (_, layout, bytes) = self
            .readbacks
            .iter()
            .find(|(id, ..)| *id == node)
            .ok_or(Error::GraphUnknownReadback(node.0))?;
        if *layout != T::LAYOUT {
            return Err(Error::GraphReadbackType {
                expected: layout.name,
                actual: T::LAYOUT.name,
            });
        }
        pod_vec_from_bytes(bytes)
    }
}

impl HeadlessDevice {
    /// Validate, encode, and run a graph, waiting for its readbacks.
    ///
    /// Nodes execute in insertion order inside one command buffer, so a node's
    /// declared dependencies are a contract that is checked, not a scheduling
    /// hint. Intermediate buffers that no node uploads or reads back never leave
    /// the GPU; the report counts their bytes as resident.
    pub fn execute_graph(&self, graph: &StagedGraph<'_>) -> Result<GraphOutput, Error> {
        let started = Instant::now();
        graph.validate(self.id)?;

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("GPU Dialect staged graph encoder"),
            });
        let mut staging_uploads = Vec::new();
        let mut pending = Vec::new();
        let mut transfers = Vec::new();
        let mut dispatches = 0;
        let mut indirect_dispatches = 0;
        let mut workgroups = 0_u64;

        for (index, node) in graph.nodes.iter().enumerate() {
            match &node.kind {
                NodeKind::Upload { target, bytes, .. } => {
                    if !bytes.is_empty() {
                        let staging = self.device.create_buffer_init(&BufferInitDescriptor {
                            label: Some("GPU Dialect staged graph upload"),
                            contents: bytes,
                            usage: wgpu::BufferUsages::COPY_SRC,
                        });
                        encoder.copy_buffer_to_buffer(
                            &staging,
                            0,
                            target.raw,
                            0,
                            bytes.len() as u64,
                        );
                        staging_uploads.push(staging);
                    }
                    transfers.push(TransferRecord {
                        node: NodeId(index),
                        direction: TransferDirection::Upload,
                        bytes: bytes.len() as u64,
                    });
                }
                NodeKind::Dispatch(dispatch) => {
                    for prepared in self.prepare_batch(std::slice::from_ref(dispatch))? {
                        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                            label: Some("GPU Dialect staged graph compute pass"),
                            timestamp_writes: None,
                        });
                        pass.set_pipeline(&prepared.cached_kernel.pipeline);
                        pass.set_bind_group(0, &prepared.bind_group, &[]);
                        pass.dispatch_workgroups(prepared.workgroup_count, 1, 1);
                        dispatches += 1;
                        workgroups += u64::from(prepared.workgroup_count);
                    }
                }
                NodeKind::DispatchIndirect { dispatch, args } => {
                    // Validation already fixed every binding length; the count is
                    // irrelevant to pipeline and bind-group preparation.
                    let placeholder = BufferDispatch::new(dispatch.kernel, 1, dispatch.bindings);
                    for prepared in self.prepare_batch(std::slice::from_ref(&placeholder))? {
                        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                            label: Some("GPU Dialect staged graph indirect compute pass"),
                            timestamp_writes: None,
                        });
                        pass.set_pipeline(&prepared.cached_kernel.pipeline);
                        pass.set_bind_group(0, &prepared.bind_group, &[]);
                        pass.dispatch_workgroups_indirect(args.raw, 0);
                        dispatches += 1;
                        indirect_dispatches += 1;
                    }
                }
                NodeKind::Readback { source, layout } => {
                    let byte_size = source.byte_len();
                    let staging = (byte_size > 0).then(|| {
                        let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
                            label: Some("GPU Dialect staged graph readback"),
                            size: byte_size,
                            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                            mapped_at_creation: false,
                        });
                        encoder.copy_buffer_to_buffer(source.raw, 0, &staging, 0, byte_size);
                        staging
                    });
                    pending.push(PendingReadback {
                        node: NodeId(index),
                        layout: *layout,
                        staging,
                    });
                    transfers.push(TransferRecord {
                        node: NodeId(index),
                        direction: TransferDirection::Readback,
                        bytes: byte_size,
                    });
                }
            }
        }

        let submission = self.queue.submit([encoder.finish()]);
        let receivers: Vec<_> = pending
            .iter()
            .map(|readback| {
                readback.staging.as_ref().map(|staging| {
                    let (sender, receiver) = mpsc::channel();
                    staging.map_async(wgpu::MapMode::Read, .., move |result| {
                        let _ = sender.send(result.map_err(|error| error.to_string()));
                    });
                    receiver
                })
            })
            .collect();
        self.wait_for_submission(submission)?;
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|error| Error::Poll(error.to_string()))?;
        drop(staging_uploads);

        let mut readbacks = Vec::with_capacity(pending.len());
        for (readback, receiver) in pending.into_iter().zip(receivers) {
            let bytes = match (readback.staging, receiver) {
                (Some(staging), Some(receiver)) => {
                    receiver
                        .recv()
                        .map_err(|error| Error::Map(error.to_string()))?
                        .map_err(Error::Map)?;
                    let view = staging
                        .get_mapped_range(..)
                        .map_err(|error| Error::Map(error.to_string()))?;
                    let bytes = view.to_vec();
                    drop(view);
                    staging.unmap();
                    bytes
                }
                _ => Vec::new(),
            };
            readbacks.push((readback.node, readback.layout, bytes));
        }

        let upload_bytes = transfers
            .iter()
            .filter(|transfer| transfer.direction == TransferDirection::Upload)
            .map(|transfer| transfer.bytes)
            .sum();
        let readback_bytes = transfers
            .iter()
            .filter(|transfer| transfer.direction == TransferDirection::Readback)
            .map(|transfer| transfer.bytes)
            .sum();
        Ok(GraphOutput {
            report: GraphReport {
                transfers,
                upload_bytes,
                readback_bytes,
                dispatches,
                indirect_dispatches,
                workgroups,
                resident_bytes: graph.resident_bytes(),
                host_elapsed: started.elapsed(),
            },
            readbacks,
        })
    }
}
