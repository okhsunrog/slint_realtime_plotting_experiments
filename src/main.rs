slint::include_modules!();

mod data_gen;
mod renderer;

use slint::wgpu_28::{WGPUConfiguration, WGPUSettings, wgpu};
use slint::{Timer, TimerMode};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

fn main() {
    let mut wgpu_settings = WGPUSettings::default();
    wgpu_settings.device_required_features = wgpu::Features::IMMEDIATES;
    wgpu_settings.device_required_limits.max_immediate_size =
        std::mem::size_of::<renderer::PlotParams>() as u32;
    wgpu_settings
        .device_required_limits
        .max_storage_buffers_per_shader_stage = 1;
    wgpu_settings
        .device_required_limits
        .max_storage_buffer_binding_size =
        (data_gen::NUM_SAMPLES * std::mem::size_of::<f32>()) as u32;

    slint::BackendSelector::new()
        .require_wgpu_28(WGPUConfiguration::Automatic(wgpu_settings))
        .select()
        .expect("Unable to create Slint backend with WGPU renderer");

    let app = App::new().unwrap();

    let simulator = Rc::new(RefCell::new(data_gen::AdcSimulator::new(10000.0)));

    let mut plot_renderer: Option<renderer::PlotRenderer> = None;

    let app_weak = app.as_weak();
    let sim_for_render = simulator.clone();

    app.window()
        .set_rendering_notifier(move |state, graphics_api| match state {
            slint::RenderingState::RenderingSetup => {
                if let slint::GraphicsAPI::WGPU28 { device, queue, .. } = graphics_api {
                    plot_renderer = Some(renderer::PlotRenderer::new(device, queue));
                }
            }
            slint::RenderingState::BeforeRendering => {
                if let (Some(renderer), Some(app)) = (plot_renderer.as_mut(), app_weak.upgrade()) {
                    let sim = sim_for_render.borrow();
                    let texture = renderer.render(
                        &sim,
                        app.get_requested_texture_width() as u32,
                        app.get_requested_texture_height() as u32,
                    );
                    app.set_texture(slint::Image::try_from(texture).unwrap());
                    app.window().request_redraw();
                }
            }
            slint::RenderingState::RenderingTeardown => {
                drop(plot_renderer.take());
            }
            _ => {}
        })
        .expect("Unable to set rendering notifier");

    let timer = Timer::default();
    let app_weak_timer = app.as_weak();
    let sim_for_timer = simulator.clone();

    timer.start(TimerMode::Repeated, Duration::from_millis(16), move || {
        if let Some(app) = app_weak_timer.upgrade() {
            if !app.get_paused() {
                let amplitude = app.get_amplitude();
                let frequency = app.get_frequency();

                let mut sim = sim_for_timer.borrow_mut();
                sim.generate_samples(160, amplitude, frequency);

                let rms = calculate_rms(&sim.buffer);
                app.set_status_text(slint::format!(
                    "Sample rate: 10 kHz | RMS: {:.2} A | Write pos: {}",
                    rms,
                    sim.write_pos
                ));
            }
            app.window().request_redraw();
        }
    });

    app.run().unwrap();
}

fn calculate_rms(buffer: &[f32]) -> f32 {
    let sum_sq: f32 = buffer.iter().map(|x| x * x).sum();
    (sum_sq / buffer.len() as f32).sqrt()
}
