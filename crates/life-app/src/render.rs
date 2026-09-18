//! Кружки мира на видеокарте: один draw call на весь кадр.
//!
//! Каждый кружок — экземпляр квадрата; окружность вырезает фрагментный
//! шейдер по расстоянию до центра, с мягким краем в один пиксель (сглаживание
//! бесплатно). Круг — это тело: радиус — половина `size` травоядного и `DIAM`
//! хищника. Буфер кружков заливается в видеокарту только когда пришёл новый
//! кадр, а не каждый кадр окна.

use std::sync::Arc;

use eframe::egui_wgpu::{self, wgpu};
use eframe::wgpu::util::DeviceExt;

use crate::frame::Instance;

const SHADER: &str = r#"
struct U {
    // где на экране (в точках, от левого верхнего угла вьюпорта) начало координат кадра
    origin: vec2<f32>,
    // размер вьюпорта в точках
    view: vec2<f32>,
    zoom: f32,
    pixels_per_point: f32,
    // наименьший радиус в точках: мелочь при отдалении остаётся точкой, а не исчезает
    min_r: f32,
    _pad: f32,
};
@group(0) @binding(0) var<uniform> u: U;

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) r_px: f32,
    @location(2) color: vec4<f32>,
};

@vertex
fn vs_main(
    @builtin(vertex_index) vi: u32,
    @location(0) pos: vec2<f32>,
    @location(1) r: f32,
    @location(2) color: u32,
) -> VOut {
    var corners = array<vec2<f32>, 6>(
        vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(1.0, 1.0),
        vec2(-1.0, -1.0), vec2(1.0, 1.0), vec2(-1.0, 1.0),
    );
    let corner = corners[vi];
    let r_pt = max(r * u.zoom, u.min_r);
    // квадрат на пиксель больше круга: место для мягкого края
    let half = r_pt + 1.0 / u.pixels_per_point;
    let p = u.origin + pos * u.zoom + corner * half;
    var out: VOut;
    out.clip = vec4(p.x / u.view.x * 2.0 - 1.0, 1.0 - p.y / u.view.y * 2.0, 0.0, 1.0);
    out.local = corner * half * u.pixels_per_point;
    out.r_px = r_pt * u.pixels_per_point;
    out.color = unpack4x8unorm(color);
    return out;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    let d = length(in.local) - in.r_px;
    let a = clamp(0.5 - d, 0.0, 1.0) * in.color.a;
    return vec4(in.color.rgb * a, a);
}
"#;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    origin: [f32; 2],
    view: [f32; 2],
    zoom: f32,
    pixels_per_point: f32,
    min_r: f32,
    _pad: f32,
}

/// Всё, что живёт в видеокарте между кадрами.
struct Resources {
    pipeline: wgpu::RenderPipeline,
    uniforms: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    instances: wgpu::Buffer,
    capacity: usize,
    count: u32,
    /// Номер залитого кадра: тот же кадр второй раз не заливаем.
    generation: u64,
}

/// Создать конвейер и буферы; живут в ресурсах рендера egui до конца программы.
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
                attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32, 2 => Uint32],
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

/// Кружки одного кадра окна: что рисовать и где.
pub struct Circles {
    pub instances: Arc<Vec<Instance>>,
    pub generation: u64,
    /// Где на экране (в точках от левого верхнего угла вьюпорта) начало координат кадра.
    pub origin: [f32; 2],
    pub view: [f32; 2],
    pub zoom: f32,
    pub pixels_per_point: f32,
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
            min_r: 0.6 / self.pixels_per_point,
            _pad: 0.0,
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
