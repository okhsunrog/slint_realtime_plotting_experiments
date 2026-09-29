//! Vello rasterizers for the plot: same inputs and output texture as the
//! hand-written wgpu pipelines, so they can be compared head to head.
//!
//! Vello takes vector paths, so the CPU turns the ring buffer into one path
//! per channel each frame: a polyline through the samples when there are few
//! per pixel, or a min/max envelope polygon per column otherwise. Three
//! Vello renderers consume those same paths:
//!
//! - compute (`vello`): everything but path encoding runs in compute shaders
//! - GPU (`vello_gpu`): sparse strips built on the CPU, drawn by render passes
//! - CPU (`vello_cpu`): multithreaded SIMD raster, uploaded as a texture

use crate::renderer::PlotBackend;
use slint::wgpu_30::wgpu;
use vello::kurbo::{Affine, BezPath, Cap, Join, Stroke};
use vello::peniko::{Color, Fill};
use vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene};
use vello_cpu::color::{AlphaColor as CpuColor, Srgb as CpuSrgb};
use vello_cpu::{
    Pixmap, RenderContext as CpuContext, Resources as CpuResources,
};
use vello_gpu::color::{AlphaColor as GpuColor, Srgb as GpuSrgb};
use vello_gpu::{
    ClearSettings, RenderSize, RenderTargetConfig, Renderer as GpuRenderer,
    Resources as GpuResources, Scene as GpuScene, TargetInit, TextureBindings,
};

pub(crate) enum VelloBackend {
    Compute {
        renderer: Renderer,
        scene: Scene,
    },
    Gpu(GpuState),
    Cpu(CpuState),
}

pub(crate) struct GpuState {
    renderer: GpuRenderer,
    resources: GpuResources,
    scene: GpuScene,
    depth: wgpu::TextureView,
    size: (u32, u32),
}

pub(crate) struct CpuState {
    context: CpuContext,
    resources: CpuResources,
    pixmap: Pixmap,
}

/// Everything needed to draw one frame; mirrors the shader's `PlotParams`.
pub(crate) struct Frame<'a> {
    /// Full ring, `capacity * num_channels` interleaved values.
    pub samples: &'a [f32],
    pub write_pos: usize,
    pub capacity: usize,
    pub num_channels: usize,
    pub visible: usize,
    pub view_offset: usize,
    pub width: u32,
    pub height: u32,
    pub y_min: f32,
    pub y_max: f32,
    pub scale: f32,
    pub colors: &'a [[f32; 4]],
    /// Draw min/max envelopes per column instead of a polyline.
    pub peak: bool,
}

/// One channel's geometry: an outline, plus a fill in peak mode.
struct ChannelPath {
    path: BezPath,
    color: [f32; 4],
    filled: bool,
}

impl VelloBackend {
    pub fn new(backend: PlotBackend, device: &wgpu::Device) -> Self {
        match backend {
            PlotBackend::Vello => Self::Compute {
                renderer: Renderer::new(
                    device,
                    RendererOptions {
                        antialiasing_support: AaSupport::area_only(),
                        ..Default::default()
                    },
                )
                .expect("failed to create Vello renderer"),
                scene: Scene::new(),
            },
            PlotBackend::VelloGpu => {
                Self::Gpu(GpuState::new(device, 1, 1))
            }
            PlotBackend::VelloCpu => Self::Cpu(CpuState::new(1, 1)),
            PlotBackend::Wgpu => unreachable!("not a Vello backend"),
        }
    }

    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
        frame: &Frame,
    ) {
        let paths = channel_paths(frame);
        let stroke_width = f64::from(1.5 * frame.scale);
        let style = Stroke::new(stroke_width)
            .with_join(Join::Round)
            .with_caps(Cap::Round);
        match self {
            Self::Compute { renderer, scene } => {
                scene.reset();
                for p in &paths {
                    let color = Color::new(p.color);
                    if p.filled {
                        scene.fill(Fill::NonZero, Affine::IDENTITY, color, None, &p.path);
                    }
                    scene.stroke(&style, Affine::IDENTITY, color, None, &p.path);
                }
                let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                renderer
                    .render_to_texture(
                        device,
                        queue,
                        scene,
                        &view,
                        &RenderParams {
                            base_color: Color::TRANSPARENT,
                            width: frame.width,
                            height: frame.height,
                            antialiasing_method: AaConfig::Area,
                        },
                    )
                    .expect("Vello render failed");
            }
            Self::Gpu(state) => {
                if state.size != (frame.width, frame.height) {
                    *state = GpuState::new(device, frame.width, frame.height);
                }
                state.scene.reset();
                state.scene.set_stroke(style);
                for p in &paths {
                    let color = GpuColor::<GpuSrgb>::new(p.color);
                    state.scene.set_paint(color);
                    if p.filled {
                        state.scene.fill_path(&p.path);
                    }
                    state.scene.stroke_path(&p.path);
                }
                let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                let mut encoder = device.create_command_encoder(&Default::default());
                state
                    .renderer
                    .render(
                        &state.scene,
                        &mut state.resources,
                        device,
                        queue,
                        &mut encoder,
                        &RenderSize {
                            width: frame.width as u16,
                            height: frame.height as u16,
                        },
                        &view,
                        Some(&state.depth),
                        &TextureBindings::new(),
                        TargetInit::Clear(ClearSettings::default()),
                    )
                    .expect("Vello GPU render failed");
                queue.submit(Some(encoder.finish()));
            }
            Self::Cpu(state) => {
                if state.pixmap.width() as u32 != frame.width
                    || state.pixmap.height() as u32 != frame.height
                {
                    *state = CpuState::new(frame.width as u16, frame.height as u16);
                }
                state.context.reset();
                state.context.set_stroke(style);
                for p in &paths {
                    let color = CpuColor::<CpuSrgb>::new(p.color);
                    state.context.set_paint(color);
                    if p.filled {
                        state.context.fill_path(&p.path);
                    }
                    state.context.stroke_path(&p.path);
                }
                state.context.flush();
                state
                    .context
                    .render(&mut state.pixmap, &mut state.resources);
                queue.write_texture(
                    texture.as_image_copy(),
                    state.pixmap.data_as_u8_slice(),
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(frame.width * 4),
                        rows_per_image: None,
                    },
                    texture.size(),
                );
            }
        }
    }
}

