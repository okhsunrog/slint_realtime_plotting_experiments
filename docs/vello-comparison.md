# Vello vs the hand-written wgpu renderer — 2026-09-29

Question: should the waveform be drawn by [Vello](https://github.com/linebender/vello)
instead of the custom wgpu pipelines (`shader.wgsl`, `reduce.wgsl`)?

**Result: no.** The custom wgpu renderer is faster, cheaper on the GPU and has
no dependency risk. Vello is viable on desktop, and its only real advantage is
extensibility (fills, markers, gradients, other chart types), which a
time-series plot does not need. Revisit if that changes or if the upstream
situation below improves.

## What was compared

`PlotBackend` (`slint-realtime-plot/src/renderer.rs`), behind the `vello` cargo
feature:

- `Wgpu` — the default pipelines: compute min/max envelope for peak mode and
  instanced line-segment quads for line mode.
- `Vello` — Vello compute renderer. The CPU builds one path per channel each
  frame (polyline in line mode, min/max envelope polygon in peak mode) and
  Vello rasterizes it. Optimized: segments are written straight into Vello's
  encoding, line mode keeps at most 4 samples per pixel column, peak mode is a
  single filled polygon.
- `VelloNaive` — the first version: kurbo paths, up to 8 samples per pixel,
  fill plus stroke of the envelope outline.
- `VelloGpu` — `vello_gpu` (sparse strips, render passes, CPU preprocessing).
- `VelloCpu` — `vello_cpu` (multithreaded, pixels uploaded as a texture).

Selection in the demo: `PLOT_BACKEND=wgpu|vello|vello_naive|vello_gpu|vello_cpu`
(on Android a file `files/plot_backend` with `<backend> [window_seconds]`).
Diagnostics: feature `vello-profile` (per-stage GPU timestamps).

Vello is a git dependency: crates.io `vello 0.10.0` needs wgpu 29 while Slint
1.18.1 uses wgpu 30, so it is taken from git main. The fork
<https://github.com/okhsunrog/vello>, branch `mali-workarounds` (upstream commit
`c750662a` plus the two Mali fixes below), is used. Branch `mali-experiments`
adds two unverified experiments (reusing the placeholder gradient texture and
skipping empty tiles in `fine.wgsl`, which is only correct if the target is
cleared first) and must not be used for rendering.

Upstream status: the compute renderer (`vello` crate) now lives under
`research/` and is described as experimental. `vello_gpu` is the intended
production GPU renderer.

## Desktop (Intel iGPU, 1000x300 texture, 3 channels)

Frame cost in ms including the wait for the GPU (test `compare_backends_png`),
measured with the first, unoptimized Vello integration. Run-to-run spread is
30-50%; read as orders of magnitude.

| Visible samples | wgpu | Vello | Vello GPU | Vello CPU |
| ---: | ---: | ---: | ---: | ---: |
| 50 | 0.46 | 0.94 | 0.60 | 0.86 |
| 600 | 0.25 | 0.82 | 1.24 | 1.12 |
| 2000 | 0.81 | 1.12 | 4.74 | 2.96 |
| 8000 | 1.71 | 2.43 | 16.7 | 9.8 |
| 32768 (peak) | 0.59 | 2.14 | 8.1 | 4.2 |

GPU time only (sum of per-stage timestamps, GPU clock pinned at 800 MHz, median
of 3x300 frames, spread <= 2%), optimized Vello:

| Case | wgpu | Vello naive | Vello optimized |
| --- | ---: | ---: | ---: |
| Peak, phone-size (1120x1770) | 0.47 | 2.12 | 1.33 |
| Peak, 1x (32768 samples) | 0.12 | 1.78 | 0.86 |
| Peak, 2x | 0.31 | 3.39 | 1.72 |
| Line, 8 samples/px | 2.32 | 2.56 | 1.36 |
| Line, <= 2 samples/px | 0.87 | 1.54 | 1.54 |
| Nearly empty scene, phone-size | 0.03 | 0.87 | 0.87 |

Vello's fixed cost per frame is the main gap: about 13 compute stages with
barriers, and fine rasterization writes every pixel of the target. For dense
lines (8 samples/px) the optimized Vello is faster than the wgpu line pass.

## Phone (Pixel 8 Pro, Mali-G715, 120 Hz), 20 s Perfetto captures

Without the Mali fixes, `Vello` loses the GPU device and `VelloGpu` segfaults
the driver (see below), so both only run on the fork.

Unpinned clocks, 3 interleaved rounds with cooldown, medians. "GPU busy" is the
app's GPU residency and includes Slint/Skia. Power comes from the on-device
power rails.

