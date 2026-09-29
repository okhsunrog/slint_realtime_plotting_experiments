//! Vello rasterizer for the plot: same inputs and output texture as the
//! hand-written wgpu pipelines, so the two can be compared head to head.
//!
//! Vello takes vector paths, so the CPU turns the ring buffer into one path
//! per channel each frame: a polyline through the samples when there are few
//! per pixel, or a min/max envelope polygon per column otherwise.

use slint::wgpu_30::wgpu;
use vello::kurbo::{Affine, BezPath, Cap, Join, Stroke};
use vello::peniko::{Color, Fill};
use vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene};

pub(crate) struct VelloBackend {
    renderer: Renderer,
    scene: Scene,
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

impl VelloBackend {
    pub fn new(device: &wgpu::Device) -> Self {
        let renderer = Renderer::new(
            device,
            RendererOptions {
                antialiasing_support: AaSupport::area_only(),
                ..Default::default()
            },
        )
        .expect("failed to create Vello renderer");
        Self {
            renderer,
            scene: Scene::new(),
        }
    }

    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        frame: &Frame,
    ) {
        self.scene.reset();
        let style = Stroke::new(f64::from(1.5 * frame.scale))
            .with_join(Join::Round)
            .with_caps(Cap::Round);
        for ch in 0..frame.num_channels {
            let color = Color::new(frame.colors[ch]);
            let path = if frame.peak {
                envelope_path(frame, ch)
            } else {
                line_path(frame, ch)
            };
            if frame.peak {
                self.scene
                    .fill(Fill::NonZero, Affine::IDENTITY, color, None, &path);
            }
            self.scene
                .stroke(&style, Affine::IDENTITY, color, None, &path);
        }
        self.renderer
            .render_to_texture(
                device,
                queue,
                &self.scene,
                view,
                &RenderParams {
                    base_color: Color::TRANSPARENT,
                    width: frame.width,
                    height: frame.height,
                    antialiasing_method: AaConfig::Area,
                },
            )
            .expect("Vello render failed");
    }
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