impl GpuState {
    fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let (w, h) = (width.max(1) as u16, height.max(1) as u16);
        let (renderer, resources) = GpuRenderer::new(
            device,
            &RenderTargetConfig {
                format: wgpu::TextureFormat::Rgba8Unorm,
                width: w,
                height: h,
            },
        );
        Self {
            renderer,
            resources,
            scene: GpuScene::new(w, h),
            depth: GpuRenderer::create_depth_texture_view(
                device,
                &RenderSize {
                    width: w,
                    height: h,
                },
            ),
            size: (width, height),
        }
    }
}

impl CpuState {
    fn new(width: u16, height: u16) -> Self {
        let (width, height) = (width.max(1), height.max(1));
        Self {
            context: CpuContext::new(width, height),
            resources: CpuResources::new(),
            pixmap: Pixmap::new(width, height),
        }
    }
}

fn channel_paths(frame: &Frame) -> Vec<ChannelPath> {
    (0..frame.num_channels)
        .map(|ch| ChannelPath {
            path: if frame.peak {
                envelope_path(frame, ch)
            } else {
                line_path(frame, ch)
            },
            color: frame.colors[ch],
            filled: frame.peak,
        })
        .collect()
}

/// Sample `index` (0 = oldest visible) of `channel`, honouring ring wrap.
fn sample(frame: &Frame, channel: usize, index: usize) -> f32 {
    let start = (frame.write_pos + frame.capacity - frame.visible - frame.view_offset)
        % frame.capacity;
    let actual = (start + index) % frame.capacity;
    frame.samples[actual * frame.num_channels + channel]
}

fn value_to_y(frame: &Frame, v: f32) -> f64 {
    let t = (v - frame.y_min) / (frame.y_max - frame.y_min);
    f64::from((1.0 - t) * frame.height as f32)
}

/// One polyline through every visible sample; non-finite values split it.
fn line_path(frame: &Frame, ch: usize) -> BezPath {
    let mut path = BezPath::new();
    let segments = (frame.visible - 1) as f64;
    let mut pen_down = false;
    for i in 0..frame.visible {
        let v = sample(frame, ch, i);
        if !v.is_finite() {
            pen_down = false;
            continue;
        }
        let p = (
            i as f64 / segments * f64::from(frame.width),
            value_to_y(frame, v),
        );
        if pen_down {
            path.line_to(p);
        } else {
            path.move_to(p);
            pen_down = true;
        }
    }
    path
}

/// Closed polygon per run of valid columns: along each column's max, then
/// back along its min. Column ranges match `reduce.wgsl`.
fn envelope_path(frame: &Frame, ch: usize) -> BezPath {
    let width = frame.width as usize;
    let last_index = frame.visible - 1;
    let half_span = (frame.visible as f32 / width as f32 * 0.5).max(0.5);
    let mut path = BezPath::new();
    let mut run_hi: Vec<(f64, f64)> = Vec::new();
    let mut run_lo: Vec<(f64, f64)> = Vec::new();
    let flush = |path: &mut BezPath, hi: &mut Vec<(f64, f64)>, lo: &mut Vec<(f64, f64)>| {
        if let Some(&first) = hi.first() {
            path.move_to(first);
            for &p in &hi[1..] {
                path.line_to(p);
            }
            for &p in lo.iter().rev() {
                path.line_to(p);
            }
            path.close_path();
        }
        hi.clear();
        lo.clear();
    };
    for column in 0..width {
        let center = (column as f32 + 0.5) / width as f32 * last_index as f32;
        let first = (center - half_span).floor().max(0.0) as usize;
        let last = ((center + half_span).ceil() as usize).min(last_index);
        let mut lo = f32::INFINITY;
        let mut hi = f32::NEG_INFINITY;
        for i in first..=last {
            let v = sample(frame, ch, i);
            if v.is_finite() {
                lo = lo.min(v);
                hi = hi.max(v);
            }
        }
        if hi < lo {
            flush(&mut path, &mut run_hi, &mut run_lo);
            continue;
        }
        let x = column as f64 + 0.5;
        run_hi.push((x, value_to_y(frame, hi)));
        run_lo.push((x, value_to_y(frame, lo)));
    }
    flush(&mut path, &mut run_hi, &mut run_lo);
    path
}
