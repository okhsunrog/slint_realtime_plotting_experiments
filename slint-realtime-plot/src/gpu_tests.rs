use crate::{PlotBackend, PlotBuffer, PlotConfig, PlotRenderer, required_wgpu_settings};
use slint::wgpu_30::wgpu;

fn pixels(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> Vec<u8> {
    let row = (texture.width() * 4).div_ceil(256) * 256;
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
    buffer.slice(..).get_mapped_range().unwrap().to_vec()
}

/// Run explicitly on a machine with a Vulkan/Metal/DX12 adapter.
#[test]
#[ignore = "requires a GPU adapter"]
fn peak_tail_line_mode_reset_and_cache() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let settings = required_wgpu_settings(32768, 1);
    let (device, queue) = pollster::block_on(
        adapter.request_device(&wgpu::DeviceDescriptor {
            required_features: settings.device_required_features,
            required_limits: settings
                .device_required_limits
                .using_resolution(adapter.limits()),
            ..Default::default()
        }),
    )
    .unwrap();
    let buffer = PlotBuffer::new(1, 32768);
    let mut data = vec![0.0; 32768];
    data[500] = 0.8; // First column's tail: beyond the old 256-iteration limit.
    buffer.push_batch(&data);
    let mut renderer = PlotRenderer::new(
        &device,
        &queue,
        PlotConfig {
            backend: PlotBackend::default(),
            num_channels: 1,
            capacity: 32768,
            y_min: -1.0,
            y_max: 1.0,
            auto_range: false,
            channel_colors: vec![[1.0, 0.0, 0.0, 1.0]],
        },
    );
    let texture = renderer.render(&buffer, 64, 64, 32768, 0, 1.0).texture;
    let peak = pixels(&device, &queue, &texture);
    assert!(
        peak[6 * 256 + 3] > 128,
        "peak beyond sample 256 must remain visible"
    );
    let texture = renderer
        .render(&buffer, 64, 64, 32768, u32::MAX, 1.0)
        .texture;
    assert_eq!(
        peak,
        pixels(&device, &queue, &texture),
        "pan must clamp to available history"
    );
    buffer.clear();
    let texture = renderer.render(&buffer, 64, 64, 32768, 0, 1.0).texture;
    assert!(
        pixels(&device, &queue, &texture)
            .chunks_exact(4)
            .all(|p| p[3] == 0)
    );
    buffer.push_batch(&[0.0, 0.0]);
    let texture = renderer.render(&buffer, 64, 64, 2, 0, 1.0).texture;
    let line = pixels(&device, &queue, &texture);
    assert!(line[32 * 256 + 32 * 4 + 3] > 128);
    let texture = renderer.render(&buffer, 64, 64, 32768, 0, 1.0).texture;
    let sparse = pixels(&device, &queue, &texture);
    assert!(sparse[32 * 256 + 63 * 4 + 3] > 128);
    assert_eq!(sparse[32 * 256 + 3], 0);

    // Projection must use the pixel metric on a wide, non-square plot.
    buffer.clear();
    buffer.push_batch(&[-0.5, 0.5]);
    let texture = renderer.render(&buffer, 320, 64, 2, 0, 1.0).texture;
    let diagonal = pixels(&device, &queue, &texture);
    assert!(diagonal[32 * 1280 + 160 * 4 + 3] > 128);
    assert!(diagonal[35 * 1280 + 160 * 4 + 3] < 16);
}

/// Renders the same signal with both backends into PNGs for visual
/// comparison: `PLOT_COMPARE_DIR=/some/dir cargo test --features vello
/// compare_backends_png -- --ignored`.
#[cfg(feature = "vello")]
#[test]
#[ignore = "requires a GPU adapter"]
fn compare_backends_png() {
    let dir = std::path::PathBuf::from(std::env::var("PLOT_COMPARE_DIR").unwrap());
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let settings = required_wgpu_settings(32768, 3);
    let (device, queue) = pollster::block_on(
        adapter.request_device(&wgpu::DeviceDescriptor {
            required_features: settings.device_required_features,
            required_limits: settings
                .device_required_limits
                .using_resolution(adapter.limits()),
            ..Default::default()
        }),
    )
    .unwrap();
    let buffer = PlotBuffer::new(3, 32768);
    let frames: Vec<f32> = (0..32768)
        .flat_map(|i| {
            let t = i as f32 / 20000.0;
            (0..3).map(move |ch| {
                let phase = ch as f32 * 2.094;
                10.0 * (t * 2.0 * std::f32::consts::PI * 50.0 + phase).sin()
                    + 0.3 * ((i * 7 + ch * 13) % 17) as f32
            })
        })
        .collect();
    buffer.push_batch(&frames);
    for (backend, name) in [(PlotBackend::Wgpu, "wgpu"), (PlotBackend::Vello, "vello")] {
        let mut renderer = PlotRenderer::new(
            &device,
            &queue,
            PlotConfig {
                backend,
                num_channels: 3,
                capacity: 32768,
                y_min: -15.0,
                y_max: 15.0,
                auto_range: true,
                channel_colors: vec![
                    [0.133, 0.827, 0.933, 1.0],
                    [0.545, 0.361, 0.965, 1.0],
                    [0.976, 0.451, 0.086, 1.0],
                ],
            },
        );
        // Visible-sample counts from a few samples per pixel down to a
        // fraction of one, at 1x and 2x hidpi (2x doubles the texture).
        for scale in [1.0f32, 2.0] {
            for visible in [50u32, 600, 2000, 8000, 32768] {
                renderer.render(
                    &buffer,
                    (1000.0 * scale) as u32,
                    (300.0 * scale) as u32,
                    visible,
                    0,
                    scale,
                );
                renderer
                    .export_png(
                        &dir.join(format!("{name}_v{visible}_x{scale}.png")),
                        [0.059, 0.059, 0.118],
                    )
                    .unwrap();
                // Frame cost: alternate the pan offset to defeat the render
                // cache, and wait for the GPU so the time includes drawing.
                let runs = 100u32;
                let t0 = std::time::Instant::now();
                for i in 0..runs {
                    // A new frame bumps the content generation; panning
                    // alone is clamped away when the window shows everything.
                    buffer.push_frame(&[0.0, 1.0, 2.0]);
                    renderer.render(
                        &buffer,
                        (1000.0 * scale) as u32,
                        (300.0 * scale) as u32,
                        visible,
                        i % 2,
                        scale,
                    );
                    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
                }
                println!(
                    "BENCH {name} visible={visible} scale={scale}: {:.3} ms/frame",
                    t0.elapsed().as_secs_f64() * 1000.0 / f64::from(runs)
                );
            }
        }
    }
}
