slint::include_modules!();

mod data_gen;
mod renderer;

use slint::wgpu_28::{WGPUConfiguration, WGPUSettings, wgpu};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

pub fn main() {
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
        (data_gen::NUM_SAMPLES * data_gen::NUM_CHANNELS * std::mem::size_of::<f32>()) as u32;

    slint::BackendSelector::new()
        .require_wgpu_28(WGPUConfiguration::Automatic(wgpu_settings))
        .select()
        .expect("Unable to create Slint backend with WGPU renderer");

    let app = App::new().unwrap();

    let simulator = Rc::new(RefCell::new(data_gen::MotorSimulator::new(
        data_gen::SAMPLE_RATE,
    )));
    let mut plot_renderer: Option<renderer::PlotRenderer> = None;
    let app_weak = app.as_weak();
    let last_frame = Cell::new(Instant::now());

    app.window()
        .set_rendering_notifier(move |state, graphics_api| match state {
            slint::RenderingState::RenderingSetup => {
                if let slint::GraphicsAPI::WGPU28 { device, queue, .. } = graphics_api {
                    plot_renderer = Some(renderer::PlotRenderer::new(device, queue));
                }
            }
            slint::RenderingState::BeforeRendering => {
                if let (Some(renderer), Some(app)) = (plot_renderer.as_mut(), app_weak.upgrade()) {
                    let now = Instant::now();
                    let dt = now.duration_since(last_frame.get()).as_secs_f32();
                    last_frame.set(now);

                    if !app.get_paused() {
                        let amplitude = app.get_amplitude();
                        let frequency = app.get_frequency();

                        let samples_per_frame = (data_gen::SAMPLE_RATE * dt).round() as usize;
                        let mut sim = simulator.borrow_mut();
                        sim.generate_samples(samples_per_frame, amplitude, frequency);

                        app.set_status_text(slint::format!(
                            "3-Phase | {:.0} Hz | {:.1} A | 20 kSa/s",
                            frequency,
                            amplitude,
                        ));
                    }

                    let sim = simulator.borrow();
                    let time_window = app.get_time_window();
                    let visible_samples = (time_window * data_gen::SAMPLE_RATE) as u32;
                    let texture = renderer.render(
                        &sim,
                        app.get_requested_texture_width() as u32,
                        app.get_requested_texture_height() as u32,
                        app.get_dark_mode(),
                        visible_samples,
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

    app.run().unwrap();
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: slint::android::AndroidApp) {
    slint::android::init(app).unwrap();
    main();
}
