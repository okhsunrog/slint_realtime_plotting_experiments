use plot_wire::{CAPACITY, CHANNELS};
use wasm_bindgen::JsCast;
use wgpu::util::DeviceExt;
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    capacity: u32,
    write: u32,
    filled: u32,
    visible: u32,
    offset: u32,
    columns: u32,
    height: u32,
    channels: u32,
    y_scale: f32,
    dpr: f32,
    pad: [u32; 2],
}
pub struct Gpu {
    pub canvas: web_sys::HtmlCanvasElement,
    pub description: String,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    samples: wgpu::Buffer,
    uniform: wgpu::Buffer,
    compute: wgpu::ComputePipeline,
    draw: wgpu::RenderPipeline,
    grid: wgpu::RenderPipeline,
    compute_group: wgpu::BindGroup,
    draw_group: wgpu::BindGroup,
    pub write: usize,
    pub filled: usize,
    pub uploaded: u64,
}
impl Gpu {
    pub async fn new() -> Result<Self, String> {
        let canvas = web_sys::window()
            .unwrap()
            .document()
            .unwrap()
            .get_element_by_id("plot")
            .unwrap()
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .unwrap();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::BROWSER_WEBGPU,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))
            .map_err(|e| e.to_string())?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                ..Default::default()
            })
            .await
            .map_err(|e| e.to_string())?;
        let info = adapter.get_info();
        if info.device_type == wgpu::DeviceType::Cpu {
            return Err("Software GPU adapter rejected".into());
        }
        let description = format!(
            "{} · {:?} · {:?}",
            info.name, info.backend, info.device_type
        );
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Wavebench"),
                ..Default::default()
            })
            .await
            .map_err(|e| e.to_string())?;
        device.on_uncaptured_error(std::sync::Arc::new(|e| crate::fail(format!("WebGPU: {e}"))));
        device.set_device_lost_callback(|reason, message| {
            crate::fail(format!("GPU device lost: {reason:?}: {message}"))
        });
        let config = surface
            .get_default_config(&adapter, 1, 1)
            .ok_or("No surface configuration")?;
        let samples = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Interleaved sample ring"),
            size: (CAPACITY * CHANNELS * 4) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Viewport"),
            size: size_of::<Params>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let peaks = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Per-column extrema"),
            size: 8192 * CHANNELS as u64 * 8,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Min/max compute"),
            source: wgpu::ShaderSource::Wgsl(include_str!("reduce.wgsl").into()),
        });
        let compute = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Peak reduction"),
            layout: None,
            module: &module,
            entry_point: Some("reduce"),
            compilation_options: Default::default(),
            cache: None,
        });
        let render_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Waveform and grid"),
            source: wgpu::ShaderSource::Wgsl(include_str!("draw.wgsl").into()),
        });
        let draw_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&draw_layout)],
            immediate_size: 0,
        });
        let make_pipeline = |vertex, fragment| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(fragment),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &render_module,
                    entry_point: Some(vertex),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &render_module,
                    entry_point: Some(fragment),
                    targets: &[Some(config.format.into())],
                    compilation_options: Default::default(),
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let draw = make_pipeline("wave", "ink");
        let grid = make_pipeline("fullscreen", "grid");
        let compute_group = Self::compute_group(&device, &compute, &samples, &uniform, &peaks);
        let draw_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &draw_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: peaks.as_entire_binding(),
                },
            ],
        });
        Ok(Self {
            canvas,
            description,
            surface,
            device,
            queue,
            config,
            samples,
            uniform,
            compute,
            draw,
            grid,
            compute_group,
            draw_group,
            write: 0,
            filled: 0,
            uploaded: 0,
        })
    }
    fn compute_group(
        device: &wgpu::Device,
        pipeline: &wgpu::ComputePipeline,
        samples: &wgpu::Buffer,
        uniform: &wgpu::Buffer,
        peaks: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: samples.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: peaks.as_entire_binding(),
                },
            ],
        })
    }
    pub fn reset(&mut self) {
        self.write = 0;
        self.filled = 0;
        self.uploaded = 0;
    }
    pub fn upload(&mut self, samples: &[f32]) {
        let frames = samples.len() / CHANNELS;
        let first = frames.min(CAPACITY - self.write);
        self.queue.write_buffer(
            &self.samples,
            (self.write * CHANNELS * 4) as u64,
            bytemuck::cast_slice(&samples[..first * CHANNELS]),
        );
        if first < frames {
            self.queue.write_buffer(
                &self.samples,
                0,
                bytemuck::cast_slice(&samples[first * CHANNELS..]),
            );
        }
        self.write = (self.write + frames) % CAPACITY;
        self.filled = (self.filled + frames).min(CAPACITY);
        self.uploaded += (frames * CHANNELS * 4) as u64;
    }
    pub fn render(&mut self, visible: usize, offset: usize, amplitude: f32) -> Result<(), String> {
        let dpr = web_sys::window().unwrap().device_pixel_ratio();
        let width = ((self.canvas.client_width() as f64 * dpr).round() as u32).clamp(1, 8192);
        let height = ((self.canvas.client_height() as f64 * dpr).round() as u32).clamp(1, 4096);
        if self.config.width != width || self.config.height != height {
            self.canvas.set_width(width);
            self.canvas.set_height(height);
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
        }
        let offset = offset.min(self.filled.saturating_sub(2));
        let visible = visible.min(self.filled - offset).max(1);
        let p = Params {
            capacity: CAPACITY as u32,
            write: self.write as u32,
            filled: self.filled as u32,
            visible: visible as u32,
            offset: offset as u32,
            columns: width,
            height,
            channels: CHANNELS as u32,
            y_scale: 0.72 / amplitude,
            dpr: dpr as f32,
            pad: [0; 2],
        };
        self.queue
            .write_buffer(&self.uniform, 0, bytemuck::bytes_of(&p));
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.config);
                return Ok(());
            }
            e => return Err(format!("Surface: {e:?}")),
        };
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.compute);
            pass.set_bind_group(0, &self.compute_group, &[]);
            pass.dispatch_workgroups((width * CHANNELS as u32).div_ceil(64), 1, 1);
        }
        let view = frame.texture.create_view(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_bind_group(0, &self.draw_group, &[]);
            pass.set_pipeline(&self.grid);
            pass.draw(0..3, 0..1);
            if self.filled > 0 {
                pass.set_pipeline(&self.draw);
                pass.draw(0..6, 0..width * CHANNELS as u32);
            }
        }
        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);
        Ok(())
    }
    pub async fn verify_compute(&self) -> Result<String, String> {
        // Read back the real reduction shader, including a wrapped ring and narrow spike.
        let samples = (0..256 * CHANNELS)
            .map(|i| {
                if i == 39 {
                    7.
                } else {
                    ((i as f32) * 0.17).sin()
                }
            })
            .collect::<Vec<_>>();
        let input = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(&samples),
                usage: wgpu::BufferUsages::STORAGE,
            });
        let p = Params {
            capacity: 256,
            write: 37,
            filled: 256,
            visible: 221,
            offset: 11,
            columns: 17,
            height: 1,
            channels: 3,
            y_scale: 1.,
            dpr: 1.,
            pad: [0; 2],
        };
        let uniform = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::bytes_of(&p),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let size = 17 * 3 * 8;
        let output = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let read = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let group = Self::compute_group(&self.device, &self.compute, &input, &uniform, &output);
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.compute);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&output, 0, &read, 0, size);
        self.queue.submit([encoder.finish()]);
        let (tx, rx) = futures_channel::oneshot::channel();
        read.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        rx.await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        let mapped = read
            .slice(..)
            .get_mapped_range()
            .map_err(|e| e.to_string())?;
        let values: &[f32] = bytemuck::cast_slice(&mapped);
        for col in 0..17 {
            for ch in 0..3 {
                let lo = col * 221 / 17;
                let hi = ((col + 1) * 221 / 17).max(lo + 1).min(221);
                let start = (37 + 256 - 221 - 11) % 256;
                let mut mn = f32::INFINITY;
                let mut mx = f32::NEG_INFINITY;
                for i in lo..hi {
                    let v = samples[((start + i) % 256) * 3 + ch];
                    mn = mn.min(v);
                    mx = mx.max(v);
                }
                let index = (col * 3 + ch) * 2;
                if (values[index] - mn).abs() > 1e-6 || (values[index + 1] - mx).abs() > 1e-6 {
                    return Err(format!("Compute mismatch at column {col}, channel {ch}"));
                }
            }
        }
        drop(mapped);
        read.unmap();
        Ok("PASS · 102 extrema · wrapped ring + spike".into())
    }
}
