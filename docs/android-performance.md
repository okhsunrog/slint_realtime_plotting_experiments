# Pixel 8 Pro plotting comparison — 2026-09-21

Both versions use Slint **1.18.1**, release builds, the same runtime dependency
lockfile, Android 17, Vulkan on Mali-G715, and the same connected Pixel 8 Pro.
The display is 1344×2992 portrait at 120 Hz (not the plot texture dimensions).
The demo generates three channels at 20 kSa/s, capacity 32768, with default
amplitude/frequency. Peak uses a 1.0 s window; line uses 0.1 s.

Baseline: `8ab951f78b3c4764b938a1d337210c05a0d039db`, with only the Slint
release dependency update and runtime `CARGO_MANIFEST_DIR` build-script fix.
It was built in a separate detached worktree; the main checkout stayed new.
New runtime: `ec2b571` (subsequent `7a29913` only adds documentation/tests).
The new APK is installed and running on the phone after the comparison.

## Measurements

Two accepted 20-second Perfetto captures per mode/version. Ranges below are
the two runs, not confidence intervals. FPS measures the cadence of application
`QueuePresentKHR` calls; CPU is the sum of scheduled time of all app threads
divided by that capture's frame count, not wall-clock render latency.

| Mode | Version | Producer FPS | Interval p95, ms | CPU, ms/frame | GPU UID residency / elapsed |
| --- | --- | ---: | ---: | ---: | ---: |
| Peak | Old | 73.11–73.69 | 16.62–16.91 | 10.44–10.58 | 91.5–91.6% |
| Peak | New | 119.99–120.00 | 10.37–11.10 | 7.28–7.54 | 38.7–39.1% |
| Line | Old | 101.57–104.78 | 12.22–12.38 | 9.09–9.18 | 88.5% |
| Line | New | 120.01 | 10.85–10.95 | 7.59–7.60 | 40.3–42.8% |

Thus peak throughput increases about 64%, line about 16%, both reaching the
display refresh ceiling. CPU time per frame falls about 30% and 17%, respectively.
Total CPU utilization does not necessarily fall: the new version produces more
frames per second (roughly 86–89% of one core versus 71–72% in old peak mode).

The driver attributes substantially less GPU residency to the app even at the
higher frame rate. Residency-weighted GPU clock is 847–853 MHz for old peak,
647–652 MHz for new peak; old/new line ranges overlap (666–691 / 629–695 MHz).
These are **not isolated shader timings or energy measurements**.

## Interpretation and limitations

- Peak computes min/max once per column/channel in a compute pass, then reads
  that envelope in the fragment shader. It no longer rescans samples per pixel.
- Line uses narrow instanced segment quads instead of a fullscreen segment loop.
  This change was essential on the phone: an intermediate fullscreen line
  implementation reached only about 23 FPS and is excluded from the final table.
- Ring snapshots are consistent under a short mutex; only changed ranges are
  copied/uploaded, except initial synchronization, reset, or producer overrun.
- Temperatures were recorded, not controlled. GPU temperature endpoints were
  65–68°C for old peak, 51–59°C for new peak, 59–61°C for old line and 49–51°C
  for new line. Battery endpoints ranged 32.8–37.4°C across accepted runs.
  No clocks, thermal limits, or display settings were overridden. This is a
  practical device comparison, not a thermally matched laboratory experiment.
- NativeActivity emitted no application FrameTimeline tokens and `gpu_slice`
  was empty. Therefore no trustworthy FrameTimeline jank rate or direct GPU
  execution duration is reported. Empty jank tables do not mean zero jank.
- SurfaceFlinger layer histograms corroborate the new version's ~8 ms display
  cadence. Its histogram-derived `averageFPS` rounds to ~125; the actual display
  is 120 Hz, so that field is not used as an FPS measurement.
- Mali `uid_time_in_state` reports per-UID frequency residency in milliseconds;
  it does not isolate plotting from the rest of the app/driver. See Google's
  [driver implementation](https://android.googlesource.com/kernel/google-modules/gpu/+/bbd2fe3e6a0294b8dddaaa5a86be53a6b505d1d5/mali_kbase/platform/pixel/pixel_gpu_sysfs.c).

## Evidence and reproduction

Local artifacts (ignored, not committed): `target/plot-benchmark-2026-09-21/`.
This contains APKs, screenshots, raw traces, CPU/thermal/driver snapshots,
SurfaceFlinger dumps, query results and the capture/analysis scripts.
Accepted labels: `baseline-peak-1`, `baseline-peak-3`, `baseline-line-1`,
`baseline-line-3`, `final-peak-1`, `final-peak-2`, `final-line-1`, `final-line-2`.
The two baseline `*-2` captures are excluded because their sessions overlapped
and the window changed near the end of the peak capture. `optimized-*` labels
are intermediate implementations, not the final runtime.

The harness uses `adb`, root access to Mali sysfs, and Perfetto trace processor
58.2. Adapt its serial, UID, and trace-processor path before reuse. Launch the
app, allow startup and ring filling, select a mode, then run captures strictly
sequentially without touching the screen:

```sh
uv run python capture.py LABEL
uv run python analyze.py LABEL
uv run python table.py LABEL
```

Host validation passed for both plotting crates: buffer concurrency/incremental
tests and explicit GPU readback tests, including peak spikes beyond index 256,
clear/reset, invalid samples, mode switching, and diagonal lines on non-square
textures. Workspace/host builds also passed. Android launch and both rendering
modes were checked visually on this phone; the oxifoc application itself was
not tested on Android.
