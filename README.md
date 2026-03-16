# Slint Real-Time Plotting Experiments

Real-time 3-phase motor current waveform visualization using **Slint** and **WGPU**, targeting desktop and Android.

![Rust](https://img.shields.io/badge/Rust-2024_edition-orange)

## Overview

This project simulates a 3-phase AC motor current sensor and renders the waveforms in real time using GPU shaders. It demonstrates high-performance data visualization with:

- **20 kHz sample rate** with a 32,768-sample circular buffer
- **GPU-accelerated rendering** via WGPU with custom WGSL shaders
- **Anti-aliased min/max line rendering** for clean waveforms at any zoom level
- **3-phase signals** with 120° phase offsets, Gaussian noise, and random transient spikes

## Features

- **Interactive controls** — adjust amplitude (0.1–10 A), frequency (1–20 Hz), and time window (0.1–1.6 s) in real time
- **Play/Pause** — freeze the simulation to inspect waveforms
- **Theme switching** — System, Dark, and Light modes with glow effects in dark mode
- **Cross-platform** — runs on Linux, Windows, macOS, and Android
- **Responsive UI** — adapts to window resizing and respects Android safe area insets

## Building

### Prerequisites

- [Rust](https://rustup.rs/) (edition 2024)
- A GPU with Vulkan, Metal, or DX12 support

### Desktop

```bash
cargo run
```

### Android

Requires the Android NDK and `cargo-apk` or equivalent tooling:

```bash
cargo build --no-default-features --features android
```

## Project Structure

```
├── Cargo.toml          # Dependencies and feature flags
├── build.rs            # Compiles .slint UI files
├── src/
│   ├── main.rs         # Entry point
│   ├── lib.rs          # App initialization, WGPU setup, timer loop
│   ├── data_gen.rs     # 3-phase motor simulator (circular buffer, noise, transients)
│   ├── renderer.rs     # WGPU render pipeline and texture management
│   └── shader.wgsl     # Vertex/fragment shaders for waveform rendering
└── ui/
    └── scene.slint     # UI layout, controls, grid, and axis labels
```

## How It Works

1. **Data generation** — `MotorSimulator` produces interleaved 3-phase samples at 20 kHz with configurable amplitude/frequency, Gaussian noise (5%), and rare transient spikes.

2. **GPU rendering** — The sample buffer is uploaded to a WGPU storage buffer each frame. A fullscreen-triangle fragment shader maps pixel columns to sample ranges, computes per-column min/max values for antialiasing, and draws color-coded waveforms:
   - **Phase 1**: Magenta
   - **Phase 2**: Red
   - **Phase 3**: Green

3. **UI** — Slint provides the control panel (sliders, theme selector, play/pause) and overlays grid lines and axis labels on top of the GPU-rendered plot texture.

## Dependencies

| Crate | Purpose |
|-------|---------|
| [slint](https://slint.dev/) | UI framework with WGPU integration |
| [wgpu](https://wgpu.rs/) | Cross-platform GPU API |
| [bytemuck](https://docs.rs/bytemuck) | Safe memory casting for GPU data transfer |

## License

This is an experimental project. See individual dependency licenses for their terms.
