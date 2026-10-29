# Worker presentation and latency experiments, 2026-10-22

These captures use hardware WebGPU with compute, 20,000 samples/s, batch 256,
Binary WebSocket, clocked playback and the render worker. The binary and CEF SDK
are identical throughout. Physical display: 60 Hz, DPR 1.35, approximately
1280 × 368 physical-pixel canvas. Do not pool this series with the preceding
DPR 1.5 results in OFFSCREEN_VALIDATION.md.

Each sample is approximately 30 seconds after warmup. The authorized session
indexer pause has an independent 90-second resume watchdog; resumption and
unlocked-screen checks passed for every capture. All traces reported no data
loss. Every capture passed wrapped-ring/spike compute readback with zero
sequence gaps and no application errors. No CEF rebuild or global system/display
changes were made. Full results, identities and raw trace paths are in
[offscreen-sync-2026-10-22.json](offscreen-sync-2026-10-22.json).

## Native presentation results

A gap means consecutive actual presentation timestamps more than 25 ms apart.
Discarded presentation feedback is excluded. Frame-count differences in the
100 ms main-thread blocking series partly reflect delayed probe-end delivery;
exact sample bounds are retained in JSON.

| Series / run | Source | FIFO | Pump ms | Double buffer | Main block ms/s | Gaps | Max gap ms | Swap→present p50 ms |
| --- | --- | --- | ---: | --- | ---: | ---: | ---: | ---: |
| sync / 1 | timer | off | 4 | False | 0 | 2 | 33.334 | 47.229 |
| sync / 2 | wayland | off | 4 | False | 0 | 2 | 33.332 | 20.094 |
| sync / 3 | wayland | on | 4 | False | 0 | 4 | 66.668 | 20.270 |
| sync / 4 | timer | on | 4 | False | 0 | 0 | 16.678 | 47.172 |
| sync-repeat / 1 | timer | on | 4 | False | 0 | 0 | 16.678 | 47.231 |
| sync-repeat / 2 | timer | off | 4 | False | 0 | 1 | 33.339 | 47.204 |
| sync-repeat / 3 | timer | off | 4 | False | 0 | 0 | 16.678 | 30.486 |
| sync-repeat / 4 | timer | on | 4 | False | 0 | 0 | 16.678 | 30.482 |
| sync-stress / 1 | timer | on | 4 | False | 100 | 0 | 16.678 | 47.082 |
| sync-stress / 2 | timer | off | 4 | False | 100 | 1 | 33.342 | 47.082 |
| sync-stress / 3 | timer | off | 4 | False | 100 | 0 | 16.678 | 47.067 |
| sync-stress / 4 | timer | on | 4 | False | 100 | 0 | 16.678 | 30.425 |
| pump-repeat / 1 | timer | on | 1 | False | 0 | 2 | 33.335 | 47.217 |
| pump-repeat / 2 | timer | on | 4 | False | 0 | 1 | 50.003 | 30.493 |
| pump-repeat / 3 | timer | on | 4 | False | 0 | 1 | 49.999 | 30.518 |
| pump-repeat / 4 | timer | on | 1 | False | 0 | 0 | 16.678 | 30.603 |
| double-repeat / 1 | timer | off | 4 | True | 0 | 0 | 16.678 | 30.557 |
| double-repeat / 2 | timer | off | 4 | False | 0 | 1 | 33.334 | 30.423 |
| double-repeat / 3 | timer | off | 4 | False | 0 | 0 | 16.677 | 47.198 |
| double-repeat / 4 | timer | off | 4 | True | 0 | 1 | 50.003 | 30.535 |
| source-repeat / 1 | wayland | off | 4 | False | 0 | 0 | 16.678 | 20.197 |
| source-repeat / 2 | timer | off | 4 | False | 0 | 3 | 33.342 | 46.532 |
| source-repeat / 3 | timer | off | 4 | False | 0 | 0 | 16.678 | 47.203 |
| source-repeat / 4 | wayland | off | 4 | False | 0 | 3 | 33.334 | 13.225 |

