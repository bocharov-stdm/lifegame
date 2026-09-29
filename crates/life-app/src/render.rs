//! Creatures on the graphics card: one draw call for the whole frame.
//!
//! Every creature is an instance of a square; the fragment shader (`creatures.wgsl`) cuts the
//! shape out by the distance to the edge, with a soft one-pixel edge. The circle is the body:
//! the radius is half the creature's `size`.
//!
//! What the shader does so that the world does not flicker:
//! - it draws a creature between the positions of the previous and the new frame (`k` is the
//!   share of the way), not in jumps from frame to frame;
//! - a newborn grows out of a point, an eaten one shrinks, one that starved to death goes grey
//!   and fades (the circle's age plus `since`, the time since it was assembled);
//! - smaller than a pixel it draws a pixel, but dimmer by area: a dot in motion neither flashes
//!   nor goes out;
//! - it draws the rim, the fullness core and the eye only when the creature is bigger than a few
//!   pixels on the screen: small details would shimmer.
//!
//! The buffer of circles is uploaded to the graphics card only when a new frame has come.

use std::sync::Arc;

use eframe::egui_wgpu::{self, wgpu};
use eframe::wgpu::util::DeviceExt;

use crate::frame::Instance;

const SHADER: &str = include_str!("creatures.wgsl");

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    origin: [f32; 2],
    view: [f32; 2],
    zoom: f32,
    pixels_per_point: f32,
    k: f32,
    since: f32,
    time: f32,
    highlight: u32,
    pad: [f32; 2],
}

/// Everything that lives on the graphics card between frames.
struct Resources {
    pipeline: wgpu::RenderPipeline,
    uniforms: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    instances: wgpu::Buffer,
    capacity: usize,
    count: u32,
    /// The number of the frame uploaded: the same frame is not uploaded twice.
    generation: u64,
}

/// Create the pipeline and the buffers; they live in egui's render resources to the end of the program.
pub fn init(render_state: &egui_wgpu::RenderState) {
    let device = &render_state.device;
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("кружки"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("кружки"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("кружки"),
        bind_group_layouts: &[Some(&bind_layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("кружки"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Instance>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &wgpu::vertex_attr_array![
                    0 => Float32x2, 1 => Float32x2, 2 => Float32, 3 => Uint32, 4 => Float32, 5 => Uint32
                ],
            })],
            compilation_options: Default::default(),
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: render_state.target_format,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        multiview_mask: None,
        cache: None,
    });
    let uniforms = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("кружки: параметры"),
        contents: bytemuck::bytes_of(&<Uniforms as bytemuck::Zeroable>::zeroed()),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("кружки"),
        layout: &bind_layout,
        entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniforms.as_entire_binding() }],
    });
    let capacity = 1024;
    let instances = instance_buffer(device, capacity);
    render_state.renderer.write().callback_resources.insert(Resources {
        pipeline,
        uniforms,
        bind_group,
        instances,
        capacity,
        count: 0,
        generation: u64::MAX,
    });
}

fn instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("кружки: экземпляры"),
        size: (capacity * std::mem::size_of::<Instance>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// The circles of one window frame: what to draw and where.
pub struct Circles {
    pub instances: Arc<Vec<Instance>>,
    pub generation: u64,
    /// Where on the screen (in points from the viewport's top left corner) the frame's origin lies.
    pub origin: [f32; 2],
    pub view: [f32; 2],
    pub zoom: f32,
    pub pixels_per_point: f32,
    /// The share of the way from the previous frame's position to the new one (0‒1).
    pub k: f32,
    /// Seconds since the frame was assembled: added to the circles' age.
    pub since: f32,
    /// Seconds since the program began, modulo: the phase of the proboscis's «gulps».
    pub time: f32,
    /// Diets to highlight, a bit per diet in `Diet` order; 0 — none, everyone drawn as usual.
    pub highlight: u32,
}

impl egui_wgpu::CallbackTrait for Circles {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen: &egui_wgpu::ScreenDescriptor,
        _encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let Some(res) = resources.get_mut::<Resources>() else { return Vec::new() };
        let u = Uniforms {
            origin: self.origin,
            view: self.view,
            zoom: self.zoom,
            pixels_per_point: self.pixels_per_point,
            k: self.k,
            since: self.since,
            time: self.time,
            highlight: self.highlight,
            pad: [0.0; 2],
        };
        queue.write_buffer(&res.uniforms, 0, bytemuck::bytes_of(&u));
        if res.generation != self.generation {
            let n = self.instances.len();
            if n > res.capacity {
                res.capacity = n.next_power_of_two();
                res.instances = instance_buffer(device, res.capacity);
            }
            if n > 0 {
                queue.write_buffer(&res.instances, 0, bytemuck::cast_slice(&self.instances));
            }
            res.count = n as u32;
            res.generation = self.generation;
        }
        Vec::new()
    }

    fn paint(
        &self,
        _info: eframe::egui::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        let Some(res) = resources.get::<Resources>() else { return };
        if res.count == 0 {
            return;
        }
        pass.set_pipeline(&res.pipeline);
        pass.set_bind_group(0, &res.bind_group, &[]);
        pass.set_vertex_buffer(0, res.instances.slice(..));
        pass.draw(0..6, 0..res.count);
    }
}
