# Wavebench: Tauri / CEF + Leptos + WebGPU

A second demo alongside the existing Slint application. The Rust backend generates
three interleaved current channels; the Rust/WASM frontend renders them with
`wgpu` on browser WebGPU, or an explicitly selected WebGL2 renderer without
compute shaders. Leptos owns the controls and telemetry. Both renderers keep
reduction on the GPU; the source and transport remain the same.

Uses pinned commits from our [Tauri](https://github.com/okhsunrog/tauri/tree/feat/cef-wayland)
and [cef-rs](https://github.com/okhsunrog/cef-rs/tree/feat/wayland-154) forks.
Dawn WebGPU uses Vulkan with `ForceEnableWebGpuInterop`; window presentation
uses ANGLE/OpenGL. The [custom CEF SDK](https://github.com/okhsunrog/cef/tree/feat/wayland-154)
is packaged by [nix/cef-wayland-binary.nix](nix/cef-wayland-binary.nix).
Cargo dependencies use exact Git revisions and nixpkgs is pinned with a verified
fetch fallback. Import the matching SDK archive into Nix before building; a
stock CEF SDK lacks the embedding API.

[smoke/](smoke/) preserves the original TypeScript smoke app, input/scaling
checks and their validation history. [sources.json](sources.json) records the
source and artifact revisions for the complete stack.

## Build and run

```sh
cd tauri-webgpu
./build.sh
./run.sh
```

Prerequisites: Rust with `wasm32-unknown-unknown`, `wasm-pack`,
Nix, and the custom CEF archive imported as described in
[nix/README.md](nix/README.md). The build selects the immutable SDK through
`CEF_PATH`; the launcher clears
`LD_LIBRARY_PATH`. Both frontend and backend are built in
release mode. `build.sh` embeds the generated WASM and assets into the native
application, so no frontend server or internet connection is needed at runtime.
Rebuild after changing frontend code. The separate Cargo workspace leaves the
original Slint dependencies and build configuration intact.

For a WebKitGTK comparison using the same Tauri 3 branch and Rust backend:

```sh
./build.sh webkit
./run-webkit.sh
```

This uses the optional Wry runtime and defaults to WebGL2. It is a WebKitGTK
control, not a reconstruction of Tauri 2. The Nix shell supplies GTK 3 and
WebKitGTK 4.1. CEF and WebKitGTK are separate feature builds; only one runtime
may be enabled. The scripts retain `wavebench-cef` and `wavebench-webkit`
executables alongside the normal Cargo output so either can be launched.

Select **Renderer → WebGL2 / no compute** in CEF, or open with
`?renderer=webgl`. Switching reloads the page and resets stream/view settings.
WebGL2 stores the interleaved ring in an R32F texture and computes column extrema
in its vertex shader. It has no storage buffers or compute dispatches and does
not call `navigator.gpu`. Extrema are recomputed by quad vertices, so its GPU
cost differs from the compute pipeline; equal input does not mean equal GPU work.
There is no automatic WebGL fallback on WebGPU failure. WebGL requests a context
with `failIfMajorPerformanceCaveat`; actual hardware acceleration still needs
runtime verification. Context loss is reported and requires reloading.

Set `WAVEBENCH_TMP` to select a temporary directory on a large disk.

Logs are retained in `logs/runtime.log`, `logs/cef.log`,
`logs/frontend-build.log` and `logs/backend-build.log`. **Save report** writes
`logs/benchmark.json`; copy it before the next comparison overwrites it.

## Try it

- Choose a transport, frame rate and batch size, then **Apply / restart**.
  A frame contains all three channels: 1 Mframe/s means 3 million floats/s,
  or 12 MB/s of raw samples. The controls go up to 8 Mframe/s.
- Zoom with the wheel or buttons. **Freeze plot** stops updating the GPU ring
  while reception and transport metrics continue. Drag to pan frozen history;
  **Resume plot** starts a fresh history at the current stream position.
- Change frequency or amplitude and apply. The narrow spikes are intentional:
  they exercise peak detection when many samples fall into a screen column.
- **Compare all** runs each of the five transports twice, in opposite orders, with a 2 s
  warmup and a 6 s measurement window per run. Keep the same window size and
  view settings while comparing. Try both large and small batches; increasing
  batch size trades batching latency for lower per-message overhead.
- **GPU diagnostics** opens CEF's `chrome://gpu`. Its compositor Vulkan status
  is separate from Dawn's Vulkan WebGPU backend.

## Transports and flow control

| Mode | Backend → frontend | Consumer acknowledgement |
| --- | --- | --- |
| JSON Channel | `Channel<InvokeResponseBody::Json>` with a numeric array | `acknowledge` invoke |
| Binary Channel | `Channel<InvokeResponseBody::Raw>` / `ArrayBuffer` | `acknowledge` invoke |
| Binary invoke pull | raw `tauri::ipc::Response` | next pull request |
| Custom protocol fetch | `register_asynchronous_uri_scheme_protocol` + `fetch().arrayBuffer()` | next GET request |
| Binary WebSocket | local binary WebSocket / `ArrayBuffer` | sequence number on the socket |

The custom-protocol mode follows Tangleaf’s attachment delivery mechanism: IPC
returns a resource identifier; the browser obtains bytes directly from a Rust
URI handler. Unlike immutable image attachments, the benchmark responses use
`Cache-Control: no-store`, a per-stream token and one consumed packet per GET.

The binary transports use the same versioned little-endian 64-byte header and
interleaved `f32` payload. Samples are generated on a dedicated Rust thread,
using a precomputed three-phase waveform table and absolute frame indices.
All modes use the same source, batching, pacing and GPU renderer.

The producer has four packet credits. JSON/Binary Channel and WebSocket can
pipeline four packets; invoke/protocol pull deliberately measure the simpler sequential
request/response approach. The queue is bounded. The producer slows down under
backpressure instead of dropping packets or growing a queue indefinitely. A
low achieved rate therefore means saturation, not successful delivery at the
requested rate. There is no deliberate decimation before delivery to WASM.
WebSocket listens only on loopback at an ephemeral port; a random per-stream
URL token prevents other pages from joining the current stream.

## GPU and measurements

Both renderers keep a 1,048,576-frame ring (12 MiB) on the GPU. WebGPU uploads
one contiguous range per batch, or two when wrapping. WebGL2 uploads texture
rows, splitting at row and ring boundaries. In WebGPU, each rendered frame runs a compute
shader over every contributing sample to produce min/max for each physical
pixel column and channel. The render pass draws those extrema and the grid.
Zoomed-in rendering connects adjacent column values. Canvas backing dimensions
follow CSS size × device pixel ratio; no native-only push constants are used.

On WebGPU startup the **actual reduction shader** is run on a known wrapped ring,
including a narrow spike. All 102 extrema are read back and compared with a CPU
reference. Validation errors, device loss and packet-sequence discontinuities
are recorded. **PASS** covers this deterministic compute check, not every
possible GPU/driver behavior. WebGL2 explicitly reports that compute is disabled;
it does not run or claim this readback check.

Metrics:

- **Payload MiB/s:** received sample bytes, excluding JSON expansion, binary
  headers and protocol overhead. Rates across modes therefore compare useful
  data, not bytes on the wire.
- **Delivery p95:** packet creation after source generation to entry into WASM.
  Clocks are calibrated using the minimum RTT of nine IPC exchanges; RTT is
  saved in the report. This excludes the time needed to accumulate a batch of
  samples at a real acquisition device.
- **WASM p95:** JS→WASM copy/decoding, CPU history copy and GPU upload calls (`queue.write_buffer` or
  `texSubImage2D`). JSON parsing before callback entry is counted in delivery latency.
- **Frame p95/p99:** intervals between `requestAnimationFrame` callbacks.
  **Render fps** counts render submissions. These do not prove compositor
  presentation timing or absence of every dropped frame.
- **CPU submit p95:** compute/render encoding and submit calls, not GPU execution
  time. Backend generation, delivery/encoding and credit-wait statistics are
  also saved; those backend averages include warmup.

The frontend keeps a bounded history of timing samples. `wasm-bindgen` provides
thin JavaScript glue for Tauri's official Channel API, timers and adapter info;
UI, transports, sample processing, transport benchmarking and rendering are
written in Rust. The optional frame-timing probe described below is JavaScript.
There are still copies across process and JS/WASM boundaries: this does not
claim to be shared-memory or zero-copy IPC.

## Checks

```sh
cargo test -p plot-wire
# Inside the same Nix shell and with CEF_PATH selected by build.sh:
cargo test --release -p tauri-webgpu-plot
cargo fmt --all -- --check
```

Runtime measurements and screenshots are described in [VALIDATION.md](VALIDATION.md).

## Frame timing probe

The opt-in `frontend/measure.js` probe uses the same rAF wrapper in both engines.
It applies binary WebSocket, batch 16384, a five-second warmup and a physical
1280 × 368 canvas, then records raw callback timestamps and CPU durations.
Normal launches do not install the wrapper. For example:

```sh
WAVEBENCH_QUERY='renderer=webgl&measure=cef-webgl&rate=1000000&seconds=30' \
WAVEBENCH_REPORT_PATH="$PWD/logs/frame-timing.json" ./run.sh
```

Use `renderer=webgpu` for CEF/WebGPU, or the same WebGL query with
`./run-webkit.sh` for WebKitGTK. The report's parent directory must exist. Query
settings reload the app; reports distinguish the runtime, renderer, DPR and
physical canvas dimensions. The probe fixes canvas flex shrinking so scrollbar
and toolkit differences do not silently change the rendering workload.

These are rAF scheduling measurements, not compositor presentation timestamps.
The frame probe is diagnostic JavaScript; the application and transport benchmark
remain Rust/WASM. WebKit may have coarser timer precision and no longtask observer.
Repeat runs in reverse order and do not measure while building or under unrelated
host load. CEF uses the basic password store for this credential-free benchmark,
preventing a fresh virtual-session KWallet prompt from blocking startup.

The WebKitGTK host supplies a valid GDK font DPI before its first WebView only
when GDK reports an unset/nonpositive value, using GTK's configured font DPI or
96 as fallback. This prevents WebKitGTK 2.54's negative-DPR startup issue while
preserving valid existing font DPI and device scale.

## Scrolling clock

**Scrolling → Clocked / buffered** advances the live view at the configured
sample rate between packets. It retains a small receive buffer (two batches plus
one 60 Hz frame interval) instead of snapping to the latest packet. The plot bar
shows the current buffer in milliseconds. This adds display latency; the delivery
metric still measures transport latency, excluding that display buffer. Large
batches at low sample rates therefore produce a visibly larger delay.

**Packet head** (`?motion=packet`) retains the original arrival-driven view for
comparison. Switching scrolling mode reloads the page, as switching renderers
does. Frozen history retains the last displayed position. Zooming out to the
entire ring leaves no history available for buffering. On starvation, the view
stops at received data and rebuilds its buffer; it never predicts missing samples.
The opt-in `measure` probe also records `[rAF timestamp, displayed head, received
head, offset]` for checking movement independently of callback cadence.

For controlled native-runtime experiments only,
`WAVEBENCH_ALLOW_CHROMIUM_ARGS=1` enables forwarding explicit Chromium arguments
from the executable command line. Release builds otherwise disable those
arguments. A reader-mode comparison must verify the actual `wayland-fd` thread
presence/absence; supplying a command-line string alone is not proof that CEF
received the switch.

The GTK-owned native reader is experimental and remains opt-in. Paired tests did
not establish a consistent frame-pacing improvement, so the default CEF watcher
is preserved:

```sh
WAVEBENCH_WAYLAND_READER=gtk ./run.sh
WAVEBENCH_WAYLAND_READER=threaded ./run.sh
```

This selects CEF's native event integration independently of **Scrolling** and
**Renderer**. It is supplied through the runtime API and does not require allowing
arbitrary Chromium command-line arguments.

After a delayed frame or producer burst, scrolling catches up if its lag exceeds
that target by more than one batch. This keeps stale display history from
accumulating latency; a recovery may make a corrective jump. The transport still
receives every packet, and retained ring data remains available for frozen
history. The backlog bound has a focused regression test in `playback.rs`.