## Interpretation

- FIFO did not show a dependable pacing advantage. Initial quiet and stressed
  repeats looked favorable, but later FIFO-on captures still had 33–50 ms gaps.
  FIFO-off also achieved zero-gap captures. Native `Wavebench.FifoConstraint`
  events verify that the requested barriers actually executed.
- Reducing the CEF pump maximum from 4 ms to 1 ms did not consistently improve
  presentation. Keep the existing 4 ms preset.
- Double buffering produced approximately 30.5 ms median swap-to-presentation
  latency in both initial repeats, but ordinary buffering also achieved this in
  one control. One double-buffer capture still had a 50 ms presentation gap.
  Native `Swap throttled` events confirmed a limit of one pending swap with
  the flag, versus two observed in ordinary captures. Keep it experimental.
- Wayland frame scheduling reduced swap-to-presentation latency in the repeated
  comparison: medians 20.20 and 13.23 ms versus 46.53 and 47.20 ms for the timer.
  Native gaps were 0/3 versus 3/0, so this is evidence of lower presentation
  latency, not fewer stalls. Earlier Wayland screening was also about 20 ms.
  Native frame-callback events verify the selected source. The shorter 13 ms
  median reflects a different timing distribution, not a guaranteed fixed delay.
  This remains an optional experiment; the timer preset is unchanged.
- A render worker isolates the graph from main-thread JavaScript work; it cannot
  guarantee that the compositor presents every frame on time. These experiments
  do not justify making worker execution the default.

`analyze-swap-latency.py` matches backend buffer-swap sends to valid native
presentation feedback using exact 64-bit display trace IDs. Every valid feedback
in this series matched a send. This measures only backend swap-send to hardware
presentation. It is not total data acquisition / input-to-pixel latency, and
must not be added mechanically to percentile values from unrelated stages.

## Source-grounded queue experiment

Chromium 154.0.8037.98 supports `--double-buffer-compositing` in
[SkiaOutputDeviceBufferQueue](https://chromium.googlesource.com/chromium/src/+/refs/tags/154.0.8037.98/components/viz/service/display_embedder/skia_output_device_buffer_queue.cc).
It sets two buffers and one pending swap. The browser forwards it to the GPU
process in
[gpu_process_host.cc](https://chromium.googlesource.com/chromium/src/+/refs/tags/154.0.8037.98/content/browser/gpu/gpu_process_host.cc).
No native patch is necessary. For an explicitly experimental launch:

```sh
WAVEBENCH_CEF_VARIANT=pacing WAVEBENCH_ALLOW_CHROMIUM_ARGS=1 \
WAVEBENCH_QUERY='execution=worker&renderer=webgpu&motion=clocked' \
./run.sh --double-buffer-compositing
```

The `kMaxFramesInFlight=3` constant in WaylandFrameManager is part of emergency
feedback cleanup after 20 queued submissions. It is not the normal queue limit;
we did not modify it.

For the lower-latency Wayland-source experiment (without FIFO or double-buffer
flags):

```sh
WAVEBENCH_CEF_VARIANT=pacing WAVEBENCH_FRAME_SOURCE=wayland WAVEBENCH_QUERY='execution=worker&renderer=webgpu&motion=clocked' ./run.sh
```

## Functional and analysis checks

Both with FIFO enabled (timer source) and with the Wayland frame source
(FIFO disabled): minimize and restore, resize at DPR 1.35, four stream
restarts alternating 20k/200k, zoom, freeze/resume, and worker→main→worker
recreation all passed. Frame advancement after restore was checked independently
of cumulative FPS. The latency analyzer was checked with synthetic 64-bit IDs,
missing IDs, discarded feedback, sample bounds, duplicate IDs, negative latency
and an empty capture. No application or SDK rebuild was needed for this series.
