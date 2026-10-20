# Runtime validation, 2026-10-21

The smoke app runs native CEF embedding on Wayland, using the laptop's Intel
Arc / Xe-LPG GPU (PCI 8086:7d55). The isolated KWin compositor uses Mesa 26.2.5;
it does not use a software renderer. GTK is 4.22.4 and Chromium is 154.0.8037.98.

Validated in the isolated session:

- `nativeWindowBackend: wayland`, with the GTK display shared with CEF.
- Intel WebGPU adapter, `isFallbackAdapter: false`.
- GPU compute/readback verifies all 1024 values of `i * i + 7`;
  the final value is 1046536. No WebGPU validation or device-loss error occurred.
- Animated WebGPU canvas, maximize/restore and fractional scaling to 150%.
  At 150%, the page reports DPR 1.5 and viewport 1067 by 705 for the maximized
  window on a 1600 by 1100 virtual output.
- ASCII typing and clipboard insertion of Russian text and emoji.
- Native select popup changes to Second item, and the JavaScript dialog closes
  with Enter.
- Opening and closing `chrome://gpu` leaves the main window alive.
- Three consecutive launches through `run.sh` / Tauri CLI complete with exit
  status 0, including closing the last window with animation running or paused.
  The frontend port is released after each exit.
- A fourth launch uses the installed complete SDK without a `CEF_PATH`
  override. Compute passes, maximize/restore changes the viewport from
  1100 by 900 to 1600 by 1072 and back, and the launcher exits with status 0.
- The main thread used about 22% of one CPU core while animating and about 1%
  with the animation paused, in two-second samples. Before the GLib idle fix,
  it used about 98% even while waiting.

`chrome://gpu` reports hardware WebGPU and hardware WebGPU interop, and lists
Intel Arc under Vulkan Information. Its top-level `Vulkan: Disabled` item
refers to Chromium's compositor. Presentation uses ANGLE/OpenGL; Dawn's Linux
WebGPU default backend selects Vulkan when `ForceEnableWebGpuInterop` is active
(`gpu/command_buffer/service/webgpu_decoder_impl.cc`).

On NixOS the system Vulkan loader is required in this SDK. The original
bundled loader failed the `vkDestroySurfaceKHR` lookup and selected SwiftShader.
`configure-runtime.sh` applies the verified host-loader workaround without
changing the downloaded archive. No GPU or global sandbox bypass was added
as a workaround for that failure.

Evidence is retained in `logs/cef.log`, `logs/webgpu-report.json`,
`logs/webgpu-report-scale-1.5.json`, `logs/chrome-gpu-final-tree.json`,
`logs/cpu-after-idle-fix.txt` and `logs/cpu-paused-after-glib-fix.txt`.
The final shutdown checks are recorded in `logs/final-cycle-1-exit.json`,
`logs/final-cycle-2-exit.json` and `logs/final-cycle-3-exit.json`.
The complete-SDK check is in `logs/full-sdk-cli.json`,
`logs/full-sdk-cli-exit.json` and `logs/final-wayland.png`.
Cloud artifacts include `validated-provenance.json` with the CEF commit,
experimental API hash, library checksum and checksums of the Chromium patches.

Manual acceptance remains for physical multi-monitor transitions, complex IME
and touch input. GTK does not expose its parent xdg_surface through its public
API, so native popup fallback is confined to the host window and may lack
outside-click dismissal. Individual Wayland webview hiding and cross-window
reparenting are not implemented. The VM and its build disk must remain retained
until the user explicitly confirms acceptance.
