# Runtime validation, 2026-10-21

Tested locally in an isolated hardware-rendered KWin Wayland session, virtual
output 1800 × 1300, app content 1440 × 1000 at DPR 1.0. CEF 154.0.8037.98,
GTK 4.22.4, Mesa 26.2.5, Intel Arc / Xe-LPG. Both Rust/WASM and the native
application use release builds. This was an interactive laptop session, not
an otherwise idle laboratory machine; the two passes illustrate run-to-run
variation and are not a statistical confidence interval.

The real WebGPU reduction shader passed CPU comparison for all 102 extrema,
with wrapped indexing and a narrow spike. The selected browser adapter
reported `intel xe-lpg` and `isFallbackAdapter: false`. Native Wayland embedding
is confirmed by the runtime log.

## Small packets: per-message overhead

Requested 4,000,000 frames/s, 1024 frames/packet, three f32 channels.
Each transport ran twice in opposite order, with 2 s warmup and 6 s measurement.
Rates below are useful sample bytes; the requested rate would be 45.78 MiB/s.

| Transport | Payload MiB/s, passes 1 / 2 | Delivery p95 ms, passes 1 / 2 |
| --- | --- | --- |
| JSON Channel | 1.12 / 1.63 | 57.31 / 43.61 |
| Binary Channel | 1.15 / 2.20 | 46.75 / 27.47 |
| Binary invoke pull | 2.01 / 2.57 | 31.48 / 25.14 |
| Custom protocol fetch | 2.95 / 3.15 | 21.20 / 21.16 |
| Binary WebSocket | 39.54 / 39.64 | 1.97 / 1.98 |

All ten runs have zero sequence discontinuities and zero recorded WebGPU
errors. WebSocket rendered about 59–60 fps, frame interval p95 16.8 ms;
WASM decode/copy/upload-call p95 was 0.1 ms. Other modes were limited by the
message path at this batch size. No transport dropped samples intentionally;
backpressure reduced the achieved source rate instead. WebSocket did not
fully reach the requested rate with these small batches and the paced source.

The custom protocol follows the mechanism in Tangleaf's
`src-tauri/src/attachment_protocol.rs` and
`src/features/markdown/use-attachment-images.ts`: a separately registered
asynchronous URI handler and direct browser resource requests. This benchmark
uses uncached sequential `fetch().arrayBuffer()` requests for fresh packets;
it does not measure `<img>` decode, image caching, or parallel image loading.

Evidence: `logs/compare-4m-1024.json`. `logs/baseline-1m-16384.json` contains
an earlier four-mode exploratory comparison before the custom protocol was
added; it is not the final five-mode result.

## Large packets: sustained sample bandwidth

Same requested 4,000,000 frames/s, now 16,384 frames/packet. Final release build,
all five modes twice; no sequence discontinuities or recorded GPU errors.

| Transport | Payload MiB/s, passes 1 / 2 | Delivery p95 ms, passes 1 / 2 |
| --- | --- | --- |
| JSON Channel | 7.38 / 4.40 | 186.63 / 373.39 |
| Binary Channel | 27.88 / 22.13 | 37.15 / 44.25 |
| Binary invoke pull | 34.24 / 37.21 | 26.88 / 24.76 |
| Custom protocol fetch | 34.95 / 39.15 | 26.40 / 22.80 |
| Binary WebSocket | 45.75 / 45.65 | 2.20 / 2.37 |

WebSocket reached approximately the requested useful bandwidth (48 MB/s =
45.78 MiB/s), rendered 59.8 fps in both passes and had frame-interval p95
16.7 ms. Its WASM decode/copy/upload-call p95 was 0.3 ms. JSON saturated the
main-thread path and showed substantial second-pass degradation; this is why
both passes, rather than only the better number, are reported.

**Use binary WebSocket for this CEF streaming workload.** It is the default.
This result is specific to these implementations, pacing, credit window and
machine. Sequential protocol/invoke pull do not test a pipelined multi-request
implementation, and these measurements do not establish an absolute transport
ceiling or results for Tauri's other webview runtimes.

Evidence: `logs/compare-4m-16384.json`; `logs/benchmark.json` is the latest saved
report and may change when the application is used. Runtime logs and reports
are local ignored artifacts. Re-run **Compare all** to regenerate them.

## Interaction and lifecycle

