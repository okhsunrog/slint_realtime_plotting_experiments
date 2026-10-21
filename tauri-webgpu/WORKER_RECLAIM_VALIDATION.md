# Consumed WebSocket buffers transferred to a Worker

Date: 2026-10-21. This experiment moves ArrayBuffer backing-store cleanup away
from the renderer main thread. It does **not** establish a physical-display
frame-pacing improvement. The default remains `reclaim=main`.

## Implementation

`reclaim=worker` changes only WebSocket acknowledgement and consumed-buffer
ownership. The Rust receiver finishes copying/decoding into owned WASM vectors,
updates the CPU ring and uploads samples before the original JS ArrayBuffer is
transferred. Its byte length must immediately become zero. The Worker receives
the buffer, replies with its ID and byte length, then retains no reference.
The reply releases the backend's existing WebSocket credit, preserving a maximum
of four outstanding packets per connection. A closed stream receives no late
acknowledgement. Startup, transfer and receipt failures are reported explicitly;
there is no silent fallback. A Worker round trip is an additional cost.

This bounds the pending message queue, not all allocated memory: unreachable
buffers still await Worker GC. No SharedArrayBuffer, COOP/COEP or new CEF build
is needed. Other transports keep their previous ownership path.

## Reproduction and provenance

From `tauri-webgpu`:

```sh
./build.sh
node diagnostics/check-reclaim.mjs
WAVEBENCH_QUERY='renderer=webgpu&measure=worker&rate=4000000&batch=16384&transport=ws&motion=clocked&reclaim=worker&seconds=30' \
WAVEBENCH_REPORT_PATH="$PWD/logs/worker.json" ./run.sh
```

Use `reclaim=main` for the control. Compare actual delivery, geometry, errors,
transfer/receipt counters and host load. Keep the same SDK, native reader,
binary and rendering settings in each comparison. `reclaim.start/end` counters
include warmup outside the sample; subtract endpoints for sample totals.

Raw artifacts, controller scripts and logs are in
`/home/okhsunrog/tmp_zfs/codex_tmp/cef-worker-reclaim/`:
`provenance.json`, `summary.json`, `*-trace.json`, `*-analysis.json`,
`*-host.json`, `*-gpu.json`, measurement JSON and journals. They are not committed.
The identical embedded-frontend executable used in all these runs has SHA256
`5555b39b7c3c14ae68157e2fbcd52192ac488d2f696de8091562719f281caf61`.

Profile and repeated runs use the previously validated diagnostic SDK
`ys07am0ally0gnac5bbgbr7a7b0wldfi-cef-wayland-finalizer-profile-154.0.8037.98-profile-0e805fc`,
libcef SHA256 `a7c5e2b0d3e62bac7f544c9fe1f7af1f6cff419f48b11313e7a83b21e74eef1e`.
Its overlays and measurement limitations are documented in
[FINALIZER_PRESENTATION_VALIDATION.md](FINALIZER_PRESENTATION_VALIDATION.md).
GTK shared-display reader, hardware Intel WebGPU/Vulkan, compute enabled,
clocked playback, WebSocket batch 16384, requested 4M three-channel frames/s.
The isolated KWin session uses DPR 1 and a physical canvas of 1280 × 368.
Virtual presentation is not acceptance on the user's physical monitor.

## Ownership and stream checks

`diagnostics/check-reclaim.mjs` executes the browser helper in a small Node
harness with real `worker_threads` transfer semantics. It verifies that the
independent copied payload survives source detachment, acknowledgements wait
for actual receipts and remain ordered, the fifth outstanding packet is rejected,
closed connections receive no late ACK, and the default path creates no Worker
and retains its source buffer. This supplements browser checks; it does not
emulate CEF, WASM or graphics.

The initial five-second CEF/WebGPU smoke delivered 4.0035M frames/s. All 2490
buffers over startup/warmup/sample were detached and acknowledged, outstanding
receipts ended at zero and the peak was four. Compute reported
`PASS · 102 extrema · wrapped ring + spike`, hardware adapter, no errors and
`Gaps 0`. Every completed repeated run also passed sequence/finite-payload
validation, compute checks and ownership counters.

