//! Vello rasterizers for the plot: same inputs and output texture as the
//! hand-written wgpu pipelines, so they can be compared head to head.
//!
//! Vello takes vector paths, so the CPU turns the ring buffer into one path
//! per channel each frame: a polyline through the samples when there are few
//! per pixel, or a min/max envelope polygon per column otherwise. Three
//! Vello renderers consume those paths:
//!
//! - compute (`vello`): everything but path encoding runs in compute shaders
//! - GPU (`vello_gpu`): sparse strips built on the CPU, drawn by render passes
//! - CPU (`vello_cpu`): multithreaded SIMD raster, uploaded as a texture

use crate::renderer::PlotBackend;
use slint::wgpu_30::wgpu;
use std::time::{Duration, Instant};
use vello::kurbo::{Affine, BezPath, Cap, Join, Stroke};
use vello::low_level::vello_encoding::{Encoding, PathEncoder, Transform};
use vello::peniko::{Color, Fill};
use vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene};
use vello_cpu::color::{AlphaColor as CpuColor, Srgb as CpuSrgb};
use vello_cpu::{Pixmap, RenderContext as CpuContext, Resources as CpuResources};
use vello_gpu::color::{AlphaColor as GpuColor, Srgb as GpuSrgb};
#[cfg(feature = "vello-profile")]
use wgpu_profiler::GpuTimerQueryResult;
use vello_gpu::{
    ClearSettings, RenderSize, RenderTargetConfig, Renderer as GpuRenderer,
    Resources as GpuResources, Scene as GpuScene, TargetInit, TextureBindings,
};

pub(crate) enum VelloBackend {
    Compute(Box<ComputeState>),
    Gpu(GpuState),
    Cpu(CpuState),
}

/// How the compute renderer's scene is built.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ComputeOptions {
    /// Build `kurbo::BezPath`s through every sample and go through
    /// `Scene::fill`/`stroke`; the other options then do not apply.
    /// Otherwise line segments are written straight into the encoding.
    pub kurbo: bool,
    pub line: LineGeometry,
    /// Peak mode: grow the envelope polygon by half the line width on the
    /// CPU and fill it once, instead of filling it and stroking its outline
    /// (same area, a fraction of the segments).
    pub outset: bool,
    /// With `outset`: cut concave corners (see `outset_polygon`).
    pub merge_concave: bool,
}

/// Polyline vertices in line mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LineGeometry {
    /// Every visible sample.
    All,
    /// Per pixel column: first, min, max and last sample, in time order.
    M4,
    /// Per pixel column: min and max sample, in time order. Cheaper than
    /// M4 but it changes which vertices neighbouring columns connect.
    #[cfg_attr(not(test), allow(dead_code, reason = "benchmark alternative"))]
    MinMax,
}

impl ComputeOptions {
    /// Segments written straight into the encoding; at most four vertices
    /// per pixel column in line mode; the envelope and its outline stroke as
    /// one CPU-grown polygon in peak mode.
    pub const OPTIMIZED: Self = Self {
        kurbo: false,
        line: LineGeometry::M4,
        outset: true,
        merge_concave: true,
    };

    /// The original integration: kurbo paths through all samples, fill plus
    /// stroke of the envelope.
    pub const NAIVE: Self = Self {
        kurbo: true,
        line: LineGeometry::All,
        outset: false,
        merge_concave: false,
    };
}

pub(crate) struct ComputeState {
    renderer: Renderer,
    scene: Scene,
    pub options: ComputeOptions,
    scratch: Scratch,
    /// CPU time of the last frame: scene building, and `render_to_texture`
    /// (resolve, recording, bind groups, upload, submit).
    pub last_build: Duration,
    pub last_submit: Duration,
    /// Per-stage GPU times (milliseconds), one entry per profiled frame.
    #[cfg(feature = "vello-profile")]
    pub gpu_stages: Vec<(String, Vec<f64>)>,
    /// CPU build and submit times (milliseconds) per frame.
    #[cfg(feature = "vello-profile")]
    pub cpu_times: Vec<(f64, f64)>,
}

