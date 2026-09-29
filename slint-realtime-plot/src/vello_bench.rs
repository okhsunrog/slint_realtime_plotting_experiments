//! Benchmark and visual-parity harness for the Vello compute scene variants:
//! `cargo test --release --features vello vello_bench -- --ignored --nocapture`
//! (add `vello-profile` for per-stage GPU times).
//!
//! All variants of a case are rendered interleaved, frame by frame, from
//! identical scrolling data, so clock and thermal drift hit them equally.
//!
//! Environment: `PLOT_BENCH_FRAMES` (default 300), `PLOT_BENCH_VARIANTS`
//! (comma list), `PLOT_BENCH_CASES` (comma list), `PLOT_BENCH_DIR` (PNG
//! output directory).

use crate::vello_backend::{ComputeOptions, LineGeometry};
use crate::{PlotBackend, PlotBuffer, PlotConfig, PlotRenderer, required_wgpu_settings};
use slint::wgpu_30::wgpu;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const CAPACITY: usize = 32768;
const CHANNELS: usize = 3;
const COLORS: [[f32; 4]; 3] = [
    [0.133, 0.827, 0.933, 1.0],
    [0.545, 0.361, 0.965, 1.0],
    [0.976, 0.451, 0.086, 1.0],
];
/// New samples per frame: 20 kSa/s at 60 fps.
const PER_FRAME: usize = 333;

struct Case {
    name: &'static str,
    width: u32,
    height: u32,
    scale: f32,
    visible: u32,
    motor: bool,
}

const CASES: &[Case] = &[
    // compare_backends_png's signal (sine + period-17 sawtooth).
    Case { name: "saw-line8k-1x", width: 1000, height: 300, scale: 1.0, visible: 8000, motor: false },
    Case { name: "saw-line2k-1x", width: 1000, height: 300, scale: 1.0, visible: 2000, motor: false },
    Case { name: "saw-peak-1x", width: 1000, height: 300, scale: 1.0, visible: 32768, motor: false },
    Case { name: "saw-peak-2x", width: 2000, height: 600, scale: 2.0, visible: 32768, motor: false },
    Case { name: "saw-line8k-2x", width: 2000, height: 600, scale: 2.0, visible: 8000, motor: false },
    // The demo's signal (5 A, 5 Hz, 5% gaussian noise) on a desktop window...
    Case { name: "motor-peak-1x", width: 1000, height: 300, scale: 1.0, visible: 20000, motor: true },
    Case { name: "motor-line-1x", width: 1000, height: 300, scale: 1.0, visible: 2000, motor: true },
    Case { name: "motor-line8k-1x", width: 1000, height: 300, scale: 1.0, visible: 8000, motor: true },
    Case { name: "motor-line4k-2x", width: 2000, height: 600, scale: 2.0, visible: 8000, motor: true },
    // ...and on the phone's plot area (1120x1770 px at scale 3.5).
    Case { name: "motor-peak-phone", width: 1120, height: 1770, scale: 3.5, visible: 20000, motor: true },
    Case { name: "motor-line-phone", width: 1120, height: 1770, scale: 3.5, visible: 2000, motor: true },
    // Almost nothing to draw: the pipelines' fixed cost.
    Case { name: "tiny-1x", width: 1000, height: 300, scale: 1.0, visible: 50, motor: true },
    Case { name: "tiny-phone", width: 1120, height: 1770, scale: 3.5, visible: 50, motor: true },
];

/// Variant name and scene options; `None` is the hand-written wgpu path.
fn variants() -> Vec<(&'static str, Option<ComputeOptions>)> {
    // The naive geometry, written straight into the encoding.
    let direct = ComputeOptions {
        kurbo: false,
        ..ComputeOptions::NAIVE
    };
    vec![
        ("wgpu", None),
        ("naive", Some(ComputeOptions::NAIVE)),
        ("direct", Some(direct)),
        ("m4", Some(ComputeOptions { line: LineGeometry::M4, ..direct })),
        ("minmax", Some(ComputeOptions { line: LineGeometry::MinMax, ..direct })),
        ("outset", Some(ComputeOptions { outset: true, ..direct })),
        (
            "outset_m",
            Some(ComputeOptions {
                outset: true,
                merge_concave: true,
                ..direct
            }),
        ),
        ("opt", Some(ComputeOptions::OPTIMIZED)),
    ]
}

