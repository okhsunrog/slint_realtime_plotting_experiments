# Linux CEF message-pump diagnosis

The Linux external pump could discard an immediate scheduling request when a
single pipe read returned two `i64` messages. `handle_check()` called
`on_schedule_work(delay_ms[0])` only for an 8-byte read; a 16-byte read delivered
only the second request. In particular, an immediate request followed by the
pump's internal `i32::MAX` fallback placeholder lost its immediate wakeup.
The host then waited for its 33 ms fallback timer.

The fix processes the first complete request for both 8- and 16-byte reads,
then processes the second request in arrival order. It retains the existing
priority, fallback interval, reader selection, and CEF SDK. This behavior was
copied from the CEF external-pump sample, which has the same equality checks.

## Evidence and method

Diagnostic runs use the existing embedded frontend, binary WebSocket stream,
4 million three-channel frames/s, batch 16384, visible window 262144, a physical
1280 x 368 canvas, DPR 1, and a separate 1440 x 1000 KWin Wayland session. The
GTK-owned reader is selected explicitly to exclude the previously identified
shared-reader barrier. Hardware WebGPU and the compute check remain enabled.

A temporary diagnostic executable enables DevTools on localhost. Chromium
tracing records the browser/GPU pipeline and renderer tasks. An LD_PRELOAD
probe records pipe reads/writes, waits and `cef_do_message_loop_work()` into a
bounded memory buffer, writing it only on process exit. A performance mark
provides the renderer-to-native clock offset. Timing comprises 5 s warmup and
30 s sampling; trace serialization and file output happen afterwards.

Before the fix, `trace6-memory-4m` captured a 16-byte `[0, 2147483647]` read at
87027728474.259 us. There was no CEF work call until 87027762143.280 us, a
33.669 ms delay. The GPU frame waited 33.968 ms for browser-side buffer swap.
The UI spent this interval in `ppoll()` with a 32 ms timeout. Pipe writes took
microseconds, and its observed queue held at most one prior message in this
sequence. This is a scheduling loss, rather than a full-pipe diagnosis.

An earlier independent trace (`trace3-gtk-4m`) captured a 34.241 ms GPU-to-browser
swap delay coinciding with a 33.118 ms poll timeout, without a long renderer
callback in that interval.

## Fix validation

Tauri commit `eed19f2a394c868761c7e78a1c0b2b010dcfbde0` changes only the
first-request condition from equality to a minimum complete-message size.
Both complete requests are processed in their original order.

The matching `trace7-fixed-4m` run captured four natural 16-byte
`[0, 2147483647]` reads during its 30-second sample. Each was followed by a
CEF work call after 1.303, 1.473, 2.211, and 2.130 microseconds. Before the fix,
the same captured pattern waited 33.669 ms.

A separate end-to-end regression injects exactly one two-message pair into
the real pipe after startup. The old executable waited 1.199 ms for a later
pump opportunity; the fixed executable called CEF after 1.248 microseconds.
The old delay depends on the remaining fallback deadline. These short injected
runs are correctness checks and are excluded from performance comparisons.

The fixed run still has renderer GC pauses and other delayed buffer swaps;
its raw RAF maximum was 200 ms. This fix does not establish elimination of all
microstutters or an overall FPS improvement. Both builds use the same SDK,
frontend, renderer, stream settings, and explicit GTK reader selection.

## Remaining renderer pauses

The full traces also locate CPU work in the application's renderer:
`cppgc::internal::SweeperImpl::IncrementalSweepTask`. In the trace with the
memory-only probe, `CppGC.SweepFinalizeSweptPages` takes up to 44.465 ms.
`CppGC.SweepInLowPriorityTask` advertises a 5 ms budget. Chromium's
`v8/src/heap/cppgc-internal/sweeper.cc` checks that budget between batches of
pages, while finalizers within each page run without an internal deadline.
This is a separate measured source of renderer stalls. Which object lifetimes
produce the expensive finalization remains under investigation.

These traces are diagnostic observations, not a controlled proof of an overall
FPS improvement or physical scanout smoothness. Other host workloads are not
controlled, and Chromium tracing adds overhead. RAF timestamps can lag callback
execution; correlation considers both clocks rather than assigning every RAF
timestamp jump to the following callback interval.

## Artifact integrity

Raw traces, native events, host load, per-run reports and scripts are kept under
`/home/okhsunrog/tmp_zfs/codex_tmp/cef-residual-jank/`.

- `trace1`: timing report retained; no complete trace was exported.
- `trace2`: truncated export; not treated as a complete trace. Its recovered
  clock marker is used only for an explicitly partial native correlation.
- `trace3`: complete trace and positive-return poll timing; older wait-only probe.
- `trace4`: discarded because the synchronous probe failed to preserve errno.
- `trace5`: complete, errno-corrected, but synchronous file output can delay the
  application; the 365 ms write/log interval is not claimed as an application bug.
- `trace6`: complete trace with memory-only native recording and preserved errno.
  The probe separately passes a signal-interrupted `poll()` regression check.

CEF binary/source artifacts and the VM build cache are unaffected by this fix.

- `trace7`: complete fixed-runtime trace, memory-only probe, no injected requests.
- `regression-diagnostic` / `regression-fixed`: separate one-pair injection checks.

## WebGL control and production smoke

`trace8-fixed-webgl-4m` uses the same fixed runtime, stream settings, reader mode
and tracing categories with WebGL2 instead of WebGPU. The reported renderer is
ANGLE on Mesa Intel Arc, no JS errors or sequence gaps. Actual throughput was
3.82 Mframes/s, compared with 3.84 Mframes/s in the fixed WebGPU trace.
`CppGC.SweepFinalizeSweptPages` still reached 44.690 ms (45.031 ms for its
containing sweeper task). Expensive finalization therefore also occurs without
WebGPU or compute shaders. The shared receive path/object lifetime is a next
candidate; this control does not identify the specific finalized object type.

The normal release executable was rebuilt with the pinned fixed Tauri commit
and the existing CEF SDK, without the diagnostic DevTools configuration. A
separate 5-second smoke after warmup uses the default threaded reader and
clocked scrolling: hardware WebGPU, 102-extrema compute PASS, no JS errors,
no sequence gaps, and clean window shutdown. Port 9227 was verified closed
while that executable was running. No preload is used by the normal launcher.

The 30-second diagnostic captures are not a balanced performance comparison;
host workloads varied. Residual native swap delays and GC stalls remain.