Two additional five-second Worker smoke checks used the normal released reader
SDK `h6cp61c4k0s68kvbax768wzbvz8yajfm-cef-wayland-binary-154.0.8037.98-a09c27de3-reader-1ad8496`
with matching resources. WebGPU delivered 4.0011M frames/s, compute PASS;
WebGL2 delivered 4.0028M frames/s with compute disabled as intended. Both had
no errors, `Gaps 0`, every consumed buffer detached, peak outstanding four and
zero outstanding receipts at the endpoint. This confirms that the ownership
path does not depend on the experimental CEF finalizer instrumentation.

## Finalizer attribution

Two 15-second samples, with `WAVEBENCH_CPPGC_FINALIZERS=1` and trace categories
`toplevel,blink.user_timing,v8,disabled-by-default-v8.gc,cppgc,disabled-by-default-cppgc,viz,wayland`:

| Sample | Main ArrayBuffer count / inclusive ms | Worker ArrayBuffer count / inclusive ms | Main cppgc union / longest ms |
| --- | ---: | ---: | ---: |
| `virtual-main-profile` | 3406 / 112.356 | 0 / 0 | 176.373 / 14.036 |
| `virtual-worker-profile` | 2487 / 2.551 | 3060 / 116.613 | 72.740 / 7.596 |

The Worker trace identifies a `DedicatedWorker thread` executing the formerly
expensive ArrayBuffer finalizers. Detached wrappers still finalize on main, but
their backing-store release is no longer there. This directly supports the
ownership mechanism. Counts differ from arrivals because objects can cross GC
and sample boundaries. These scopes measure inclusive wall time with tracing
overhead; they are not exact destructor CPU costs.

The Worker profile also had substantially greater host load (median/max
15.75/64.31% versus 6.50/12.49%) and lower delivery (3.8680M versus 3.9992M).
Its RAF results are not evidence of a performance regression or improvement
caused by ownership. Worker cppgc reached a 29.151 ms merged interval, which
can delay credit replies even though it no longer pauses the renderer main
thread. The analyzer now reports finalizers per thread and merges nested cppgc
scopes independently on each thread, including sweep on allocation.
Both trace exports reported `dataLossOccurred:false`.

## Repeated runs without per-object profiling

Five-second warmup, then 30 seconds per sample, order main/worker/worker/main:

| Sample | Actual Mframes/s | RAF intervals >25 ms / max ms | Host CPU median / max % | App cgroup memory min / max / end MiB |
| --- | ---: | ---: | ---: | ---: |
| `virtual-1-main` | 3.9978 | 8 / 33.4 | 6.23 / 12.19 | 464 / 677 / 508 |
| `virtual-2-worker` | 3.9983 | 8 / 33.4 | 6.49 / 11.85 | 473 / 631 / 539 |
| `virtual-3-worker` | 3.8323 | 32 / 66.7 | 18.94 / 70.33 | 477 / 631 / 491 |
| `virtual-4-main` | 3.9966 | 2 / 33.4 | 6.17 / 11.64 | 523 / 728 / 542 |

CPU and memory columns use one-second host samples after the first seven seconds
of application startup. Memory includes the entire application cgroup and cache;
it is not live ArrayBuffer size. These short runs do not establish absence of a
long-term leak. Diagnostic artifact hashing also ran during the third sample;
that sample is retained as a loaded functional check, not a matched timing run.

Worker samples transferred 7321 and 7018 packets inside their sample windows,
all detached and all acknowledged by the end, peak outstanding four, no errors
or sequence gaps. Under the lower observed host load, the transport maintains
the requested rate; the extra Worker round trip did not impose an observable
4M frames/s throughput ceiling in these runs.

RAF gaps remain in both modes. The virtual series supports functional delivery
and relocated finalization, not a repeatable smoothness win. Physical Wayland
presentation/discard measurements need a separate quiet-host series; no new
physical-screen result is claimed here. In particular this experiment does not
resolve the native presentation/discard behavior documented in the previous
report. Keep the mode opt-in until that comparison is available.
