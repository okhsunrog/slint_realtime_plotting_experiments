//! GPU-accelerated real-time plotting for Slint via WGPU.
//!
//! Rust side: [`PlotBuffer`] — a lock-free SPSC ring buffer to feed samples
//! into (from any thread), and [`PlotRenderer`] — renders one chart into a
//! texture inside Slint's rendering notifier. Configure the Slint backend
//! with [`required_wgpu_settings`].
//!
//! UI side: `ui/plot.slint` exports the `PlotWidget` component (grid, axes,
//! legend, pan/zoom, cursor). Map it in your `build.rs`:
//!
//! ```ignore
//! # use std::collections::HashMap;
//! let mut library_paths = HashMap::new();
//! library_paths.insert(
//!     "slint-realtime-plot".to_string(),
//!     std::path::PathBuf::from("path/to/slint-realtime-plot/ui"),
//! );
//! slint_build::compile_with_config(
//!     "ui/app.slint",
//!     slint_build::CompilerConfiguration::new().with_library_paths(library_paths),
//! ).unwrap();
//! ```
//!
//! then import it with `import { PlotWidget } from "@slint-realtime-plot/plot.slint";`.

mod buffer;
mod renderer;

pub use buffer::PlotBuffer;
pub use renderer::{PlotConfig, PlotRenderer, RenderOutput};

use slint::wgpu_30::{WGPUSettings, wgpu};

/// Maximum number of channels supported by the shader.
pub const MAX_CHANNELS: usize = 8;

/// Build the [`WGPUSettings`] required by the plot renderer.
///
/// - `max_capacity`  — largest ring-buffer capacity used across all charts.
/// - `max_channels`  — largest number of channels in any single chart.
pub fn required_wgpu_settings(max_capacity: usize, max_channels: usize) -> WGPUSettings {
    let mut s = WGPUSettings::default();
    s.device_required_features = wgpu::Features::IMMEDIATES;
    s.device_required_limits.max_immediate_size = size_of::<renderer::PlotParams>() as u32;
    s.device_required_limits
        .max_storage_buffers_per_shader_stage = 1;
    s.device_required_limits.max_storage_buffer_binding_size =
        (max_capacity * max_channels * size_of::<f32>()) as u64;
    s
}