/// Interleaved frames: the test signal of `compare_backends_png`, or the
/// demo's motor simulator.
fn signal(motor: bool, frames: usize) -> Vec<f32> {
    if !motor {
        return (0..frames)
            .flat_map(|i| {
                let t = i as f32 / 20000.0;
                (0..3).map(move |ch| {
                    let phase = ch as f32 * 2.094;
                    10.0 * (t * 2.0 * std::f32::consts::PI * 50.0 + phase).sin()
                        + 0.3 * ((i * 7 + ch * 13) % 17) as f32
                })
            })
            .collect();
    }
    let mut rng = 0xDEAD_BEEF_CAFE_BABEu64;
    let mut normal = move || {
        let mut sum = 0.0f32;
        for _ in 0..6 {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            sum += (rng & 0xFFFF_FFFF) as u32 as f32 / u32::MAX as f32;
        }
        sum - 3.0
    };
    let (amplitude, frequency) = (5.0f32, 5.0f32);
    let step = std::f32::consts::TAU * frequency / 20000.0;
    let mut phase = 0.0f32;
    let mut out = Vec::with_capacity(frames * 3);
    for _ in 0..frames {
        phase = (phase + step) % std::f32::consts::TAU;
        for k in 0..3 {
            let offset = k as f32 * std::f32::consts::TAU / 3.0;
            out.push(amplitude * (phase + offset).sin() + normal() * 0.05 * amplitude);
        }
    }
    out
}

fn pixels(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> Vec<u8> {
    let width = texture.width();
    let row = (width * 4).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(row * texture.height()),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: None,
            },
        },
        texture.size(),
    );
    queue.submit(Some(encoder.finish()));
    let (tx, rx) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().unwrap();
    let data = buffer.slice(..).get_mapped_range().unwrap();
    data.chunks(row as usize)
        .flat_map(|r| r[..(width * 4) as usize].iter().copied())
        .collect()
}

fn median(values: &mut [f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f64::total_cmp);
    values[values.len() / 2]
}

