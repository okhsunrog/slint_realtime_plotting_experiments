# Finalizer attribution and presentation feedback

The dominant timed cppgc finalizer type is `ArrayBuffer`. In the repeat with
YouTube paused it accounts for 220.83 of 293.92 ms (75.1%) across 6597 callbacks.
All types containing `GPU` together account for 37.71 ms; the compute-pass
encoder accounts for 2.25 ms. Disabling compute does not remove the expensive
ArrayBuffer finalization observed in the exploratory WebGL run.

There is also a native presentation issue to investigate. A separate ten-second
Wayland diagnostic received five actual `wp_presentation_feedback.discarded`
callbacks despite steady RAF timing. This localizes a second investigation to
Chromium/Wayland/compositor frame pacing; it does not establish a KWin bug.
Neither the default transport nor the normal SDK was changed.

## Diagnostic SDK and method

The optional overlays are in the CEF fork:
https://github.com/okhsunrog/cef/tree/experiment/cppgc-finalizer-profile/tools/wayland-build/experiments/cppgc-finalizers.
They extend the per-page deadline candidate from [CPPGC_VALIDATION.md](CPPGC_VALIDATION.md).

`HeapObjectHeader::Finalize` records class names before destruction, then wraps
actual callbacks in opt-in Perfetto scopes. `FinalizePage` adds page scopes.
Individual scopes include tracing overhead, so their durations are inclusive
wall-time attribution, not an exact CPU-cost census. Counts are finalized
objects, not all allocations; GC timing can carry objects across sample boundaries.
GCInfo indices are process-local and are not compared as stable type IDs.

Viz instrumentation records raw/sanitized feedback flags and timestamps.
A separate marker in Wayland `OnPresented` records actual successful protocol
callbacks, avoiding Chromium's synthetic-feedback fallback. For failure
attribution the final diagnostic also enables Chromium's existing `wayland`
category, matching `StoreFeedback` frame IDs to the enclosing
`WaylandFrameManager::HandleFeedback(discarded=true)` scope.

The executable is frozen across all SDK and instrumentation switches. It uses
hardware Intel Vulkan WebGPU, with the 102-extrema compute check passing;
WebGL runs explicitly disable compute. Actual delivery is measured from
received sample counters. All streams have no sequence gaps or JavaScript errors.
Every trace exports completely with `dataLossOccurred=false`. Actual mapped
libcef paths, hardware driver mappings, geometry and stream integrity are saved.

## Repeated physical-display series after video was paused

The user reported concurrent YouTube playback during the exploratory work.
Those earlier observations are retained but do not establish an FPS ranking or
a clean batch-size comparison. The user then authorized a repeat after pausing
video. The table below is the new matched control / object profile / control
series on the normal desktop, not virtual KWin.

Three 30-second samples after five seconds of stream-ready warmup. Requested
4 M three-channel frames/s, WebSocket, batch 16384, clocked scrolling,
262144 visible frames, GTK reader. All have DPR 1.5 and physical canvas
1281 x 369. The one-pixel rounding difference from the virtual 1280 x 368
reference is retained, not relabeled as identical rendering work. Desktop
outputs were 60 Hz, with scales 1.35 and 1.5. No output setting was changed.

| Run | Object scopes | Actual Mframes/s | cppgc union total / longest, ms | RAF >25 ms / max, ms | Successful / failed Viz feedback | Confirmed presentation gaps >25 ms / max, ms | Host CPU median / max, % |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| control 1 | off | 3.9668 | 227.70 / 12.39 | 13 / 50.0 | 1768 / 19 | 30 / 50.00 | 7.25 / 51.72 |
| object profile | on | 3.9894 | 347.62 / 25.13 | 2 / 49.9 | 1784 / 11 | 14 / 50.00 | 7.20 / 12.54 |
| control 2 | off | 4.0001 | 236.20 / 11.39 | 0 / 16.8 | 1787 / 11 | 13 / 33.34 | 7.27 / 13.20 |

The cppgc union merges overlapping main-renderer cppgc scopes, including
synchronous sweep-on-allocation work. Nested pages/objects are not added
multiple times. These maxima differ from scheduled cppgc task maxima alone.
V8 GC is tracked separately. Presentation gaps are intervals between unique
successful Wayland presentation timestamps, not callback receipt times.
Raw and traced Viz timestamps agree exactly in these samples. Overlap counts
are temporal correlations, not proof of causation; earlier work can shift
later frame timing.

Controls have object tracing disabled, confirmed by zero finalizer-object
scopes. The profiling run is for attribution, not an SDK performance win.
Control 1 still includes a brief unrelated host-load pulse. Its largest native
GPU-to-BrowserMain delivery delay is 40.69 ms; control 2 and the object-profile
run have no such delays over 8 ms. Both latter runs have low recorded host load.

In control 2, RAF never exceeds 25 ms, but confirmed presentation intervals
include thirteen gaps near 33 ms and eleven failed feedbacks. Only one gap
overlaps any cppgc work, and that overlap is under 2 ms. Thus RAF alone would
miss the remaining presentation irregularity. Missing successful feedback by
itself cannot distinguish compositor discards from CEF-generated failures;
the extra diagnostic below makes that distinction for its own sample.

