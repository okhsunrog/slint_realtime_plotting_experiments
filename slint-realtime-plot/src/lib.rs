//! GPU-accelerated real-time plotting for Slint via WGPU.
//!
//! Rust side: [`PlotBuffer`] — a consistent ring buffer to feed samples
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
#[cfg(feature = "vello")]
mod vello_backend;

#[cfg(test)]
mod gpu_tests;
#[cfg(all(test, feature = "vello"))]
mod vello_bench;

pub use buffer::PlotBuffer;
pub use renderer::{PlotBackend, PlotConfig, PlotRenderer, RenderOutput};

use slint::wgpu_30::{WGPUSettings, wgpu};

/// Maximum number of channels supported by the shader.
pub const MAX_CHANNELS: usize = 8;

/// Build the [`WGPUSettings`] required by the plot renderer.
///
/// - `max_capacity`  — largest ring-buffer capacity used across all charts.
/// - `max_channels`  — largest number of channels in any single chart.
pub fn required_wgpu_settings(max_capacity: usize, max_channels: usize) -> WGPUSettings {
    let mut s = WGPUSettings::default();
    // Vello's compute pipeline needs storage textures, many storage buffers
    // and 256-wide workgroups, none of which the small default limits allow.
    #[cfg(feature = "vello")]
    {
        s.device_required_limits = wgpu::Limits::default();
    }
    s.device_required_features = wgpu::Features::IMMEDIATES;
    // Diagnostics builds time every pipeline stage.
    #[cfg(feature = "vello-profile")]
    {
        s.device_required_features |= wgpu::Features::TIMESTAMP_QUERY
            | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS
            | wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES;
    }
    #[cfg(not(feature = "vello"))]
    {
        s.device_required_limits
            .max_compute_invocations_per_workgroup = 64;
        s.device_required_limits.max_compute_workgroup_size_x = 64;
        s.device_required_limits.max_compute_workgroup_size_y = 1;
        s.device_required_limits.max_compute_workgroup_size_z = 1;
        s.device_required_limits
            .max_compute_workgroups_per_dimension = 65535;
        s.device_required_limits
            .max_storage_buffers_per_shader_stage = 2;
    }
    s.device_required_limits.max_immediate_size = size_of::<renderer::PlotParams>() as u32;
    let plot_binding_size = ((max_capacity * max_channels * size_of::<f32>()) as u64).max(
        // Slint upgrades texture dimensions to the adapter's resolution
        // limits. Reserve envelopes for up to 32768 columns, not just
        // the small WebGL-compatible default requested above.
        32768 * max_channels as u64 * 16,
    );
    // Vello binds scene-sized buffers (tens of MiB), so never go below the
    // default there.
    #[cfg(feature = "vello")]
    let plot_binding_size =
        plot_binding_size.max(u64::from(wgpu::Limits::default().max_storage_buffer_binding_size));
    s.device_required_limits.max_storage_buffer_binding_size = plot_binding_size;
    s
}