fn percentile(values: &mut [f64], p: f64) -> f64 {
    values.sort_by(f64::total_cmp);
    values[((values.len() - 1) as f64 * p).round() as usize]
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

/// Pixel difference versus the reference: number of pixels whose largest
/// channel difference exceeds 32 and 96, and the largest difference.
fn diff(a: &[u8], b: &[u8]) -> (usize, usize, u8) {
    let mut over32 = 0;
    let mut over96 = 0;
    let mut max = 0u8;
    for (pa, pb) in a.chunks_exact(4).zip(b.chunks_exact(4)) {
        let d = pa.iter().zip(pb).map(|(x, y)| x.abs_diff(*y)).max().unwrap();
        max = max.max(d);
        over32 += usize::from(d > 32);
        over96 += usize::from(d > 96);
    }
    (over32, over96, max)
}

struct Run {
    name: &'static str,
    buffer: PlotBuffer,
    renderer: PlotRenderer,
    build: Vec<f64>,
    submit: Vec<f64>,
    wait: Vec<f64>,
    total: Vec<f64>,
    segments: u32,
    parity: String,
}

#[test]
#[ignore = "requires a GPU adapter"]
fn vello_bench() {
    let frames: usize = std::env::var("PLOT_BENCH_FRAMES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(300);
    let only_variants: Option<Vec<String>> = std::env::var("PLOT_BENCH_VARIANTS")
        .ok()
        .map(|v| v.split(',').map(str::to_owned).collect());
    let only_cases: Option<Vec<String>> = std::env::var("PLOT_BENCH_CASES")
        .ok()
        .map(|v| v.split(',').map(str::to_owned).collect());
    let png_dir = std::env::var("PLOT_BENCH_DIR").ok().map(PathBuf::from);

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let settings = required_wgpu_settings(CAPACITY, CHANNELS);
    #[cfg(feature = "vello-profile")]
    let features = settings.device_required_features
        | (adapter.features() & wgpu_profiler::GpuProfiler::ALL_WGPU_TIMER_FEATURES);
    #[cfg(not(feature = "vello-profile"))]
    let features = settings.device_required_features;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: features,
        required_limits: settings
            .device_required_limits
            .using_resolution(adapter.limits()),
        ..Default::default()
    }))
    .unwrap();
    println!("adapter: {:?}, {frames} frames per variant", adapter.get_info().name);

    for case in CASES {
        if only_cases
            .as_ref()
            .is_some_and(|c| !c.iter().any(|n| n == case.name))
        {
            continue;
        }
        let data = signal(case.motor, CAPACITY + (frames + 1) * PER_FRAME);
        let mut reference: Option<Vec<u8>> = None;
        let mut runs = Vec::new();
        for (name, options) in variants() {
            if only_variants
                .as_ref()
                .is_some_and(|v| !v.iter().any(|n| n == name))
                && name != "naive"
            {
                continue;
            }
            let buffer = PlotBuffer::new(CHANNELS, CAPACITY);
            buffer.push_batch(&data[..CAPACITY * CHANNELS]);
            let mut renderer = PlotRenderer::new(
                &device,
                &queue,
                PlotConfig {
                    backend: if options.is_some() {
                        PlotBackend::Vello
                    } else {
                        PlotBackend::Wgpu
                    },
                    num_channels: CHANNELS,
                    capacity: CAPACITY,
                    y_min: -15.0,
                    y_max: 15.0,
                    auto_range: true,
                    channel_colors: COLORS.to_vec(),
                },
            );
            if let Some(options) = options {
                renderer
                    .vello_mut()
                    .and_then(|v| v.compute_mut())
                    .unwrap()
                    .options = options;
            }
            // Settle the auto-range on the initial data, then capture.
            let mut texture = None;
            for _ in 0..60 {
                texture = Some(
                    renderer
                        .render(&buffer, case.width, case.height, case.visible, 0, case.scale)
                        .texture,
                );
            }
            let image = pixels(&device, &queue, &texture.unwrap());
            let parity = match (&reference, name) {
                (_, "naive") => {
                    reference = Some(image);
                    String::new()
                }
                (Some(r), _) => {
                    let (over32, over96, max) = diff(r, &image);
                    format!(" | vs naive: {over32} px >32, {over96} px >96, max {max}")
                }
                _ => String::new(),
            };
            if let Some(dir) = &png_dir {
                renderer
                    .export_png(
                        &dir.join(format!("{}_{name}.png", case.name)),
                        [0.059, 0.059, 0.118],
                    )
                    .unwrap();
            }
            let compute = renderer.vello_mut().and_then(|v| v.compute_mut());
            let segments = compute.as_ref().map(|c| c.segments()).unwrap_or(0);
            #[cfg(feature = "vello-profile")]
            if let Some(c) = compute {
                c.gpu_stages.clear();
            }
            #[cfg(not(feature = "vello-profile"))]
            let _ = compute;
            runs.push(Run {
                name,
                buffer,
                renderer,
                build: Vec::new(),
                submit: Vec::new(),
                wait: Vec::new(),
                total: Vec::new(),
                segments,
                parity,
            });
        }

        for f in 0..frames {
            let start = (CAPACITY + f * PER_FRAME) * CHANNELS;
            let chunk = &data[start..start + PER_FRAME * CHANNELS];
            // Rotate the order so no variant always follows the same one.
            let n = runs.len();
            for k in 0..n {
                let run = &mut runs[(f + k) % n];
                run.buffer.push_batch(chunk);
                let t0 = Instant::now();
                run.renderer
                    .render(&run.buffer, case.width, case.height, case.visible, 0, case.scale);
                let t1 = Instant::now();
                device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
                let t2 = Instant::now();
                run.total.push(ms(t2 - t0));
                run.wait.push(ms(t2 - t1));
                if let Some(c) = run.renderer.vello_mut().and_then(|v| v.compute_mut()) {
                    run.build.push(ms(c.last_build));
                    run.submit.push(ms(c.last_submit));
                }
            }
        }

        for run in &mut runs {
            let p90 = percentile(&mut run.total, 0.9);
            println!(
                "BENCH {:<17} {:<10} total {:.3} ms (p90 {:.3}; build {:.3}, submit {:.3}, gpu wait {:.3}) segs {}{}",
                case.name,
                run.name,
                median(&mut run.total),
                p90,
                median(&mut run.build),
                median(&mut run.submit),
                median(&mut run.wait),
                run.segments,
                run.parity,
            );
            #[cfg(feature = "vello-profile")]
            if run.name == "wgpu" {
                println!(
                    "STAGES {:<17} {:<10} sum={:.3}",
                    case.name,
                    run.name,
                    median(&mut run.renderer.gpu_times)
                );
            }
            #[cfg(feature = "vello-profile")]
            if let Some(c) = run.renderer.vello_mut().and_then(|v| v.compute_mut()) {
                let stages: Vec<String> = c
                    .gpu_stages
                    .iter_mut()
                    .map(|(label, times)| {
                        let label = label.trim_start_matches("vello.");
                        format!("{label}={:.3}", median(times))
                    })
                    .collect();
                println!("STAGES {:<17} {:<10} {}", case.name, run.name, stages.join(" "));
            }
        }
    }
}