## Direct Wayland failure attribution

A separate ten-second physical-display diagnostic enables `wayland` tracing.
Its background-video state is not controlled and it is not used as a clean FPS
comparison. It records:

- 600 RAF callbacks, maximum RAF interval 16.8 ms;
- 599 submitted/feedback frames: 594 `presented`, five failures;
- all five failures matched by display trace ID and backend frame ID to
  `StoreFeedback(flags=16)` inside `HandleFeedback(discarded=true)`;
- all five handlers originate from the registered Wayland `OnDiscarded`
  listener, rather than the CEF fallback, timeout or out-of-order failure paths;
- six successful-presentation gaps near 33.3 ms, with no overlapping main-thread
  cppgc or V8 GC and no GPU-to-BrowserMain delivery delay over 8 ms;
- longest cppgc union 9.55 ms and host CPU median/max 7.43/11.36%.

This proves real compositor discard notifications in this sample. Such
notifications can legitimately arise when a surface update is superseded; they
do not prove that the compositor, CEF embedding, GPU fence timing, or frame
clock is individually defective. The next native test should correlate
commit/readiness/frame-callback timing, or compare the same workload outside
GTK embedding, before changing synchronization policy.

## Exploratory object/batch observations

The initial virtual-display samples use packet scrolling and broader trace
categories; they are not directly compared with the physical repeat.

| Renderer / batch | Actual Mframes/s | ArrayBuffer count / inclusive total, ms | Share of timed object scopes | Host CPU median / max, % |
| --- | ---: | ---: | ---: | ---: |
| WebGPU / 16384 | 3.7420 | 6302 / 287.41 | 73.2% | 27.17 / 94.46 |
| WebGPU / 65536 | 4.0000 | 1679 / 174.07 | 68.0% | 13.05 / 20.35 |
| WebGL / 16384 | 3.9598 | 7620 / 286.43 | 85.0% | 12.65 / 75.16 |

Larger batches reduce object turnover, but the individual GC pauses remain
around 20 ms and the samples have different background load. No default batch
change or throughput/FPS improvement is inferred. Batch size also changes
arrival latency and the maximum data held by four credits.

The largest exploratory presentation gaps, 250–267 ms, contain no measured
main-thread cppgc or V8 GC. The 267 ms interval overlaps a host-load sample of
94.5%; the app uses only 5.4% of all CPU capacity in that sample. This is evidence
of substantial external load, not proof of a particular scheduler/GPU cause.

## Next application-side experiment

The receive path creates a native ArrayBuffer for each WebSocket batch, copies
it into WASM, and then leaves its cleanup to the browser. Type counts and the
batch-size response make incoming payload buffers the primary candidate.
The finalizer trace does not contain allocation stacks proving every buffer's
origin. GPU rendering/compute finalizers are a much smaller part of this sample.

A focused next experiment is to transfer consumed payload buffers to a Worker
for cleanup after the WASM copy, measuring main-thread GC and total memory.
A Worker-owned receive path with a shared ring is a larger alternative. The
current page has `Worker` available but `crossOriginIsolated=false` and no
`SharedArrayBuffer`; shared memory would first need isolation headers and
backpressure preserving sequence integrity. Neither approach is implemented or
claimed to improve pacing by this report. These application-side experiments
need no new CEF feature, but require their own runtime validation.

## Reproduction and artifacts

All raw data, scripts, logs and source hashes are under
`/home/okhsunrog/tmp_zfs/codex_tmp/cef-finalizer-profile/`.
`clean-live-{1-control,2-objects,3-control}` names the video-paused series.
`diagnostic-wayland-feedback.json` records the five exact protocol matches.
The initial live observation was rejected by the old controller's strict
1280 x 368 geometry check; its actual 1281 x 369 trace is retained only as a
background-load observation, as recorded in `live-observation-acceptance.json`.

For the repeat, categories are
`toplevel,blink.user_timing,v8,disabled-by-default-v8.gc,cppgc,disabled-by-default-cppgc,viz`.
Add `wayland` for protocol failure attribution. Set
`WAVEBENCH_CPPGC_FINALIZERS=1` only for object attribution; controls set it to `0`.
From the artifact directory, the committed analysis tools can be run with:

```sh
uv run --no-project python /path/to/tauri-webgpu/diagnostics/analyze-finalizers.py clean-live-3-control
uv run --no-project python /path/to/tauri-webgpu/diagnostics/analyze-finalizers.py diagnostic-wayland
uv run --no-project python /path/to/tauri-webgpu/diagnostics/diagnose-feedback.py diagnostic-wayland
```

Native executable SHA256: `b5d24c7b3582504fecbb9bc318db404f575d6b860bb5bae5a5a2f0c8e5d94d14`.
Nix-patched diagnostic libcef: `a7c5e2b0d3e62bac7f544c9fe1f7af1f6cff419f48b11313e7a83b21e74eef1e`.
SDK archive SHA256: `9c825c2610f35423771c29fb943e1fe5e25df450ee071a8db388c330d19dd57e`.
API 999999 hash remains `a15420be190a255a6479f26806ed67fd59e1eeae`.
The VM is SHUTOFF; its source tree, Docker image, build cache and 500 GB disk
are retained. No VM or disk was deleted.
