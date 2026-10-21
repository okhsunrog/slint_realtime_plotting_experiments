# Transport and cppgc comparison

Six 30-second samples after five seconds of stream-ready warmup, in the order
`ws / binary / protocol / protocol / binary / ws`. The source requests
1,000,000 three-channel frames/s, batch 16384, visible window 262144, hardware
WebGPU including compute, physical canvas 1280 x 368, DPR 1, virtual KWin
1440 x 1000. The fixed Tauri pump and the explicitly selected GTK reader are
identical in every run. No CEF rebuild or native preload is involved.

`delivery.frames_per_s` comes from received sample counters and monotonic
frontend time during the sampled frames. It is not the requested source rate
or rounded UI telemetry. All runs are within the predefined 5% delivery band.

| Run | Transport | Actual Mframes/s | Finalization total / max, ms | RAF >25 ms / max, ms | Host CPU median / max, % |
| --- | --- | ---: | ---: | ---: | ---: |
| 1 | ws | 1.0000 | 79.88 / 15.65 | 4 / 50.0 | 7.1 / 12.2 |
| 2 | binary | 0.9518 | 145.39 / 5.40 | 39 / 266.7 | 25.7 / 78.9 |
| 3 | protocol | 0.9999 | 73.89 / 9.42 | 3 / 33.3 | 10.4 / 17.8 |
| 4 | protocol | 0.9826 | 89.16 / 17.11 | 21 / 83.3 | 18.6 / 68.3 |
| 5 | binary | 1.0004 | 131.63 / 5.34 | 6 / 33.4 | 10.8 / 18.8 |
| 6 | ws | 0.9995 | 69.40 / 18.48 | 6 / 50.0 | 13.9 / 42.2 |

Finalization measures `CppGC.SweepFinalizeSweptPages` on the renderer thread
containing the application clock marker. Durations are clipped to the sample
window; nested GC phase names are not summed together. Host CPU figures cover
that same wall-clock window. RAF is scheduling timing, not physical scanout.

All six reports have visible canvases, no JavaScript errors, no sequence gaps,
a hardware adapter, and the 102-extrema compute PASS. Traces report no data
loss. Their JSON files are atomically renamed only after complete export.

## Interpretation

- Switching transport does not eliminate cppgc work. Binary channel gives
  shorter individual finalization slices in these samples, but approximately
  twice the total finalization time of WebSocket. Protocol also retains long
  finalization slices.
- The first binary and second protocol samples overlap substantial unrelated
  host CPU load. They are retained and explicitly marked by the table; their
  RAF outliers do not establish a causal transport ranking.
- A separate five-second preflight requested 4 Mframes/s and observed about
  3.81 M with WS, 2.34 M with binary, and 2.73 M with protocol. These are short
  observations, not established throughput ceilings or a balanced benchmark.
  They motivated the 1 Mframes/s comparison rather than treating slower delivery
  as a frame-pacing improvement.
- The default transport is unchanged. A source-level cppgc deadline experiment
  is the next step; these results do not identify every finalized object type
  or explain every native swap delay.

## Reproduction and artifacts

For each mode use the same executable, SDK and canvas, repeat in reverse order:

```sh
WAVEBENCH_WAYLAND_READER=gtk \
WAVEBENCH_QUERY='renderer=webgpu&transport=ws&motion=packet&measure=transport-ws&rate=1000000&batch=16384&seconds=30' \
WAVEBENCH_REPORT_PATH="$PWD/logs/transport-ws.json" ./run.sh
```

Use `transport=binary` or `transport=protocol` for the other modes. For Chromium
tracing, explicitly set `WAVEBENCH_DEBUG_PORT=9227`. The normal default leaves
the DevTools protocol server disabled. Trace categories used here are
`toplevel,blink.user_timing,v8,disabled-by-default-v8.gc,cppgc,disabled-by-default-cppgc,renderer.scheduler,devtools.timeline`.
Trace export and analysis occur after sampling and before the next run.

Raw reports, traces, host/driver checks, controllers and the analysis are in
`/home/okhsunrog/tmp_zfs/codex_tmp/cef-transport-gc/`.
`integrity.json` records acceptance checks and `binaries.json` records immutable
executable, SDK library, WASM and measurement-script SHA256 hashes. The CEF
library remains `d838a08dc714d2893ac41b1c9f91fa44a926ccd0e6f503c253e652b8186d3e26`.
The measurement feature adds no dependencies or normal-launch instrumentation.
