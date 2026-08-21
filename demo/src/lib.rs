slint::include_modules!();

mod data_gen;

use slint::wgpu_30::WGPUConfiguration;
use slint_realtime_plot::{PlotBuffer, PlotConfig, PlotRenderer, required_wgpu_settings};
use std::cell::Cell;
use std::io::Write as _;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};

/// Knobs shared between the UI thread and the sample-generator thread.
struct SimControl {
    amplitude_bits: AtomicU32,
    frequency_bits: AtomicU32,
    paused: AtomicBool,
    running: AtomicBool,
}

fn unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn fmt_sample(v: f32) -> String {
    if v.is_finite() {
        format!("{v:.2}")
    } else {
        "—".to_string()
    }
}

/// Write the currently visible window as CSV into the working directory.
fn export_csv(buffer: &PlotBuffer, time_window: f32, view_offset: u32) -> Result<String, String> {
    let vis = ((time_window * data_gen::SAMPLE_RATE) as u32).clamp(2, buffer.capacity as u32);
    let name = format!("plot_{}.csv", unix_secs());
    let file = std::fs::File::create(&name).map_err(|e| format!("create {name}: {e}"))?;
    let mut out = std::io::BufWriter::new(file);
    writeln!(out, "time_s,phase_a,phase_b,phase_c").map_err(|e| e.to_string())?;

    let mut frame = [0.0f32; data_gen::NUM_CHANNELS];
    for i in 0..vis {
        let frames_back = (view_offset + vis - i) as usize;
        buffer.read_back(frames_back, &mut frame);
        let t = -((frames_back - 1) as f32 / data_gen::SAMPLE_RATE);
        write!(out, "{t:.5}").map_err(|e| e.to_string())?;
        for v in frame {
            if v.is_finite() {
                write!(out, ",{v:.4}").map_err(|e| e.to_string())?;
            } else {
                write!(out, ",").map_err(|e| e.to_string())?;
            }
        }
        writeln!(out).map_err(|e| e.to_string())?;
    }
    out.flush().map_err(|e| e.to_string())?;
    Ok(name)
}