- Native select controls and Apply/restart were exercised with real EIS input.
- Freeze/resume, zoom and pointer dragging in frozen history were exercised.
- While frozen, changing output scale to 150% and maximizing resized the GPU
  backing buffer to 1559 × 477 at DPR 1.5. No validation/device-loss error was
  recorded. Returning to 100% and resuming restored live rendering.
- `chrome://gpu` confirms hardware WebGPU and Intel Arc. Vulkan Information
  lists the Intel device; compositor presentation remains ANGLE/OpenGL.
- Repeated normal launches and closures exit with status 0, including opening
  and closing the secondary GPU diagnostics window.
- Wire format/generator unit tests, producer-credit-window unit test, release
  builds, Rust formatting and shell syntax checks pass.

Additional local evidence: `logs/interaction-100.json`,
`logs/interaction-150.json`, `logs/interaction-100-return.json`,
`logs/wavebench-frozen-150.png`, `logs/gpu-diagnostics-tree.json` and
`logs/backend-tests.log`. Physical mixed-DPI monitor movement and GPU execution
profiling with timestamp queries were not part of this test.

## Immutable Nix SDK integration

The SDK is now `cef-wayland-binary-154.0.8037.98-a09c27de3`, produced by
`nix/cef-wayland-binary.nix`. The local tested store path is
`/nix/store/ami12j9nqmk4li256finjqnx1ilgr9m1-cef-wayland-binary-154.0.8037.98-a09c27de3`.
The original archive and libcef checksums are checked before ELF patching;
packaged and staged libcef both have SHA256
`c7760d839c6b0f75a4a72eee5b2fe71d602539946df24c3f1290ae9dcc8f06cb`.

- The package install check resolves libcef dependencies without LD_LIBRARY_PATH.
- Wavebench was rebuilt with CEF_PATH pointing at the immutable store SDK.
  Re-running its build script against read-only staged files succeeds.
- The executable also has explicit Nix RPATHs for its own GTK dependencies;
  libcef's RUNPATH alone does not resolve the executable's direct dependencies.
- The GUI test intentionally supplied invalid legacy CEF_PATH/LD_LIBRARY_PATH
  values. The launcher selected the Nix package and removed LD_LIBRARY_PATH;
  `/proc/<pid>/environ` confirms its absence. Process maps show GTK4 and the
  Nix Vulkan loader, and staged libcef matches the package checksum.
- Native Wayland embedding, Intel hardware WebGPU, the animated waveform and
  all 102 compute readback comparisons pass. No sequence gaps or GPU errors
  were recorded. `chrome://gpu` confirms hardware WebGPU and Intel Vulkan info.
- Opening/closing GPU diagnostics and normal application shutdown exit cleanly.

Local evidence: `logs/nix-webgpu-report.json`, `logs/nix-runtime-tree.json`,
`logs/nix-gpu-diagnostics-tree.json`, `logs/nix-executable-ldd.txt`,
`logs/nix-repeat-build.log` and `logs/nix-exit.json`. Package build logs and
loaded-library/environment evidence are under
`/home/okhsunrog/tmp_zfs/codex_tmp/tauri-webgpu-nix/`.

## WebGL2 / WebKitGTK comparison preparation (2026-10-21)

Added an explicit WebGL2 renderer and an optional WebKitGTK/Wry host using the
same pinned Tauri 3 branch. WebGL2 uses an R32F texture ring and vertex-shader
min/max; it does not invoke WebGPU or compute. This follows the older plot's
non-compute approach in spirit; it does not reproduce its storage-buffer WGSL
shader verbatim (that shader is not WebGL compatible).

Runtime checks and timing comparisons are **pending**: the user requested that
they be deferred while the laptop is under unrelated load. No rendering,
acceleration, WebGL extrema readback, or microstutter result is claimed here.
Later compare CEF/WebGPU, CEF/WebGL2 and WebKitGTK/WebGL2 at equal transport,
rate, batch size, view and physical canvas size. WebGL repeats reduction work
across quad vertices, so GPU execution costs differ from the compute pipeline.
Switching renderers reloads and resets controls/history; reapply equal settings.

Compilation checks: WASM frontend `cargo check` and release compilation passed;
WebKitGTK/Wry native `cargo check --no-default-features --features
custom-protocol,webkit` passed. The CEF native release build passed. These checks
do not compile the GLSL through a browser driver or validate the WebKitGTK
executable at runtime; both remain part of the deferred GUI checks.