/// Reused per-frame buffers.
#[derive(Default)]
struct Scratch {
    values: Vec<f32>,
    points: Vec<[f32; 2]>,
    runs: Vec<usize>,
    hi: Vec<f32>,
    lo: Vec<f32>,
    polygon: Vec<[f32; 2]>,
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
            PlotBackend::Vello | PlotBackend::VelloNaive => {
                Self::Compute(Box::new(ComputeState {
                    renderer: Renderer::new(
                        device,
                        RendererOptions {
                            antialiasing_support: AaSupport::area_only(),
                            ..Default::default()
                        },
                    )
                    .expect("failed to create Vello renderer"),
                    scene: Scene::new(),
                    options: if backend == PlotBackend::VelloNaive {
                        ComputeOptions::NAIVE
                    } else {
                        ComputeOptions::OPTIMIZED
                    },
                    scratch: Scratch::default(),
                    last_build: Duration::ZERO,
                    last_submit: Duration::ZERO,
                    #[cfg(feature = "vello-profile")]
                    gpu_stages: Vec::new(),
                    #[cfg(feature = "vello-profile")]
                    cpu_times: Vec::new(),
                }))
            }
            PlotBackend::VelloGpu => Self::Gpu(GpuState::new(device, 1, 1)),
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
        let style = line_style(frame.scale);
        match self {
            Self::Compute(state) => state.render(device, queue, texture, frame),
            Self::Gpu(state) => {
                let paths = channel_paths(frame);
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
                let paths = channel_paths(frame);
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

    #[cfg(any(test, feature = "vello-profile"))]
    pub fn compute_mut(&mut self) -> Option<&mut ComputeState> {
        match self {
            Self::Compute(state) => Some(state),
            _ => None,
        }
    }
}

/// Clears the plot texture to transparent with a render pass. Only for the
/// experimental Vello patch whose fine stage leaves empty tiles untouched.
#[cfg(feature = "vello-skip-empty-tiles")]
fn clear_target(device: &wgpu::Device, queue: &wgpu::Queue, view: &wgpu::TextureView) {
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("plot_clear"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    queue.submit(Some(encoder.finish()));
}

/// The waveform line: 1.5 logical pixels, round joins and caps.
fn line_style(scale: f32) -> Stroke {
    Stroke::new(f64::from(1.5 * scale))
        .with_join(Join::Round)
        .with_caps(Cap::Round)
}

impl ComputeState {
    fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
        frame: &Frame,
    ) {
        let t0 = Instant::now();
        self.scene.reset();
        let options = self.options;
        let style = line_style(frame.scale);
        if options.kurbo {
            for p in &channel_paths(frame) {
                let color = Color::new(p.color);
                if p.filled {
                    self.scene
                        .fill(Fill::NonZero, Affine::IDENTITY, color, None, &p.path);
                }
                self.scene
                    .stroke(&style, Affine::IDENTITY, color, None, &p.path);
            }
        } else {
            let encoding = self.scene.encoding_mut();
            let half_width = 0.75 * frame.scale;
            for ch in 0..frame.num_channels {
                let color = Color::new(frame.colors[ch]);
                let s = &mut self.scratch;
                channel_values(frame, ch, &mut s.values);
                if !frame.peak {
                    line_points(frame, &s.values, options.line, &mut s.points, &mut s.runs);
                    encode_path(encoding, Some(&style), color, |path| {
                        for run in s.runs.windows(2) {
                            polyline(path, &s.points[run[0]..run[1]]);
                        }
                    });
                    continue;
                }
                envelope(frame, &s.values, &mut s.hi, &mut s.lo);
                if options.outset {
                    encode_path(encoding, None, color, |path| {
                        for_each_run(&s.hi, |run| {
                            envelope_polygon(&s.hi, &s.lo, run, &mut s.polygon);
                            outset_polygon(path, &s.polygon, half_width, options.merge_concave);
                        });
                    });
                } else {
                    let mut outline = |path: &mut PathEncoder<'_>| {
                        for_each_run(&s.hi, |run| {
                            envelope_polygon(&s.hi, &s.lo, run, &mut s.polygon);
                            polygon(path, &s.polygon);
                        });
                    };
                    encode_path(encoding, None, color, &mut outline);
                    encode_path(encoding, Some(&style), color, &mut outline);
                }
            }
        }
        let t1 = Instant::now();
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        #[cfg(feature = "vello-skip-empty-tiles")]
        clear_target(device, queue, &view);
        self.renderer
            .render_to_texture(
                device,
                queue,
                &self.scene,
                &view,
                &RenderParams {
                    base_color: Color::TRANSPARENT,
                    width: frame.width,
                    height: frame.height,
                    antialiasing_method: AaConfig::Area,
                },
            )
            .expect("Vello render failed");
        self.last_build = t1 - t0;
        self.last_submit = t1.elapsed();
        #[cfg(feature = "vello-profile")]
        self.cpu_times.push((
            self.last_build.as_secs_f64() * 1000.0,
            self.last_submit.as_secs_f64() * 1000.0,
        ));
        #[cfg(feature = "vello-profile")]
        if let Some(results) = self.renderer.profile_result.take() {
            // The sum of the dispatches excludes gaps in which the GPU ran
            // other clients' work (the desktop compositor, a browser).
            let sum: f64 = results
                .iter()
                .flat_map(|r| &r.nested_queries)
                .filter_map(|q| q.time.as_ref())
                .map(|t| (t.end - t.start) * 1000.0)
                .sum();
            accumulate_stages(&mut self.gpu_stages, &results);
            match self.gpu_stages.iter_mut().find(|(label, _)| label == "sum") {
                Some((_, times)) => times.push(sum),
                None => self.gpu_stages.insert(0, ("sum".into(), vec![sum])),
            }
        }
    }

    /// Number of encoded path segments in the last scene.
    #[cfg(any(test, feature = "vello-profile"))]
    pub fn segments(&self) -> u32 {
        self.scene.encoding().n_path_segments
    }

    /// Since the last report, as `median(p10-p90)` over frames: CPU build
    /// and submit, then per-stage GPU times; clears the collected samples.
    #[cfg(feature = "vello-profile")]
    pub fn take_report(&mut self) -> String {
        let (mut build, mut submit): (Vec<f64>, Vec<f64>) = self.cpu_times.drain(..).unzip();
        let mut out = format!(
            "frames {} segs {} cpu build={} submit={} | gpu",
            build.len(),
            self.segments(),
            spread(&mut build),
            spread(&mut submit)
        );
        for (label, times) in &mut self.gpu_stages {
            out += &format!(" {}={}", label.trim_start_matches("vello."), spread(times));
        }
        self.gpu_stages.clear();
        out
    }
}

#[cfg(feature = "vello-profile")]
pub(crate) fn median(values: &mut [f64]) -> f64 {
    if values.is_empty() {
        return f64::NAN;
    }
    values.sort_by(f64::total_cmp);
    values[values.len() / 2]
}

/// `median(p10-p90)` of `values`, in the units given.
#[cfg(feature = "vello-profile")]
pub(crate) fn spread(values: &mut [f64]) -> String {
    let m = median(values);
    if values.is_empty() {
        return "-".into();
    }
    let at = |p: f64| values[((values.len() - 1) as f64 * p).round() as usize];
    format!("{m:.3}({:.3}-{:.3})", at(0.1), at(0.9))
}

#[cfg(feature = "vello-profile")]
fn accumulate_stages(stages: &mut Vec<(String, Vec<f64>)>, results: &[GpuTimerQueryResult]) {
    for result in results {
        if let Some(time) = &result.time {
            let ms = (time.end - time.start) * 1000.0;
            match stages.iter_mut().find(|(label, _)| *label == result.label) {
                Some((_, times)) => times.push(ms),
                None => stages.push((result.label.clone(), vec![ms])),
            }
        }
        accumulate_stages(stages, &result.nested_queries);
    }
}

/// Encodes one draw object: a fill (`stroke == None`) or a stroke of the
/// segments that `build` emits, in a solid colour.
fn encode_path(
    encoding: &mut Encoding,
    stroke: Option<&Stroke>,
    color: Color,
    build: impl FnOnce(&mut PathEncoder<'_>),
) {
    encoding.encode_transform(Transform::IDENTITY);
    match stroke {
        Some(style) => {
            let encoded = encoding.encode_stroke_style(style);
            debug_assert!(encoded, "stroke width must be non-zero");
        }
        None => encoding.encode_fill_style(Fill::NonZero),
    }
    let mut path = encoding.encode_path(stroke.is_none());
    build(&mut path);
    if path.finish(true) != 0 {
        encoding.encode_brush(color, 1.0);
    }
}

fn polyline(path: &mut PathEncoder<'_>, points: &[[f32; 2]]) {
    if let Some((first, rest)) = points.split_first() {
        path.move_to(first[0], first[1]);
        for p in rest {
            path.line_to(p[0], p[1]);
        }
    }
}

fn polygon(path: &mut PathEncoder<'_>, points: &[[f32; 2]]) {
    polyline(path, points);
    path.close();
}

/// Calls `f` with each maximal range of columns that have data.
fn for_each_run(hi: &[f32], mut f: impl FnMut(std::ops::Range<usize>)) {
    let mut start = None;
    for (i, v) in hi.iter().enumerate() {
        match (v.is_nan(), start) {
            (false, None) => start = Some(i),
            (true, Some(s)) => {
                f(s..i);
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        f(s..hi.len());
    }
}

/// The visible window of one channel, oldest first, unwrapped from the ring,
/// with every non-finite value (gaps) turned into NaN.
fn channel_values(frame: &Frame, ch: usize, out: &mut Vec<f32>) {
    out.clear();
    let start = (frame.write_pos + frame.capacity - frame.visible - frame.view_offset)
        % frame.capacity;
    let n = frame.num_channels;
    let first = (frame.capacity - start).min(frame.visible);
    let sanitize = |v: &f32| if v.is_finite() { *v } else { f32::NAN };
    out.extend(
        frame.samples[start * n..(start + first) * n]
            .iter()
            .skip(ch)
            .step_by(n)
            .map(sanitize),
    );
    out.extend(
        frame.samples[..(frame.visible - first) * n]
            .iter()
            .skip(ch)
            .step_by(n)
            .map(sanitize),
    );
}

fn to_y(frame: &Frame, v: f32) -> f32 {
    let t = (v - frame.y_min) / (frame.y_max - frame.y_min);
    (1.0 - t) * frame.height as f32
}

/// Polyline vertices with the same x/y mapping as the wgpu line shader;
/// `runs` holds the start of each pen-down run plus the final end.
fn line_points(
    frame: &Frame,
    values: &[f32],
    mode: LineGeometry,
    points: &mut Vec<[f32; 2]>,
    runs: &mut Vec<usize>,
) {
    points.clear();
    runs.clear();
    let x_scale = frame.width as f32 / (values.len() - 1) as f32;
    let mut pen_down = false;
    let point = |i: usize| [i as f32 * x_scale, to_y(frame, values[i])];
    if mode == LineGeometry::All {
        for (i, v) in values.iter().enumerate() {
            if !v.is_finite() {
                pen_down = false;
                continue;
            }
            if !pen_down {
                runs.push(points.len());
                pen_down = true;
            }
            points.push(point(i));
        }
    } else {
        // Samples of one pixel column within one run, as indices.
        let mut group: Option<(usize, usize, usize, usize)> = None; // first, min, max, last
        let mut column = usize::MAX;
        let flush = |group: &mut Option<(usize, usize, usize, usize)>,
                         points: &mut Vec<[f32; 2]>| {
            if let Some((first, min, max, last)) = group.take() {
                let mut picked = match mode {
                    LineGeometry::M4 => [first, min, max, last],
                    _ => [min, max, usize::MAX, usize::MAX],
                };
                picked.sort_unstable();
                let mut prev = usize::MAX;
                for i in picked {
                    if i != usize::MAX && i != prev {
                        points.push(point(i));
                        prev = i;
                    }
                }
            }
        };
        for (i, &v) in values.iter().enumerate() {
            if !v.is_finite() {
                flush(&mut group, points);
                pen_down = false;
                continue;
            }
            if !pen_down {
                runs.push(points.len());
                pen_down = true;
            }
            let c = (i as f32 * x_scale) as usize;
            if c != column {
                flush(&mut group, points);
                column = c;
            }
            group = Some(match group {
                None => (i, i, i, i),
                Some((first, min, max, _)) => (
                    first,
                    if v < values[min] { i } else { min },
                    if v > values[max] { i } else { max },
                    i,
                ),
            });
        }
        flush(&mut group, points);
    }
    runs.push(points.len());
}

/// Per-column min/max as screen y (`hi` is the upper edge, so the smaller
/// y); NaN where a column has no finite samples. Ranges match `reduce.wgsl`.
fn envelope(frame: &Frame, values: &[f32], hi: &mut Vec<f32>, lo: &mut Vec<f32>) {
    let width = frame.width as usize;
    let last_index = values.len() - 1;
    let half_span = (values.len() as f32 / width as f32 * 0.5).max(0.5);
    hi.clear();
    lo.clear();
    for column in 0..width {
        let center = (column as f32 + 0.5) / width as f32 * last_index as f32;
        let first = (center - half_span).floor().max(0.0) as usize;
        let last = ((center + half_span).ceil() as usize).min(last_index);
        // Plain comparisons skip NaN gaps and vectorize, unlike f32::min.
        let mut min = f32::INFINITY;
        let mut max = f32::NEG_INFINITY;
        for &v in &values[first..=last] {
            min = if v < min { v } else { min };
            max = if v > max { v } else { max };
        }
        if max < min {
            hi.push(f32::NAN);
            lo.push(f32::NAN);
        } else {
            hi.push(to_y(frame, max));
            lo.push(to_y(frame, min));
        }
    }
}

/// Closed envelope polygon of `columns`: along the maxima left to right,
/// back along the minima. Consecutive duplicate points are dropped.
fn envelope_polygon(
    hi: &[f32],
    lo: &[f32],
    columns: std::ops::Range<usize>,
    out: &mut Vec<[f32; 2]>,
) {
    out.clear();
    let mut push = |p: [f32; 2]| {
        if out.last() != Some(&p) {
            out.push(p);
        }
    };
    for c in columns.clone() {
        push([c as f32 + 0.5, hi[c]]);
    }
    for c in columns.rev() {
        push([c as f32 + 0.5, lo[c]]);
    }
    if out.len() > 1 && out.first() == out.last() {
        out.pop();
    }
}

/// Emits `points` (a closed polygon, clockwise on screen) grown by `radius`:
/// every edge offset outward, round joins at convex corners flattened like
/// Vello's GPU stroker (0.25 px tolerance). The result covers the same area
/// as filling the polygon plus stroking its outline `2 * radius` wide,
/// without the stroke's self-overlap where the envelope is thinner than the
/// line (area AA adds overlapping coverage up, so those edges came out
/// bolder).
///
/// At concave corners the two offset edges cross; with `merge_concave` they
/// are cut at the crossing when it lies within both edges, otherwise (and
/// always without it) they are left as a small loop, which non-zero
/// filling covers anyway.
fn outset_polygon(
    path: &mut PathEncoder<'_>,
    points: &[[f32; 2]],
    radius: f32,
    merge_concave: bool,
) {
    let n = points.len();
    if n < 2 {
        return;
    }
    const TOLERANCE: f32 = 0.25;
    let theta = (2.0 * (1.0 - TOLERANCE / radius.max(TOLERANCE)).acos()).max(1e-4);
    let cos_theta = theta.cos();
    // Unit direction and length of the edge from `a` to `b`.
    let edge = |a: [f32; 2], b: [f32; 2]| {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len = (dx * dx + dy * dy).sqrt();
        ([dx / len, dy / len], len)
    };
    let mut first = true;
    let mut emit = |x: f32, y: f32| {
        if first {
            path.move_to(x, y);
            first = false;
        } else {
            path.line_to(x, y);
        }
    };
    let (mut d0, mut len0) = edge(points[n - 1], points[0]);
    for i in 0..n {
        let p = points[i];
        let (d1, len1) = edge(p, points[if i + 1 == n { 0 } else { i + 1 }]);
        let n0 = [d0[1] * radius, -d0[0] * radius];
        let n1 = [d1[1] * radius, -d1[0] * radius];
        let cross = d0[0] * d1[1] - d0[1] * d1[0];
        let dot = d0[0] * d1[0] + d0[1] * d1[1];
        if cross > 0.0 || (cross == 0.0 && dot < 0.0) {
            // Convex: arc from n0 to n1, a single chord for gentle turns.
            emit(p[0] + n0[0], p[1] + n0[1]);
            if dot < cos_theta {
                let angle = cross.abs().atan2(dot);
                let steps = (angle / theta).ceil() as u32;
                let (s, c) = (angle / steps as f32).sin_cos();
                let mut r = n0;
                for _ in 1..steps {
                    r = [c * r[0] - s * r[1], s * r[0] + c * r[1]];
                    emit(p[0] + r[0], p[1] + r[1]);
                }
            }
            emit(p[0] + n1[0], p[1] + n1[1]);
        } else if cross == 0.0 {
            // Straight on.
            emit(p[0] + n1[0], p[1] + n1[1]);
        } else {
            // Concave: where the offset edges cross, if that is on both.
            let k = 1.0 / (1.0 + dot);
            let m = [(n0[0] + n1[0]) * k, (n0[1] + n1[1]) * k];
            let back = -(m[0] * d0[0] + m[1] * d0[1]);
            let ahead = m[0] * d1[0] + m[1] * d1[1];
            if merge_concave && back <= len0 && ahead <= len1 {
                emit(p[0] + m[0], p[1] + m[1]);
            } else {
                emit(p[0] + n0[0], p[1] + n0[1]);
                emit(p[0] + n1[0], p[1] + n1[1]);
            }
        }
        d0 = d1;
        len0 = len1;
    }
    path.close();
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