pub fn main() {
    let wgpu_settings = required_wgpu_settings(data_gen::NUM_SAMPLES, data_gen::NUM_CHANNELS);

    slint::BackendSelector::new()
        .require_wgpu_30(WGPUConfiguration::Automatic(wgpu_settings))
        .select()
        .expect("Unable to create Slint backend with WGPU renderer");

    let app = App::new().unwrap();

    let plot_buffer = Arc::new(PlotBuffer::new(
        data_gen::NUM_CHANNELS,
        data_gen::NUM_SAMPLES,
    ));
    let control = Arc::new(SimControl {
        amplitude_bits: AtomicU32::new(app.get_amplitude().to_bits()),
        frequency_bits: AtomicU32::new(app.get_frequency().to_bits()),
        paused: AtomicBool::new(app.get_paused()),
        running: AtomicBool::new(true),
    });

    // Sample generation runs on its own thread at a fixed tick — the
    // lock-free PlotBuffer is the only shared state. Sample density no
    // longer depends on the UI frame rate.
    let sim_thread = {
        let control = control.clone();
        let buffer = plot_buffer.clone();
        std::thread::spawn(move || {
            let mut sim = data_gen::MotorSimulator::new(data_gen::SAMPLE_RATE);
            let mut last = Instant::now();
            let mut carry = 0.0f32;
            while control.running.load(Ordering::Relaxed) {
                if control.paused.load(Ordering::Relaxed) {
                    last = Instant::now();
                } else {
                    let now = Instant::now();
                    let dt = now.duration_since(last).as_secs_f32().min(0.1);
                    last = now;

                    let exact = data_gen::SAMPLE_RATE * dt + carry;
                    let count = exact.floor();
                    carry = exact - count;

                    let amplitude = f32::from_bits(control.amplitude_bits.load(Ordering::Relaxed));
                    let frequency = f32::from_bits(control.frequency_bits.load(Ordering::Relaxed));
                    sim.generate_samples(&buffer, count as usize, amplitude, frequency);
                }
                std::thread::sleep(Duration::from_millis(4));
            }
        })
    };

    {
        let buffer = plot_buffer.clone();
        let weak = app.as_weak();
        app.on_export_csv(move || {
            if let Some(app) = weak.upgrade() {
                let msg = match export_csv(
                    &buffer,
                    app.get_time_window(),
                    app.get_view_offset().max(0) as u32,
                ) {
                    Ok(name) => slint::format!("Saved {name}"),
                    Err(e) => slint::format!("CSV export failed: {e}"),
                };
                app.set_export_status(msg);
            }
        });
    }

    // PNG export needs the renderer, which lives inside the rendering
    // notifier — hand the request over via a flag checked each frame.
    let png_requested = Rc::new(Cell::new(false));
    {
        let flag = png_requested.clone();
        app.on_export_png(move || flag.set(true));
    }

    let mut plot_renderer: Option<PlotRenderer> = None;
    let app_weak = app.as_weak();
    let cursor_was_active = Cell::new(false);
    let render_control = control.clone();
    let render_buffer = plot_buffer.clone();

    app.window()
        .set_rendering_notifier(move |state, graphics_api| match state {
            slint::RenderingState::RenderingSetup => {
                if let slint::GraphicsAPI::WGPU30 { device, queue, .. } = graphics_api {
                    plot_renderer = Some(PlotRenderer::new(
                        device,
                        queue,
                        PlotConfig {
                            num_channels: data_gen::NUM_CHANNELS,
                            capacity: data_gen::NUM_SAMPLES,
                            y_min: -15.0,
                            y_max: 15.0,
                            auto_range: true,
                            channel_colors: vec![
                                [0.133, 0.827, 0.933, 1.0], // cyan   – Phase A
                                [0.545, 0.361, 0.965, 1.0], // violet – Phase B
                                [0.976, 0.451, 0.086, 1.0], // orange – Phase C
                            ],
                        },
                    ));
                }
            }
            slint::RenderingState::BeforeRendering => {
                if let (Some(renderer), Some(app)) = (plot_renderer.as_mut(), app_weak.upgrade()) {
                    let amplitude = app.get_amplitude();
                    let frequency = app.get_frequency();
                    let paused = app.get_paused();
                    render_control
                        .amplitude_bits
                        .store(amplitude.to_bits(), Ordering::Relaxed);
                    render_control
                        .frequency_bits
                        .store(frequency.to_bits(), Ordering::Relaxed);
                    render_control.paused.store(paused, Ordering::Relaxed);

                    if !paused {
                        app.set_status_text(slint::format!(
                            "3-Phase | {:.0} Hz | {:.1} A | 20 kSa/s",
                            frequency,
                            amplitude,
                        ));
                    }

                    let visible_samples = ((app.get_time_window() * data_gen::SAMPLE_RATE) as u32)
                        .clamp(2, data_gen::NUM_SAMPLES as u32);
                    let view_offset = app.get_view_offset().max(0) as u32;

                    // Cursor readout: sample the ring at the hovered position
                    if app.get_cursor_active() {
                        let frac = app.get_cursor_frac().clamp(0.0, 1.0);
                        let i = (frac * (visible_samples - 1) as f32).round() as u32;
                        let frames_back = (view_offset + visible_samples - i) as usize;
                        let mut frame = [0.0f32; data_gen::NUM_CHANNELS];
                        render_buffer.read_back(frames_back, &mut frame);
                        let t = (frames_back - 1) as f32 / data_gen::SAMPLE_RATE;
                        app.set_cursor_text(slint::format!(
                            "t: -{:.0} ms   A {}  B {}  C {}",
                            t * 1000.0,
                            fmt_sample(frame[0]),
                            fmt_sample(frame[1]),
                            fmt_sample(frame[2]),
                        ));
                        cursor_was_active.set(true);
                    } else if cursor_was_active.take() {
                        app.set_cursor_text(Default::default());
                    }

                    let output = renderer.render(
                        &render_buffer,
                        app.get_requested_texture_width() as u32,
                        app.get_requested_texture_height() as u32,
                        visible_samples,
                        view_offset,
                        app.window().scale_factor(),
                    );
                    app.set_texture(slint::Image::try_from(output.texture).unwrap());
                    app.set_y_min(output.y_min);
                    app.set_y_max(output.y_max);
                    app.set_y_divisions(output.y_divisions as i32);

                    if png_requested.take() {
                        let background = if app.get_dark_mode() {
                            [0.059, 0.059, 0.118] // #0f0f1e
                        } else {
                            [0.980, 0.980, 0.980] // #fafafa
                        };
                        let name = format!("plot_{}.png", unix_secs());
                        let msg = match renderer.export_png(name.as_ref(), background) {
                            Ok(()) => slint::format!("Saved {name}"),
                            Err(e) => slint::format!("PNG export failed: {e}"),
                        };
                        app.set_export_status(msg);
                    }

                    app.window().request_redraw();
                }
            }
            slint::RenderingState::RenderingTeardown => {
                drop(plot_renderer.take());
            }
            _ => {}
        })
        .expect("Unable to set rendering notifier");

    app.run().unwrap();

    control.running.store(false, Ordering::Relaxed);
    sim_thread.join().ok();
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: slint::android::AndroidApp) {
    slint::android::init(app).unwrap();
    main();
}