| Config | fps | p95 ms | CPU ms/frame | GPU busy % | GPU active ms/frame | GPU / CPU / memory mW |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| wgpu, peak | 120.1 | 10.95 | 7.77 | 42.5 | 3.54 | 308 / 571 / 232 |
| Vello naive, peak | 119.7 | 11.94 | 8.40 | 87.4 | 7.32 | 495 / 855 / 387 |
| Vello optimized, peak | 120.1 | 11.39 | 8.15 | 66.2 | 5.44 | 407 / 904 / 316 |
| wgpu, line | 120.1 | 10.88 | 7.70 | 38.7 | 3.23 | 227 / 552 / 247 |
| Vello naive, line | 120.1 | 11.86 | 7.85 | 71.6 | 5.97 | 435 / 786 / 341 |
| Vello optimized, line | 120.1 | 11.84 | 7.84 | 68.1 | 5.66 | 455 / 810 / 341 |

Other backends (single runs, before the optimization work): Vello GPU 93 fps
(peak) and 85 fps (line); Vello CPU about 52 fps at ~65 ms CPU/frame.

Per-stage GPU time on Mali (clock pinned at 649 MHz, ms/frame, 2 runs each):

| Stage | Vello optimized, peak | Vello naive, peak | Vello optimized, line | Vello, nearly empty scene |
| --- | ---: | ---: | ---: | ---: |
| flatten | 0.26 | 0.22 | 0.22 | 0.11 |
| path_count | 0.12 | 0.23 | 0.21 | 0.05 |
| path_tiling | 0.08 | 0.18 | 0.15 | 0.04 |
| backdrop_dyn | 0.08 | 0.15 | 0.07 | 0.06 |
| coarse | 0.11 | 0.14 | 0.09 | 0.09 |
| other 8 small stages | ~0.30 | ~0.31 | ~0.27 | ~0.22 |
| fine_area | 2.03 | 2.83 | 2.28 | 1.70 |
| **total** | **2.98** | 4.07 | 3.29 | 2.26 |

wgpu on the same phone: peak 1.42 ms, line 0.99 ms, nearly empty 0.10 ms.
About three quarters of Vello's time in peak mode is the fixed cost of an empty
scene, mostly `fine_area`. Hypothesis (not verified): it writes all pixels
through a storage image, while the wgpu path draws through a render attachment.

## Crashes on Mali-G715 (root causes)

These come from an investigation run on the phone; upstream issue numbers were
reported by it and not opened or verified here (vello#1815, #1816, PR #1817,
wgpu#10186).

1. **Vello compute — `VK_ERROR_DEVICE_LOST` on the first render.** Several
   shaders read `sh[i - 1u]` inside `select(0u, sh[i - 1u], i > 0u)`. Both
   operands are evaluated, so the first lane reads `sh[0xFFFFFFFF]`. Vello
   builds its shaders with runtime checks off and Mali loses the device on the
   out-of-range workgroup read. Fix: `sh[max(i, 1u) - 1u]` in `tile_alloc`,
   `backdrop_dyn`, `coarse` and `fine` (fork commit `7860adbc`).
2. **Vello GPU — SIGSEGV in `vkCreateGraphicsPipelines`.** The Mali r54p3
   shader compiler crashes on `OpImageQueryLevels` for an image passed as a
   function parameter. naga emits it only for image-load bounds checks, which
   wgpu 30 enables because the device reports `robustImageAccess` but not
   `robustImageAccess2`. Fix: build the render and filter shaders without image
   bounds checks (same commit).

Neither is caused by the integration in this repo.

## Recommendation

- Keep the wgpu renderer. On the phone it holds 120 fps at ~40% GPU busy and
  ~3.5 ms of GPU time; optimized Vello needs about 1.5x the GPU time and more
  power in every rail, and only works with patched shaders.
- Vello would help if the widget grows beyond time-series lines (filled areas,
  markers, gradients, other chart types), and only once a Vello release for
  wgpu 30 exists and the Mali fixes are upstream.
- Each Vello `Renderer` holds ~165 MiB of buffers (from the code, not measured)
  and pays its fixed cost per chart; a UI with several charts would need one
  shared renderer.

## Not measured

Startup time and process memory of Vello, energy of the pinned-clock runs, the
effect of timestamp queries on the profiled numbers, other GPUs (Adreno,
NVIDIA, AMD, Apple), WebGPU. The desktop numbers come from a single machine.
Phone runs: the backend comparison (fps, Vello GPU/CPU) has two runs per
configuration for wgpu and Vello CPU and one for Vello and Vello GPU; the
interleaved power series has three rounds; the stage profiles have two runs
each. Temperatures were not identical between series.

The Perfetto harness (adb + root sysfs reads + trace processor scripts) is not
part of the repository; the earlier `docs/android-performance.md` describes the
approach.