/// Composites RGBA pixels over the dark theme background, like
/// `PlotRenderer::export_png`, so differences are measured as seen.
fn composite(rgba: &[u8]) -> Vec<u8> {
    let bg = [15.0f32, 15.0, 30.0];
    rgba.chunks_exact(4)
        .flat_map(|p| {
            let a = f32::from(p[3]) / 255.0;
            (0..3).map(move |c| (f32::from(p[c]) * a + bg[c] * (1.0 - a)).round() as u8)
        })
        .collect()
}

/// Edge cases for the optimized scene building (gaps, isolated samples,
/// flat and vertical segments, spikes): the optimized Vello scene must look
/// like the naive one. Prints the number of visibly different pixels.
#[test]
#[ignore = "requires a GPU adapter"]
fn vello_optimized_matches_naive() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let settings = required_wgpu_settings(CAPACITY, CHANNELS);
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: settings.device_required_features,
        required_limits: settings
            .device_required_limits
            .using_resolution(adapter.limits()),
        ..Default::default()
    }))
    .unwrap();
    type Gen = fn(usize, usize) -> f32;
    let signals: [(&str, Gen); 6] = [
        ("flat", |_, ch| ch as f32),
        ("gaps", |i, ch| {
            // Gaps of 300 samples every 1000, and isolated samples inside.
            if i % 1000 >= 700 && i % 50 != 0 {
                f32::NAN
            } else {
                (i as f32 * 0.002 + ch as f32).sin() * 5.0
            }
        }),
        ("square", |i, ch| if (i / 400 + ch) % 2 == 0 { 4.0 } else { -4.0 }),
        ("spikes", |i, ch| if (i + ch * 37) % 977 == 0 { 9.0 } else { 0.1 * ch as f32 }),
        ("sine", |i, ch| (i as f32 * 0.0031 + ch as f32 * 2.1).sin() * 6.0),
        ("infinite", |i, ch| if i % 3000 < 5 { f32::INFINITY } else { (i as f32 * 0.001).cos() + ch as f32 }),
    ];
    let mut worst = 0.0f64;
    for (name, generate) in signals {
        let data: Vec<f32> = (0..CAPACITY)
            .flat_map(|i| (0..CHANNELS).map(move |ch| generate(i, ch)))
            .collect();
        for (width, height, scale, visible) in [
            (1000, 300, 1.0, 32768),
            (1000, 300, 1.0, 8000),
            (1000, 300, 1.0, 2000),
            (1000, 300, 1.0, 50),
            (2000, 600, 2.0, 32768),
            (1120, 1770, 3.5, 20000),
            (1120, 1770, 3.5, 2000),
        ] {
            let mut images = Vec::new();
            for backend in [PlotBackend::VelloNaive, PlotBackend::Vello] {
                let buffer = PlotBuffer::new(CHANNELS, CAPACITY);
                buffer.push_batch(&data);
                let mut renderer = PlotRenderer::new(
                    &device,
                    &queue,
                    PlotConfig {
                        backend,
                        num_channels: CHANNELS,
                        capacity: CAPACITY,
                        y_min: -10.0,
                        y_max: 10.0,
                        auto_range: false,
                        channel_colors: COLORS.to_vec(),
                    },
                );
                let texture = renderer
                    .render(&buffer, width, height, visible, 0, scale)
                    .texture;
                images.push(composite(&pixels(&device, &queue, &texture)));
                if let Ok(dir) = std::env::var("PLOT_BENCH_DIR") {
                    let variant = if backend == PlotBackend::Vello { "opt" } else { "naive" };
                    renderer
                        .export_png(
                            &Path::new(&dir)
                                .join(format!("{name}-{visible}-x{scale}_{variant}.png")),
                            [0.059, 0.059, 0.118],
                        )
                        .unwrap();
                }
            }
            let (mut over32, mut over96) = (0usize, 0usize);
            for (a, b) in images[0].chunks_exact(3).zip(images[1].chunks_exact(3)) {
                let d = a.iter().zip(b).map(|(x, y)| x.abs_diff(*y)).max().unwrap();
                over32 += usize::from(d > 32);
                over96 += usize::from(d > 96);
            }
            let total = (width * height) as f64;
            println!(
                "PARITY {name:<8} {width}x{height} x{scale} visible {visible:>5}: {over32} px >32 ({:.3}%), {over96} px >96",
                100.0 * over32 as f64 / total
            );
            worst = worst.max(over96 as f64 / total);
        }
    }
    // Up to ~1% (flat signal, peak mode, 2x): the naive stroke of a
    // zero-height envelope lies on itself and area AA adds up the overlap,
    // so its edge rows come out fully opaque; the grown polygon covers
    // them once, at the true line width.
    assert!(worst < 0.012, "optimized scene differs visibly from the naive one");
}
