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

Final physical-screen A/B tests must include both frame sources and, separately,
FIFO. A locked or occluded physical desktop must not be ranked as a renderer
failure. The retained cloud disk contains the Chromium sources and incremental
build cache; neither the VM nor the disk is deleted by these experiments.

The final exact binary also completed a quiet 30 s control at about 3.999 million
samples/s with 1800 RAF callbacks and no RAF gaps. The trace document origin
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
