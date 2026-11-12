//! Worker-owned instanced renderer. No pipeline creation, uploads or draw encoding on a UI frame.
use super::{camera::Camera, draw_plan};
#[path = "textures.rs"]
mod textures;
use super::Palette as Tokens;
use craft_anything_formats::scene_bundle::{Scene, Visibility, math};
use craft_anything_platform::cancel::CancelToken;
use std::{
    collections::BTreeMap,
    future::Future,
    sync::{Arc, mpsc},
    task::{Context, Poll, Waker},
    time::{Duration, Instant},
};
use wgpu as w;

fn bounded<F: Future>(future: F, device: &w::Device) -> Result<F::Output, String> {
    let mut cx = Context::from_waker(Waker::noop());
    let mut future = std::pin::pin!(future);
    let start = Instant::now();
    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut cx) {
            return Ok(value);
        }
        if start.elapsed() > Duration::from_secs(10) {
            return Err("GPU operation timed out; try another graphics adapter.".into());
        }
        let _ = device.poll(w::PollType::Poll);
        std::thread::sleep(Duration::from_millis(2));
    }
}
struct Draw {
    indices: std::ops::Range<u32>,
    base: i32,
}
const INSTANCE_BYTES: usize = 96;
/// Explicit experimental per-instance rendering policy supplied by the shared evaluator.
#[derive(Debug, Clone)]
pub struct InstanceStyle {
    /// None requests original object/source colour, suppressing legacy phase colours.
    pub color: Option<[f32; 3]>,
    pub opacity: f32,
    /// Original world coordinates; keep dot(normal,point)+d <= 0. No cut caps are generated.
    pub clip_plane: Option<[f64; 4]>,
}
impl Default for InstanceStyle {
    fn default() -> Self {
        Self { color: None, opacity: 1., clip_plane: None }
    }
}
/// Transparent instances are sorted globally; exceeding this limit returns a recoverable error.
pub const MAX_TRANSPARENT_DRAWS: usize = 20_000;
struct TransparentDraw {
    mesh: usize,
    instance: u32,
}
pub struct FrameData {
    pub texture_preview: bool,
    pub flip_texture_v: bool,
    pub camera: Camera,
    pub visibility: Visibility,
    pub selected: Option<usize>,
    pub selected_resource: Option<usize>,
    pub size: [u32; 2],
    pub tokens: Tokens,
    pub phases: std::collections::BTreeMap<usize, u8>,
    pub source_colors: bool,
    pub object_colors: bool,
    pub instance_styles: BTreeMap<usize, InstanceStyle>,
}
pub struct Gpu {
    device: w::Device,
    queue: w::Queue,
    pipeline: w::RenderPipeline,
    transparent_pipeline: w::RenderPipeline,
    vertices: w::Buffer,
    uvs: w::Buffer,
    textures: textures::TextureSet,
    indices: w::Buffer,
    instances: w::Buffer,
    uniform: w::Buffer,
    bind: w::BindGroup,
    draws: Vec<Draw>,
    order: Vec<usize>,
    errors: mpsc::Receiver<String>,
    pub view: w::TextureView,
    _color: w::Texture,
    depth: w::TextureView,
    _depth: w::Texture,
    size: [u32; 2],
    pub stats: Option<draw_plan::DrawStats>,
    pub elapsed_ms: u64,
}
fn target(device: &w::Device, size: [u32; 2], depth: bool) -> (w::Texture, w::TextureView) {
    let texture = device.create_texture(&w::TextureDescriptor {
        label: Some("Scene viewport"),
        size: w::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: w::TextureDimension::D2,
        format: if depth { w::TextureFormat::Depth32Float } else { w::TextureFormat::Rgba8Unorm },
        usage: if depth {
            w::TextureUsages::RENDER_ATTACHMENT
        } else {
            w::TextureUsages::RENDER_ATTACHMENT | w::TextureUsages::TEXTURE_BINDING | w::TextureUsages::COPY_SRC
        },
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    (texture, view)
}
fn checked_u32(v: usize) -> Result<u32, String> {
    u32::try_from(v).map_err(|_| "Scene exceeds graphics address range.".into())
}
fn buffer(device: &w::Device, queue: &w::Queue, bytes: &[u8], usage: w::BufferUsages, cancel: &CancelToken) -> Result<w::Buffer, String> {
    let length = bytes.len().max(4) as u64;
    if length > device.limits().max_buffer_size {
        return Err("Scene exceeds this adapter's buffer limit.".into());
    }
    let out = device.create_buffer(&w::BufferDescriptor {
        label: Some("Scene data"),
        size: length,
        usage: usage | w::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    for (i, block) in bytes.chunks(4 * 1024 * 1024).enumerate() {
        if cancel.is_cancelled() {
            return Err("Scene upload cancelled.".into());
        }
        queue.write_buffer(&out, (i * 4 * 1024 * 1024) as u64, block);
        queue.submit([]);
    }
    Ok(out)
}
impl Gpu {
    pub fn new(device: &w::Device, queue: &w::Queue, scene: &Scene, cancel: &CancelToken) -> Result<Self, String> {
        let device = device.clone();
        let queue = queue.clone();
        let (tx, errors) = mpsc::sync_channel(8);
        let lost = tx.clone();
        device.on_uncaptured_error(Arc::new(move |e| {
            let _ = tx.try_send(format!("Graphics error: {e}"));
        }));
        device.set_device_lost_callback(move |_, message| {
            let _ = lost.try_send(format!("Graphics device lost: {message}"));
        });
        let scope = device.push_error_scope(w::ErrorFilter::Validation);
        let shader = device
            .create_shader_module(w::ShaderModuleDescriptor { label: Some("Scene shading"), source: w::ShaderSource::Wgsl(include_str!("scene.wgsl").into()) });
        let uniform = device.create_buffer(&w::BufferDescriptor {
            label: Some("Scene camera"),
            size: 112,
            usage: w::BufferUsages::UNIFORM | w::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&w::BindGroupLayoutDescriptor {
            label: None,
            entries: &[w::BindGroupLayoutEntry {
                binding: 0,
                visibility: w::ShaderStages::VERTEX_FRAGMENT,
                ty: w::BindingType::Buffer { ty: w::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });
        let bind = device.create_bind_group(&w::BindGroupDescriptor {
            label: None,
            layout: &layout,
            entries: &[w::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() }],
        });
        let textures = textures::TextureSet::new(&device, &queue, scene, cancel)?;
        let pipeline_layout = device.create_pipeline_layout(&w::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout), Some(&textures.layout)],
            immediate_size: 0,
        });
        let vertex_attrs = [w::VertexAttribute { format: w::VertexFormat::Float32x3, offset: 0, shader_location: 0 }];
        let uv_attrs = [w::VertexAttribute { format: w::VertexFormat::Float32x2, offset: 0, shader_location: 10 }];
        let instance_attrs = [
            w::VertexAttribute { format: w::VertexFormat::Float32x4, offset: 0, shader_location: 1 },
            w::VertexAttribute { format: w::VertexFormat::Float32x4, offset: 16, shader_location: 2 },
            w::VertexAttribute { format: w::VertexFormat::Float32x4, offset: 32, shader_location: 3 },
            w::VertexAttribute { format: w::VertexFormat::Float32x4, offset: 48, shader_location: 4 },
            w::VertexAttribute { format: w::VertexFormat::Uint32, offset: 64, shader_location: 5 },
            w::VertexAttribute { format: w::VertexFormat::Uint32, offset: 68, shader_location: 6 },
            w::VertexAttribute { format: w::VertexFormat::Uint32, offset: 72, shader_location: 7 },
            w::VertexAttribute { format: w::VertexFormat::Float32, offset: 76, shader_location: 8 },
            w::VertexAttribute { format: w::VertexFormat::Float32x4, offset: 80, shader_location: 9 },
        ];
        let create_pipeline = |transparent: bool| {
            device.create_render_pipeline(&w::RenderPipelineDescriptor {
                label: Some(if transparent { "Scene alpha instancing" } else { "Scene opaque instancing" }),
                layout: Some(&pipeline_layout),
                vertex: w::VertexState {
                    module: &shader,
                    entry_point: Some("vertex"),
                    compilation_options: Default::default(),
                    buffers: &[
                        Some(w::VertexBufferLayout { array_stride: 12, step_mode: w::VertexStepMode::Vertex, attributes: &vertex_attrs }),
                        Some(w::VertexBufferLayout {
                            array_stride: INSTANCE_BYTES as u64,
                            step_mode: w::VertexStepMode::Instance,
                            attributes: &instance_attrs,
                        }),
                        Some(w::VertexBufferLayout { array_stride: 8, step_mode: w::VertexStepMode::Vertex, attributes: &uv_attrs }),
                    ],
                },
                primitive: w::PrimitiveState { cull_mode: None, ..Default::default() },
                depth_stencil: Some(w::DepthStencilState {
                    format: w::TextureFormat::Depth32Float,
                    depth_write_enabled: Some(!transparent),
                    depth_compare: Some(w::CompareFunction::LessEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(w::FragmentState {
                    module: &shader,
                    entry_point: Some("fragment"),
                    compilation_options: Default::default(),
                    targets: &[Some(w::ColorTargetState {
                        format: w::TextureFormat::Rgba8Unorm,
                        blend: transparent.then_some(w::BlendState::ALPHA_BLENDING),
                        write_mask: w::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let pipeline = create_pipeline(false);
        let transparent_pipeline = create_pipeline(true);
        if let Some(e) = bounded(scope.pop(), &device)? {
            return Err(format!("Cannot create scene renderer: {e}"));
        }
        let vertices = buffer(&device, &queue, &scene.gpu_positions, w::BufferUsages::VERTEX, cancel)?;
        let mut uv_bytes = Vec::new();
        let uv_size = scene.positions.len().checked_mul(8).ok_or("UV buffer size overflow")?;
        uv_bytes.try_reserve_exact(uv_size).map_err(|_| "Cannot allocate UV buffer")?;
        for (id, mesh) in scene.meshes.iter().enumerate() {
            let coordinates = scene.mesh_uvs.get(&id);
            if coordinates.is_some_and(|uv| uv.len() != mesh.vertex_count) {
                return Err("UV count differs from mesh vertices".into());
            }
            for vertex in 0..mesh.vertex_count {
                if vertex % 65536 == 0 && cancel.is_cancelled() {
                    return Err("UV upload cancelled".into());
                }
                let uv = coordinates.and_then(|uv| uv.get(vertex)).copied().unwrap_or([0.; 2]);
                for value in uv {
                    if !value.is_finite() {
                        return Err("Invalid UV coordinate".into());
                    }
                    uv_bytes.extend_from_slice(&value.to_le_bytes());
                }
            }
        }
        let uvs = buffer(&device, &queue, &uv_bytes, w::BufferUsages::VERTEX, cancel)?;
        let indices = buffer(&device, &queue, &scene.gpu_indices, w::BufferUsages::INDEX, cancel)?;
        let mut groups = vec![Vec::new(); scene.meshes.len()];
        for (id, i) in scene.instances.iter().enumerate() {
            groups.get_mut(i.mesh).ok_or("Invalid mesh group")?.push(id);
        }
        let mut order = Vec::new();
        let mut draws = Vec::new();
        for (id, group) in groups.into_iter().enumerate() {
            let mesh = scene.meshes.get(id).ok_or("Invalid draw mesh")?;
            order.extend(group);
            draws.push(Draw {
                indices: checked_u32(mesh.index_start)?..checked_u32(mesh.index_start.saturating_add(mesh.triangle_count.saturating_mul(3)))?,
                base: i32::try_from(mesh.vertex_start).map_err(|_| "Mesh base exceeds graphics range")?,
            });
        }
        let instance_size = u64::try_from(order.len().max(1).checked_mul(INSTANCE_BYTES).ok_or("Instance buffer size overflow")?)
            .map_err(|_| "Instance buffer address range")?;
        if instance_size > device.limits().max_buffer_size {
            return Err("Instance data exceeds this adapter buffer limit".into());
        }
        let instances = device.create_buffer(&w::BufferDescriptor {
            label: Some("Scene transforms"),
            size: instance_size,
            usage: w::BufferUsages::VERTEX | w::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let size = [16, 16];
        let (color, view) = target(&device, size, false);
        let (depth, depth_view) = target(&device, size, true);
        Ok(Self {
            device,
            queue,
            pipeline,
            transparent_pipeline,
            vertices,
            uvs,
            textures,
            indices,
            instances,
            uniform,
            bind,
            draws,
            order,
            errors,
            view,
            _color: color,
            depth: depth_view,
            _depth: depth,
            size,
            stats: None,
            elapsed_ms: 0,
        })
    }
    /// Read exactly the rendered frame, excluding row padding. No presentation surface required.
    pub fn rgba(&self, cancel: &CancelToken) -> Result<Vec<u8>, String> {
        let [width, height] = self.size;
        let stride = (width * 4).div_ceil(256) * 256;
        let size = u64::from(stride) * u64::from(height);
        if size > 16 * 1024 * 1024 {
            return Err("Frame readback budget exceeded".into());
        }
        let buffer = self.device.create_buffer(&w::BufferDescriptor {
            label: Some("Scene readback"),
            size,
            usage: w::BufferUsages::COPY_DST | w::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            w::TexelCopyTextureInfo { texture: &self._color, mip_level: 0, origin: w::Origin3d::ZERO, aspect: w::TextureAspect::All },
            w::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: w::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(stride), rows_per_image: Some(height) },
            },
            w::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        self.queue.submit([encoder.finish()]);
        let (tx, rx) = mpsc::channel();
        buffer.slice(..).map_async(w::MapMode::Read, move |result| {
            let _ = tx.send(result.map_err(|e| e.to_string()));
        });
        let start = Instant::now();
        loop {
            let _ = self.device.poll(w::PollType::Poll);
            match rx.try_recv() {
                Ok(r) => {
                    r?;
                    break;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(_) => return Err("Readback callback disconnected".into()),
            }
            if cancel.is_cancelled() || start.elapsed() > Duration::from_secs(15) {
                return Err("Frame readback cancelled or timed out".into());
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        let mapped = buffer.slice(..).get_mapped_range().map_err(|e| e.to_string())?;
        let mut out = Vec::with_capacity((width as usize) * (height as usize) * 4);
        for row in mapped.chunks(stride as usize).take(height as usize) {
            out.extend_from_slice(row.get(..width as usize * 4).ok_or("Truncated readback row")?);
        }
        drop(mapped);
        buffer.unmap();
        Ok(out)
    }
    pub fn render(&mut self, scene: &Scene, frame: &FrameData, cancel: &CancelToken) -> Result<(), String> {
        let started = Instant::now();
        let FrameData { camera, visibility, selected, selected_resource, size: requested, tokens, instance_styles, .. } = frame;
        if let Ok(e) = self.errors.try_recv() {
            return Err(e);
        }
        let cap = self.device.limits().max_texture_dimension_2d.min(1600);
        let size = [requested[0].clamp(1, cap), requested[1].clamp(1, cap)];
        let scope = self.device.push_error_scope(w::ErrorFilter::Validation);
        if size != self.size {
            (self._color, self.view) = target(&self.device, size, false);
            (self._depth, self.depth) = target(&self.device, size, true);
            self.size = size;
        }
        let mut effective_visibility = visibility.clone();
        if instance_styles.len() > scene.instances.len() {
            return Err("Style count exceeds scene instance count".into());
        }
        for (id, style) in instance_styles {
            if cancel.is_cancelled() {
                return Err("Scene render cancelled.".into());
            }
            if *id >= scene.instances.len() {
                return Err("Style references a missing scene instance".into());
            }
            validate_style(style)?;
            if style.opacity == 0. {
                effective_visibility.hidden_instances.insert(*id);
            }
        }
        let plan = draw_plan::plan_with_cancel(
            scene,
            camera,
            &effective_visibility,
            f64::from(size[0]) / f64::from(size[1]),
            draw_plan::DEFAULT_TRIANGLE_BUDGET,
            || cancel.is_cancelled(),
        )?;
        let mut counts = vec![0u32; self.draws.len()];
        let capacity = self.order.len().checked_mul(INSTANCE_BYTES).ok_or("Frame instance size overflow")?;
        let mut bytes = Vec::new();
        bytes.try_reserve(capacity).map_err(|_| "Cannot allocate frame instance data")?;
        let (eye, forward, _, _) = camera.basis();
        let mut transparent = Vec::new();
        for id in &self.order {
            if cancel.is_cancelled() {
                return Err("Scene render cancelled.".into());
            }
            if !plan.visible_instances.contains(id) {
                continue;
            }
            let instance = scene.instances.get(*id).ok_or("Invalid draw instance")?;
            let opacity = instance_styles.get(id).map_or(1., |style| style.opacity);
            let packed = encode_instance(scene, *id, frame)?;
            if opacity < 1. {
                if transparent.len() >= MAX_TRANSPARENT_DRAWS {
                    return Err(format!(
                        "The frame exceeds {MAX_TRANSPARENT_DRAWS} transparent object draws. Hide or isolate resources; no partial frame was rendered."
                    ));
                }
                let depth = math::dot(math::sub(instance.bounds.center(), eye), forward);
                if !depth.is_finite() {
                    return Err("Transparent object depth is not finite".into());
                }
                transparent.push((*id, depth, packed));
            } else {
                let count = counts.get_mut(instance.mesh).ok_or("Invalid opaque mesh group")?;
                *count = count.checked_add(1).ok_or("Opaque instance count overflow")?;
                bytes.extend_from_slice(&packed);
            }
        }
        // Global camera-depth order across all meshes. This does not resolve intersecting
        // transparent surfaces or establish native sorting/cut-cap parity.
        transparent.sort_unstable_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let mut transparent_draws = Vec::new();
        for (id, _, packed) in transparent {
            if cancel.is_cancelled() {
                return Err("Scene render cancelled.".into());
            }
            let instance = scene.instances.get(id).ok_or("Invalid transparent instance")?;
            let slot = checked_u32(bytes.len() / INSTANCE_BYTES)?;
            bytes.extend_from_slice(&packed);
            transparent_draws.push(TransparentDraw { mesh: instance.mesh, instance: slot });
        }
        self.queue.write_buffer(&self.instances, 0, &bytes);
        let matrix = camera.projection(scene.render_origin, f64::from(size[0]) / f64::from(size[1]), scene.bounds.radius())?;
        let mut uniform = Vec::with_capacity(112);
        for v in matrix {
            let packed = v as f32;
            if !packed.is_finite() {
                return Err("Camera projection exceeds graphics precision range".into());
            }
            uniform.extend_from_slice(&packed.to_le_bytes());
        }
        for color in [tokens.text_dim, tokens.accent] {
            for v in color {
                uniform.extend_from_slice(&v.to_le_bytes());
            }
        }
        uniform.extend_from_slice(
            &(if selected_resource.is_some() { u32::MAX - 1 } else { selected.map(checked_u32).transpose()?.unwrap_or(u32::MAX) }).to_le_bytes(),
        );
        uniform.extend_from_slice(&[0; 12]);
        self.queue.write_buffer(&self.uniform, 0, &uniform);
        let mut encoder = self.device.create_command_encoder(&Default::default());
        let color = tokens.canvas;
        {
            let mut pass = encoder.begin_render_pass(&w::RenderPassDescriptor {
                label: Some("Scene viewport"),
                color_attachments: &[Some(w::RenderPassColorAttachment {
                    view: &self.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: w::Operations {
                        load: w::LoadOp::Clear(w::Color { r: f64::from(color[0]), g: f64::from(color[1]), b: f64::from(color[2]), a: 1. }),
                        store: w::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(w::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(w::Operations { load: w::LoadOp::Clear(1.), store: w::StoreOp::Discard }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind, &[]);
            pass.set_vertex_buffer(0, self.vertices.slice(..));
            pass.set_vertex_buffer(1, self.instances.slice(..));
            pass.set_vertex_buffer(2, self.uvs.slice(..));
            pass.set_index_buffer(self.indices.slice(..), w::IndexFormat::Uint32);
            let mut start = 0u32;
            for (mesh_id, (d, count)) in self.draws.iter().zip(counts).enumerate() {
                pass.set_bind_group(1, self.textures.bind(mesh_id).ok_or("Missing texture bind")?, &[]);
                if cancel.is_cancelled() {
                    return Err("Scene render cancelled.".into());
                }
                let end = start.checked_add(count).ok_or("Draw instance range overflows")?;
                if count > 0 {
                    pass.draw_indexed(d.indices.clone(), d.base, start..end);
                }
                start = end;
            }
            pass.set_pipeline(&self.transparent_pipeline);
            for draw in transparent_draws {
                if cancel.is_cancelled() {
                    return Err("Scene render cancelled.".into());
                }
                let mesh = self.draws.get(draw.mesh).ok_or("Transparent draw mesh is absent")?;
                pass.set_bind_group(1, self.textures.bind(draw.mesh).ok_or("Missing texture bind")?, &[]);
                let end = draw.instance.checked_add(1).ok_or("Transparent draw range overflow")?;
                pass.draw_indexed(mesh.indices.clone(), mesh.base, draw.instance..end);
            }
        }
        let submission = self.queue.submit([encoder.finish()]);
        // Backpressure: at most one scene render is in flight, including while dragging.
        self.device
            .poll(w::PollType::Wait { submission_index: Some(submission), timeout: Some(Duration::from_secs(10)) })
            .map_err(|e| format!("GPU frame did not complete: {e}"))?;
        if let Some(e) = bounded(scope.pop(), &self.device)? {
            return Err(format!("Scene rendering failed: {e}"));
        }
        self.stats = Some(plan.stats);
        self.elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        Ok(())
    }
}

fn validate_style(style: &InstanceStyle) -> Result<(), String> {
    if !style.opacity.is_finite() || !(0.0..=1.0).contains(&style.opacity) {
        return Err("Instance opacity must be finite and within 0..1".into());
    }
    if style.color.is_some_and(|rgb| rgb.iter().any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))) {
        return Err("Instance colour must be finite and within 0..1".into());
    }
    if style.clip_plane.is_some_and(|plane| plane.iter().any(|v| !v.is_finite())) {
        return Err("Instance clip plane is not finite".into());
    }
    Ok(())
}
fn rebased_plane(plane: Option<[f64; 4]>, origin: math::Point) -> Result<[f32; 4], String> {
    let Some(mut plane) = plane else {
        return Ok([0., 0., 0., -1.]);
    };
    if !plane.iter().chain(origin.iter()).all(|v| v.is_finite()) {
        return Err("Clip plane or render origin is not finite".into());
    }
    let mut constant = *plane.get(3).ok_or("Clip plane constant is absent")?;
    for (normal, offset) in plane.iter().take(3).zip(origin) {
        constant += normal * offset;
    }
    if !constant.is_finite() {
        return Err("Rebased clip plane exceeds numeric range".into());
    }
    if let Some(value) = plane.get_mut(3) {
        *value = constant;
    }
    let scale = plane.iter().map(|v| v.abs()).fold(0.0f64, f64::max);
    if scale == 0. {
        return Ok([0.; 4]);
    }
    let mut packed = [0.; 4];
    for (value, coefficient) in packed.iter_mut().zip(plane) {
        *value = (coefficient / scale) as f32;
        if !value.is_finite() {
            return Err("Clip plane exceeds graphics range".into());
        }
    }
    Ok(packed)
}
fn original_color(scene: &Scene, id: usize, objects: bool, materials: bool) -> Option<[f32; 3]> {
    objects.then(|| scene.object_colors.get(&id).copied()).flatten().or_else(|| materials.then(|| scene.source_colors.get(&id).copied()).flatten())
}
fn encode_instance(scene: &Scene, id: usize, frame: &FrameData) -> Result<[u8; INSTANCE_BYTES], String> {
    let instance = scene.instances.get(id).ok_or("Draw instance is absent")?;
    let mesh = scene.meshes.get(instance.mesh).ok_or("Draw mesh is absent")?;
    let style = frame.instance_styles.get(&id);
    if let Some(style) = style {
        validate_style(style)?;
    }
    let mut bytes = Vec::with_capacity(INSTANCE_BYTES);
    let shift = math::sub(math::transform(&instance.matrix, mesh.origin), scene.render_origin);
    let mut matrix = instance.matrix;
    for (slot, value) in matrix.iter_mut().skip(12).take(3).zip(shift) {
        *slot = value;
    }
    for value in matrix {
        let packed = value as f32;
        if !packed.is_finite() {
            return Err("Transform exceeds graphics precision range".into());
        }
        bytes.extend_from_slice(&packed.to_le_bytes());
    }
    let picked_resource = frame.selected_resource.is_some_and(|r| instance.resources.contains(&r));
    bytes.extend_from_slice(&(if picked_resource { u32::MAX - 1 } else { checked_u32(id)? }).to_le_bytes());
    let has_phase = instance.resources.iter().filter_map(|r| frame.phases.get(r)).any(|p| *p == 1 || *p == 2);
    let explicit_rgb = ((frame.object_colors || style.is_some()) && scene.object_colors.contains_key(&id))
        || style.is_some_and(|s| s.color.is_some())
        || (style.is_none() && has_phase);
    let textured = frame.texture_preview && !explicit_rgb && scene.mesh_textures.contains_key(&instance.mesh) && scene.mesh_uvs.contains_key(&instance.mesh);
    let flags = 1u32 | if textured { 2 } else { 0 } | if frame.flip_texture_v { 4 } else { 0 };
    bytes.extend_from_slice(&flags.to_le_bytes());
    let color = if let Some(style) = style {
        // An explicit OriginalColour policy must not silently become the old green/yellow phase.
        style.color.or_else(|| original_color(scene, id, true, true))
    } else {
        let phase = instance.resources.iter().filter_map(|r| frame.phases.get(r)).filter(|p| **p == 1 || **p == 2).max().copied();
        if let Some(phase) = phase {
            let rgba = if phase == 2 { frame.tokens.warning } else { frame.tokens.success };
            Some([rgba[0], rgba[1], rgba[2]])
        } else {
            original_color(scene, id, frame.object_colors, frame.source_colors)
        }
    };
    let packed = color
        .map(|rgb| {
            rgb.iter().enumerate().fold(0xff00_0000u32, |out, (n, v)| out | ((v.clamp(0., 1.) * 255.).round() as u32).checked_shl((n * 8) as u32).unwrap_or(0))
        })
        .unwrap_or(0);
    bytes.extend_from_slice(&packed.to_le_bytes());
    bytes.extend_from_slice(&style.map_or(1., |style| style.opacity).to_le_bytes());
    let plane = rebased_plane(style.and_then(|style| style.clip_plane), scene.render_origin)?;
    for value in plane {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.try_into().map_err(|_| "Instance encoding does not match its 96-byte layout".into())
}
#[cfg(test)]
mod style_tests {
    use super::*;
    #[test]
    fn clip_plane_rebasing_preserves_original_world_half_space() -> Result<(), String> {
        let plane = rebased_plane(Some([1., 0., 0., -1005.]), [1000., 0., 0.])?;
        assert!(plane[0] * 4.9 + plane[3] < 0.);
        assert!(plane[0] * 5.1 + plane[3] > 0.);
        assert_eq!(rebased_plane(None, [0.; 3])?, [0., 0., 0., -1.]);
        Ok(())
    }
    #[test]
    fn invalid_style_scalars_reject_before_upload() {
        for opacity in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
            assert!(validate_style(&InstanceStyle { opacity, ..Default::default() }).is_err());
        }
        assert!(validate_style(&InstanceStyle { color: Some([0., f32::NAN, 1.]), ..Default::default() }).is_err());
        assert!(validate_style(&InstanceStyle { clip_plane: Some([0., 0., 1., f64::INFINITY]), ..Default::default() }).is_err());
        assert!(validate_style(&InstanceStyle::default()).is_ok());
        assert!(rebased_plane(Some([f64::MAX, 0., 0., 0.]), [f64::MAX, 0., 0.]).is_err());
    }

    #[test]
    fn instance_bytes_match_96_byte_shader_offsets_and_original_color_policy() -> Result<(), String> {
        use craft_anything_formats::scene_bundle::{self, Inputs};
        let files = craft_anything_testkit::scene_bundle::fixture();
        let get = |index| files.get(index).map(Vec::as_slice).ok_or_else(|| "test member".to_string());
        let mut scene = scene_bundle::parse(
            Inputs { manifest: get(0)?, nodes: get(1)?, meshes: get(2)?, instances: get(3)?, bindings: get(4)?, project: get(5)?, binary: get(6)? },
            |_| true,
        )
        .map_err(|e| e.to_string())?;
        scene.object_colors.insert(0, [0.25, 0.5, 0.75]);
        let mut frame = FrameData {
            texture_preview: false,
            flip_texture_v: false,
            camera: Camera::default(),
            visibility: Visibility::default(),
            selected: None,
            selected_resource: None,
            size: [32, 32],
            tokens: Tokens::default(),
            phases: BTreeMap::from([(0, 1)]),
            source_colors: false,
            object_colors: false,
            instance_styles: BTreeMap::from([(0, InstanceStyle { opacity: 0.25, clip_plane: Some([1., 0., 0., -5.]), color: None })]),
        };
        let mesh_id = scene.instances.first().ok_or("test instance")?.mesh;
        scene.mesh_textures.insert(mesh_id, "synthetic-texture".into());
        scene.mesh_uvs.insert(mesh_id, vec![]);
        frame.texture_preview = true;
        frame.flip_texture_v = true;
        let packed = encode_instance(&scene, 0, &frame)?;
        let flag = |bytes: &[u8]| -> Result<u32, String> {
            Ok(u32::from_le_bytes(bytes.get(68..72).ok_or("test flags")?.try_into().map_err(|_| "test flags width")?))
        };
        assert_eq!(flag(&packed)?, 5, "entity RGB suppresses texture, flip flag remains");
        scene.object_colors.remove(&0);
        assert_eq!(flag(&encode_instance(&scene, 0, &frame)?)?, 7, "OriginalColour admits original texture candidate");
        frame.instance_styles.insert(0, InstanceStyle { color: Some([1., 0., 0.]), ..Default::default() });
        assert_eq!(flag(&encode_instance(&scene, 0, &frame)?)?, 5, "profile RGB suppresses texture");
        frame.instance_styles.clear();
        frame.phases.clear();
        scene.object_colors.insert(0, [0.25, 0.5, 0.75]);
        frame.object_colors = false;
        assert_eq!(flag(&encode_instance(&scene, 0, &frame)?)?, 7, "3D raw texture inspection can disable object colours");
        frame.object_colors = true;
        assert_eq!(flag(&encode_instance(&scene, 0, &frame)?)?, 5, "enabled object/group colours override texture candidates");
        assert_eq!(packed.len(), 96);
        let color = u32::from_le_bytes(packed.get(72..76).ok_or("test color")?.try_into().map_err(|_| "test color width")?);
        assert_eq!(color, 0xffbf8040, "explicit OriginalColour must override legacy green phase even with checkboxes off");
        let opacity = f32::from_le_bytes(packed.get(76..80).ok_or("test opacity")?.try_into().map_err(|_| "test opacity width")?);
        assert_eq!(opacity, 0.25);
        let plane = rebased_plane(Some([1., 0., 0., -5.]), scene.render_origin)?;
        for (axis, value) in plane.iter().enumerate() {
            let at = 80 + axis * 4;
            assert_eq!(packed.get(at..at + 4), Some(value.to_le_bytes().as_slice()));
        }
        Ok(())
    }
}
