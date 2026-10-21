# Per-page cppgc deadline experiment

The candidate reduced the longest observed renderer cppgc foreground task from
42–44 ms to 16–19 ms in an A/B/B/A series. This does **not** establish an overall
frame-pacing improvement: the final baseline had fewer RAF outliers than either
candidate run. The normal SDK remains unchanged.

## Change and build

V8's `DeadlineChecker` in `src/heap/cppgc-internal/sweeper.cc` originally checks
the deadline every fourth page. Each space creates a fresh checker, so short
spaces can finish without a check. The candidate checks after every page.
The nominal incremental foreground sweep budget is 5 ms. A page's finalizers
remain indivisible, so this change cannot enforce a hard 5 ms limit.

The source patch and complete artifact provenance are committed at
https://github.com/okhsunrog/cef/tree/experiment/cppgc-page-deadline/tools/wayland-build/experiments/cppgc-page-deadline.
It is an optional overlay, not registered in the default CEF patch recipe.
The retained source/build cache produced the candidate with `-j8` on
16 vCPUs / 32 GiB RAM in 3m54s including packaging. No clean build was needed.
The VM is now SHUTOFF; its 500 GB disk, source tree, Docker image and cache remain.

## Method and results

Four 30-second samples after five seconds of stream-ready warmup, requesting
4 M three-channel frames/s over WebSocket. Same frozen native executable,
WASM, measurement script, batch 16384, 262144 visible frames, packet motion,
1280 x 368 canvas, DPR 1 and virtual KWin 1440 x 1000. Every run explicitly
selects the GTK Wayland reader. Each SDK supplies its own matching snapshots
and resources; `/proc` maps verify the actual loaded library. No native preload.
Trace export and JSON analysis take place between measurements.

| Order | SDK | Actual Mframes/s | Finalization total / max, ms | Foreground GC task max wall / CPU, ms | RAF >25 ms / max, ms | Host CPU median / max, % |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| 1 | baseline | 3.778 | 276.16 / 42.42 | 43.61 / 44.76 | 34 / 333.3 | 26.7 / 81.7 |
| 2 | candidate | 3.989 | 273.26 / 18.54 | 18.97 / 18.46 | 7 / 116.8 | 29.7 / 41.2 |
| 3 | candidate | 3.910 | 237.92 / 15.78 | 16.12 / 15.93 | 25 / 83.4 | 24.7 / 70.6 |
| 4 | baseline | 3.986 | 231.06 / 41.75 | 42.72 / 42.28 | 4 / 33.4 | 19.3 / 29.0 |

Finalization means renderer `CppGC.SweepFinalizeSweptPages`, clipped to the
actual measurement window. Nested GC phases are not added together. Foreground
GC task maxima are `ThreadControllerImpl::RunTask` events sourced from cppgc;
wall and CPU maxima can belong to different tasks. Host CPU covers the same
wall-clock window. Actual delivery comes from received counters, not the source
rate or rounded footer. Run 1 falls outside the intended ±5% delivery band and
has substantial background load; retain it as an observation, not a matched
throughput/FPS comparison. Runs 2–4 are within that band.

All four have no JavaScript errors or sequence gaps, hardware Intel Vulkan
WebGPU, and the 102-extrema compute PASS. All trace exports report no data loss.

## Interpretation

- The repeatable GC tail reduction is the useful result. Similar total
  finalization time means the work is distributed into shorter slices rather
  than removed. Both wall and thread-CPU maxima decrease.
- Pauses of 16–19 ms remain; the nominal 5 ms budget is still exceeded. This
  experiment does not instrument individual pages or identify finalized types,
  so it cannot attribute each remaining pause to a specific page/object.
- Whole-app smoothness is unresolved. Different host load and asynchronous RAF
  timestamps prevent a causal FPS claim; RAF is not physical presentation.
  The baseline's final run is a concrete counterexample to claiming a universal
  RAF improvement from this patch. Native swap delays remain a separate target.
- Keep WebSocket as the default. The preceding transport comparison is in
  [TRANSPORT_VALIDATION.md](TRANSPORT_VALIDATION.md). Before deeper sweeper
  changes, profile allocation/finalizer types and correlate native presentation
  delays on a quiet host. Reducing object churn may avoid GC work altogether.

## Local artifacts and manual launch

Everything is retained under
`/home/okhsunrog/tmp_zfs/codex_tmp/cef-transport-gc/`:
`cppgc-series.log`, four `cppgc-4m-*` reports/traces/host checks,
`cppgc-summary.json`, `cppgc-integrity.json`, and `cppgc-candidate/` containing
patch, build log, archive, Nix expression and SDK. The candidate's API hash is
unchanged, so the benchmark executable was not rebuilt between SDKs.

Native binary SHA256: `b5d24c7b3582504fecbb9bc318db404f575d6b860bb5bae5a5a2f0c8e5d94d14`.
Baseline Nix libcef: `d838a08dc714d2893ac41b1c9f91fa44a926ccd0e6f503c253e652b8186d3e26`.
Candidate Nix libcef: `726c04d8cafab04b98ec966ce3231542ad3ce7e8f8c4702a52e67a7eb7793262`.
Candidate archive SHA256: `85d2b0f7d5d8dbe0806da8ceb37f0fad88b7a9787131f10b08c24b4529dcca90`.

To inspect the candidate interactively on this laptop:

```sh
/home/okhsunrog/tmp_zfs/codex_tmp/cef-transport-gc/cppgc-candidate/run-demo.sh
```

This uses the frozen benchmark binary, matching candidate resources, and GTK
reader by default. Log: `cppgc-candidate/demo.log`. To test the normal threaded
reader, prefix `WAVEBENCH_WAYLAND_READER=threaded`; that reader/SDK combination
was not measured in this series. Ordinary `./run.sh` continues to use the stable SDK.
