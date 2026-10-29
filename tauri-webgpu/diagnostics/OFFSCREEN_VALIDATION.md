# OffscreenCanvas / render-worker experiment

The opt-in `execution=worker` mode moves WebSocket reception, Rust/WASM packet
validation, the playback clock, WebGPU uploads, compute reduction and drawing
into a dedicated module Worker. Leptos and native Tauri commands stay on the
main thread. Both modes use the same production `Gpu`, WGSL shaders and
`PlaybackClock`. The worker acknowledges each packet after consumption; the
backend's four-packet credit limit is unchanged. It does not need the separate
buffer-reclamation worker. No CEF rebuild was needed.

Select **Execution → Render worker (experimental)**, or launch:

```sh
WAVEBENCH_CEF_VARIANT=pacing \
WAVEBENCH_QUERY='execution=worker&renderer=webgpu&motion=clocked' ./run.sh
```

The default remains **Main thread**, at 20,000 samples/s with 256-sample batches.
The experimental mode supports Binary WebSocket, WebGPU/compute and clocked
scrolling. Other transport comparisons and WebGL2 remain available in the main
mode. Execution changes reload the document to recreate the transferred canvas.
Resize, zoom, freeze/resume and frozen-history dragging are forwarded to the
worker; resizing uses physical pixel dimensions including DPR.

## Measurements on 2026-10-22

[Full results and binary identity](offscreen-validation-2026-10-22.json) include
all captures, source-clock/backend statistics, functional checks and the capture
that encountered substantial background work. Raw traces remain under
`/home/okhsunrog/tmp_zfs/codex_tmp/cef-frame-pacing/`.

All cases below used 20k/s, batch 256, hardware Intel Xe-LPG WebGPU, compute,
the same rebuilt binary and pacing CEF SDK, a 60 Hz physical desktop at DPR 1.5,
and the same approximately 1280 × 368 physical-pixel canvas. Thirty-second native
controls temporarily paused the session indexer under the authorized independent
90-second resume watchdog, and verified its resumption afterwards. CPU maximum
was 7.2–8.7%. All traces reported no data loss. No system tuning was changed.

The stressor deliberately occupies the page's main JavaScript thread once per
second. It is enabled only by `block_ms=40` or `block_ms=100` together with the
opt-in measurement query. It models UI work, not global CPU or GPU saturation.

| Main-thread block each second | Execution | Native presentations | Native intervals >25 ms | Maximum native interval | Actual callback intervals >25 ms |
| --- | --- | ---: | ---: | ---: | ---: |
| none | Main | 1795 | 5 | 33.339 ms | 0 |
| none | Worker | 1792 | 7 | 50.006 ms | 0 |
| 40 ms | Main | 1786 | 15 | 50.007 ms | 29 |
| 40 ms | Worker | 1787 | 13 | 50.007 ms | 0 |
| 100 ms | Main | 1659 | 42 | 100.007 ms | 30 |
| 100 ms | Worker | 1797 | 11 | 33.339 ms | 0 |

The timed samples are approximately 30 seconds; main-thread blocking and probe
message delivery can extend them slightly. Exact durations are retained in JSON.
RAF timestamps alone understated main-thread blocking: actual callback start
times showed pauses up to 119.6 ms in the 100 ms main case, versus 23.2 ms in the
worker. Native presentation feedback provides the evidence for visible timing.

At 100 ms, the steady plotted head stalled eight times in main mode and zero in
worker mode. Packet delivery p95 was 4.31 versus 1.29 ms. All cases had zero
sequence gaps and passed the actual wrapped-ring/spike compute readback.
Steady motion excludes initial history filling: the 262,144-sample view needs
13.1 seconds at 20k/s, so motion is ranked from stream age 20 seconds.

Two preceding untraced minute captures do not establish an equal-load performance
ranking: main had CPU maximum 11.6%, zero steady RAF gaps and 44.7 ms median
buffer; worker encountered CPU maximum 70.2%, 15 steady RAF gaps and 39.6 ms median
buffer. Neither had steady head stalls. This also shows that a worker cannot
prevent delays caused by competing host workload.

The controlled experiment supports better isolation from substantial main-thread
JavaScript work. It does **not** establish lower CPU consumption or improved
quiet-state frame pacing. The quiet worker capture still had native frame gaps,
and the 40 ms stress case showed little presentation improvement. Keep the mode
experimental and selectable; do not treat smooth worker callbacks as proof of
smooth presentation or make it the default based on this evidence alone.

## Validation

- Release WASM and CEF application build; WASM target check and formatting.
- Hardware compute readback and stream integrity in every capture.
- Real CEF functional checks: four stream restarts alternating 20k/200k,
  zoom, freeze/resume, canvas resize at DPR 1.5, and main/worker/main-document
  recreation. Restart testing found and fixed a race: the old worker socket must
  be stopped before the backend replaces its session.
- Existing real buffer-reclaimer Worker test and five motion-analysis tests pass.
- Worker frame/motion probes collect locally, transfer after sampling, and align
  worker/main `performance.timeOrigin`; individual frames are not posted to UI.
