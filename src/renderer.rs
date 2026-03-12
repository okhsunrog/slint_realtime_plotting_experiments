use crate::data_gen::{AdcSimulator, NUM_SAMPLES};
use slint::wgpu_28::wgpu;

const SAMPLES_BUFFER_SIZE: u64 = (NUM_SAMPLES * std::mem::size_of::<f32>()) as u64;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PlotParams {
    pub write_pos: u32,
    pub num_samples: u32,
    pub y_min: f32,
    pub y_max: f32,
    pub grid_x_divisions: f32,
    pub grid_y_divisions: f32,
    pub time_val: f32,
    pub _padding: f32,
}

pub struct PlotRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    texture: wgpu::Texture,
    samples_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    start_time: std::time::Instant,
}

impl PlotRenderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("plot_shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(include_str!(
                "shader.wgsl"
            ))),
        });

        let samples_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("samples_buffer"),
            size: SAMPLES_BUFFER_SIZE,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("plot_bind_group_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("plot_bind_group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: samples_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("plot_pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout],
            immediate_size: std::mem::size_of::<PlotParams>() as u32,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("plot_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::TextureFormat::Rgba8UnormSrgb.into())],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let texture = Self::create_texture(device, 320, 200);

        Self {
            device: device.clone(),
            queue: queue.clone(),
            pipeline,
            texture,
            samples_buffer,
            bind_group,
            start_time: std::time::Instant::now(),
        }
    }

    fn create_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("plot_texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
    }

    pub fn render(&mut self, simulator: &AdcSimulator, width: u32, height: u32) -> wgpu::Texture {
        let width = width.max(1);
        let height = height.max(1);

        if self.texture.size().width != width || self.texture.size().height != height {
            self.texture = Self::create_texture(&self.device, width, height);
        }

        self.queue.write_buffer(
            &self.samples_buffer,
            0,
            bytemuck::cast_slice(&simulator.buffer),
        );

        let amplitude_max = simulator
            .buffer
            .iter()
            .fold(0.0f32, |acc, &x| acc.max(x.abs()));
        let y_range = (amplitude_max * 1.2).max(1.0);

        let params = PlotParams {
            write_pos: simulator.write_pos,
            num_samples: NUM_SAMPLES as u32,
            y_min: -y_range,
            y_max: y_range,
            grid_x_divisions: 10.0,
            grid_y_divisions: 8.0,
            time_val: self.start_time.elapsed().as_secs_f32(),
            _padding: 0.0,
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("plot_encoder"),
            });
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("plot_render_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self
                        .texture
                        .create_view(&wgpu::TextureViewDescriptor::default()),
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.06,
                            g: 0.06,
                            b: 0.12,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            rpass.set_pipeline(&self.pipeline);
            rpass.set_bind_group(0, &self.bind_group, &[]);
            rpass.set_immediates(0, bytemuck::bytes_of(&params));
            rpass.draw(0..3, 0..1);
        }

        self.queue.submit(Some(encoder.finish()));
        self.texture.clone()
    }
}
