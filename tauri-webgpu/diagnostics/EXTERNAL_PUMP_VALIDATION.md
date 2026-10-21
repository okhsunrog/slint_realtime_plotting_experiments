# External pump and presentation experiments, 2026-10-21

These measurements use an isolated KWin Wayland session on the local Intel GPU.
Hardware WebGPU and the compute correctness check pass. The physical desktop
was locked during the new candidate tests; physical presentation and discards
have not been accepted. RAF callbacks and native Wayland presentation timestamps
are measured independently.

The Tauri runtime option is in
[`fix/cef-message-pump-interval`](https://github.com/okhsunrog/tauri/tree/fix/cef-message-pump-interval).
Its public `Cef::message_pump_max_delay(Duration)` clamps to 1–33 whole ms;
the library default remains 33 ms. The boundary regression passed after running
the test binary with the Nix runtime library closure.

The exact native source patches, source hashes and SDK hashes are retained in
[`experiment/external-pump-yield`](https://github.com/okhsunrog/cef/tree/experiment/external-pump-yield/tools/wayland-build/experiments/external-pump).
The separate
[FIFO experiment](https://github.com/okhsunrog/cef/tree/experiment/wayland-fifo/tools/wayland-build/experiments/fifo)
remains opt-in: virtual tests had zero discards with either setting, so they do
not establish whether it fixes the earlier physical compositor discards.

## Confirmed mechanisms

At the 33 ms fallback, a scoped scheduler/syscall trace captured UI `ppoll`
waits of 29.067 and 27.063 ms inside native presentation gaps. The UI thread's
maximum runnable wait in that recorded interval was 0.596 ms. This supports
host-loop sleeping rather than prolonged UI-thread CPU starvation for those
specific gaps. At 4 ms, the longest recorded `ppoll` was 4.429 ms; some native
gaps nevertheless remained.

`MessagePumpExternal::Run()` obtains a finite deadline directly from Chromium's
`Delegate::DoWork()`. The finite-deadline patch forwards it when returning to
the host instead of relying exclusively on `ScheduleDelayedWork()` callbacks.
With one immutable app binary, Worker reclamation, GTK reader, Wayland frame
source and a 33 ms host bound, an old/new/new/old sequence produced 5/1/1/7
native gaps over 25 ms in four 30 s samples. CPU maxima stayed around 12–14%.
This is a measured partial improvement, not elimination of stutter.

The independent yield switch returns to the host once no immediate work remains
instead of spinning until the 10 ms time slice ends. Same-SDK off/on/on/off
samples used 1.025/0.951/0.964/1.021 CPU cores, with 2/3/0/1 native gaps.
Thus this pair shows a CPU benefit, not a clear presentation benefit. Matched
scoped scheduler traces measured maximum uninterrupted UI running slices of
10.273 ms off and 1.518 ms on, with runnable waits below 0.2 ms.

A sweep of 1/2/4/8 ms polling bounds did not show consistent superiority of
1 ms. The 1 ms samples used about 1.15 cores versus 1.04–1.06 at 4–8 ms.
Quiet 4 ms samples could present all 1800 frames in 30 s; individual misses
still occurred in repeats. No global timer-slack, scheduler, filesystem or
desktop tuning was applied.

## Buffer ownership and load

In a quiet combined sample (yield on, 4 ms bound, Wayland source), the Worker
path presented all 1800 frames in 30 s at 3.998 million samples/s. A quiet main
reclamation sample had four native gaps and a longest cppgc interval of
13.395 ms. The Worker path still does not guarantee a smooth frame under load.

Other combined runs were contaminated by host CPU bursts up to 40–59%. One
Worker run had a 103.6 ms callback and a 100 ms native gap; its main-thread
cppgc intervals stayed below 2.4 ms. This event remains under investigation;
it must not be reported as a demonstrated GC or GPU-driver defect. A loaded
main-reclamation run with 19 RAF misses is not a fair ranking against a quiet
Worker run. Throughput and sequence checks are reported separately from pacing.

The 8 million samples/s WebGPU stress run sustained about 7.91 million/s with
no sequence gaps; it is a stress result, not a claim of perfect 8 million/s
sustained delivery. WebGL is tested separately with compute disabled.

## Native CEF control

A standalone `cefsimple` binary using CEF's native message loop loaded a local
Canvas2D sine-wave page. It had no Tauri, WASM, backend stream or WebGPU compute.
The same SDK and compositor were used, with only the frame-source feature
changed. Each sample lasted 30 s after warmup.

| Source sequence | RAF intervals over 25 ms | Native intervals over 25 ms | Host CPU median / maximum |
| --- | --- | --- | --- |
| Wayland | 34 | 34 | 3.8% / 9.5% |
| Timer | 1 | 0 | 3.7% / 7.9% |
| Timer | 1 | 0 | 8.3% / 47.5% |
| Wayland | 3 | 3 | 3.8% / 8.0% |

Native trace events confirm the Wayland source was active only in its selected
runs. Both timer runs presented all 1800 frames. The large first Wayland result
did not repeat at the same magnitude. This isolates some presentation variation
outside our Rust frontend and transport, but does not establish a physical
KWin/driver defect or justify disabling synchronization.

## Acceptance boundary

The diagnostic yield SDK passed resize, 4/8/4 million samples/s restarts,
minimize/restore, 125%/150%/100% fractional scale with exact canvas dimensions,
compute correctness, sequence integrity and graceful close. The ordinary
project build passed the same lifecycle and fractional-DPI checks through its
actual Nix preset launcher, without externally supplied Chromium CLI switches.
A further 35 s diagnostic run sustained all 2100 RAF callbacks and native
presentations without a gap; compute and sequence checks passed. A complete
1,039,530-event scoped kernel trace accompanied that run. Its longest observed
WebGPU call was a 1.7 ms queue write. The earlier broader kernel capture lost
910 events and is excluded from scheduler conclusions. The rare 103.6 ms event
did not recur and is not considered localized.

The telemetry now labels FPS as a session average: minimizing a window includes
that idle interval in the average even when rendering has resumed at 60 fps.
The per-run frame report also includes its document time origin for aligning
performance timestamps with the native trace.

The physical-screen follow-up below includes both frame sources and, separately,
FIFO. A locked or occluded physical desktop must not be ranked as a renderer
failure. The retained cloud disk contains the Chromium sources and incremental
build cache; neither the VM nor the disk is deleted by these experiments.

The final exact binary also completed a quiet 30 s control at about 3.999 million
samples/s with 1800 RAF callbacks and native presentations, no gaps in either, and a
16.667 ms maximum native presentation interval. The trace document origin
matched the report at both ends. A parallel read-only process I/O sampler saw
maximum host CPU 13.3%, without large disk writes. Perfetto's shared-memory
implementation uses memfd, and CDP creates the output stream file when stopping
the trace; a theory that tracing always causes the large ZFS bursts was not
confirmed. Idle sampling without the demo stayed below 6% host CPU. The original
bursts' writing process remains unidentified.

The final ordinary entry-point run also passed startup, SDK-copy verification
and stream correctness, but had host CPU up to 73.6%; its remaining timing gaps
are retained as loaded observations rather than quiet acceptance results.
WebGPU system information reports `webgpu_on_vk_via_gl_interop: enabled` on the
Intel adapter. The compositor reports OpenGL; its separate Vulkan status must
not be substituted for the WebGPU interop status.

Compatibility checks also rebuilt the WebKitGTK target after the common Tauri
pin update. A short WebKitGTK 2.54.1 / WebGL2 smoke run delivered approximately
999,345 samples/s at a requested 1 million/s, with no sequence gaps or frontend
errors. Compute was explicitly disabled in that control; it is not a WebGPU
result or a timing comparison against CEF.

## Physical screen follow-up, 2026-10-22

[Machine-readable results](physical-validation-2026-10-22.json) retain all 18
physical captures, including runs affected by background CPU activity. The
screen was checked unlocked before and after every capture. The app was on the
built-in 2256×1504 / 60 Hz panel at 135% scale, with a second 4K / 60 Hz display
at 150% still connected. KWin was 6.8.0. WebGPU used the hardware Intel adapter;
compute correctness and stream sequence checks passed in the app captures.

The detailed trace series retained occasional native presentation failures even
when RAF was uniform. Six relatively quiet detailed captures had 2, 5, 3, 2,
3 and 7 failed presentation feedbacks per 30 seconds. Two further runs had CPU
spikes near 80% and are not suitable for ranking configurations. Native Wayland
events confirmed both that FIFO constraints were applied and that failures
included actual compositor `discarded` callbacks. FIFO is still optional:
these captures do not establish an advantage from enabling it.

Detailed traces were about 350 MB. Reducing the categories to
`viz,wayland,blink.user_timing` produced about 43 MB and retained native
presentation feedback. Each following capture waited for eight consecutive
one-second host CPU samples below 15%. These reduced traces do not contain GC
events and cannot exclude GC as a cause of a delay.

| Frame source | FIFO | RAF callbacks | Successful presentations | Native maximum interval |
| --- | --- | ---: | ---: | ---: |
| Timer | Off | 1800 | 1800 | 16.678 ms |
| Timer | On | 1800 | 1800 | 16.678 ms |
| Wayland | On | 1800 | 1800 | 16.678 ms |
| Wayland | Off | 1800 | 1800 | 16.678 ms |

All four runs delivered approximately 3.997–3.999 million samples/s without
sequence gaps. Host CPU median was 6.8–6.9%, maximum 8.0–12.3%; app CPU averaged
0.765–0.780 logical cores. This demonstrates smooth physical presentation with
the candidate under those conditions, including fractional scaling and compute.

A subsequent baseline/candidate/candidate/baseline sequence used the same
reduced trace categories. The baseline used the earlier profiling SDK, main-thread
buffer reclamation and the 33 ms pump bound; the candidate used Worker reclamation,
the combined native patch and the 4 ms bound. Both used timer pacing, FIFO off
and the 4 million samples/s stream. One quiet candidate run and one quiet baseline
run each produced 1800 successful presentations without gaps. Their app CPU was
0.764 and 0.699 logical cores respectively. The other two captures had background
CPU spikes of 44.7% and 65.3%. Consequently this comparison does **not** establish
a general microstutter fix or a CPU saving from the combined preset. The previous
scoped evidence about pump deadlines and main-thread finalization remains useful,
but must not be substituted for that missing overall result.

Standalone CEF with its own native event loop and a Canvas2D page, without Tauri,
WASM or a data stream, also produced 1800 successful presentations without gaps
in its quiet Wayland-source capture. Its timer-source capture had a CPU spike of
66.6%, so it cannot establish a difference between the frame sources.

A separate read-only process I/O sample observed the session indexer
`agentsview-linu` writing up to 20–29 MB/s to its SQLite database while ZFS worker
CPU activity was high. Trace export and analysis also generated disk traffic after
measurement. This identifies competing work, not a proven causal attribution for
every stall. The controlled source-clock follow-up below measures the indexer's
contribution. These initial captures did not change compositor, network,
filesystem or scheduler settings.

The preset remains experimental and the stable SDK stays the default. Quiet
physical presentation is verified; a universal fix for pauses under background
load, including the earlier rare 103.6 ms callback, is not established.

### Movement and source-clock follow-up

Presentation cadence alone missed another defect. Reanalysis of the recorded
sample positions found one stationary-head interval in the quiet Wayland/FIFO
capture despite all 1800 native presentations succeeding. A subsequent 60-second
run without DevTools or Chromium tracing had 3600 regular RAF callbacks but seven
stationary-head intervals. Buffer depletion and recovery can freeze the plotted
position while frames continue to be drawn.

[Source clock validation](SOURCE_CLOCK_VALIDATION.md) documents the application
fix, its regression test, the additional flow measurements and the temporary
paused-indexer controls. Both indexer pauses were followed by a verified resume.
The final corrected-clock capture also verified 1800 native
presentations and continuous plotted-head movement without gaps.
