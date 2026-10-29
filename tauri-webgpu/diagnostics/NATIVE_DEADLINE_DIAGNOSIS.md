# Native presentation deadline diagnosis, 2026-10-22

Follow-up: the 2000 µs override is now active and has undergone
[initial validation on 2026-10-29](KWIN_MARGIN_VALIDATION.md). The text below
records the original baseline diagnosis and its then-untested hypothesis.

Five captured 33 ms presentation gaps share a concrete failure: the i915 display
update runs into or past its vblank exclusion window. The buffer eventually
shown had already been sent by Chromium before the preceding presentation.
This localizes this class of stalls to the KWin/DRM commit deadline, rather than
missing stream data or a late render-worker callback for that buffer.

The proposed mitigation is **not yet tested**: increase KWin's DRM safety margin
from 1000 to 2000 microseconds. No compositor setting was changed for these
captures, and no CEF rebuild was performed. This does not explain every earlier
stall: a separate 50 ms capture contained missing Chromium frame submissions.

## Capture conditions and controls

Same immutable binary and SDK as PRESENTATION_QUEUE_VALIDATION.md; WebGPU with
compute, render worker, Wayland frame source, FIFO off, pump 4 ms, 20k samples/s,
batch 256, physical display at 60 Hz / DPR 1.35. The authorized session-indexer
pause used its independent resume watchdog and restored the process after each
capture. No network, display-mode, scheduler or power-policy settings changed.

Chromium and a private kernel trace instance use the monotonic clock. The kernel
trace includes DRM vblank timestamps, i915 pipe updates and requests, DMA-fence
signals, scheduler events and selected syscalls. The final capture also follows
KWin's `eDP-1` commit thread, TID 3770. Vblank hardware timestamps agree with
Chromium's presentation feedback. Event delivery timestamps precede those
reported hardware timestamps by about 0.45 ms; these two fields must not be
silently treated as identical.

| Capture suffix | Native presentations | Gaps >25 ms | Maximum interval |
| --- | ---: | ---: | ---: |
| pipeline-4 | 1801 | 0 | 16.678 ms |
| pipeline-5 | 1798 | 3 | 33.334 ms |
| pipeline-6 | 1799 | 2 | 33.338 ms |

All three captures have zero kernel overruns/dropped events and no Chromium
trace data loss. All five analyzed gap windows are entirely within the kernel
capture. Compute readback and sequence integrity checks passed. The zero-gap
capture is retained as a control, not omitted from the result.

## One matched timeline

The first gap in `physical-indexer-paused-pipeline-6` starts at monotonic
142370103280 microseconds. Times below are relative to that last presentation.

| Event | Relative time |
| --- | ---: |
| Chromium sends the buffer subsequently displayed at +33.333 ms | -5.831 ms |
| KWin wakes its eDP-1 commit thread | +7.695 ms |
| Commit thread deliberately enters `clock_nanosleep` | +7.704 ms |
| Timer wakes the commit thread | +15.091 ms |
| Thread gets CPU | +15.140 ms |
| Zero-timeout fence poll reports ready | +15.256 ms |
| Atomic DRM ioctl begins | +15.297 ms |
| Atomic DRM ioctl returns successfully | +15.933 ms |
| i915 pipe update starts, scanline 1501 (exclusion range 1495–1504) | +16.149 ms |
| i915 pipe update ends, frame counter advances, scanline 1515 | +16.307 ms |
| New graph buffer finally presented | +33.333 ms |

The second gap repeats the sequence: wake +15.090 ms, CPU +15.149 ms,
ioctl +15.359 to +15.952 ms, pipe update +16.166 to +16.314 ms. CPU scheduling
latency after the timer wake was only about 0.05–0.06 ms. The intentional wait,
ioctl work and subsequent kernel work leave insufficient margin in these cases.
The delayed buffer was sent at -4.240 ms, so optimizing its WASM compute cannot
recover the missed display deadline.

In pipeline-5's middle gap, the KWin GPU context 99 fences were already signaled
at +12.081/+12.090 ms, yet the pipe update began at +16.286 ms. Fence signaling
observed by a polling thread is not necessarily the GPU's actual completion
instant; the claim here is only that these fences were ready by the recorded time.
Request retirement timestamps are not used as GPU execution durations.

## Source-grounded mitigation candidate

KWin 6.8.0's
[DrmCommitThread implementation](https://invent.kde.org/plasma/kwin/-/blob/v6.8.0/src/backends/drm/drm_commit_thread.cpp)
intentionally waits until the target pageflip minus its safety margin. The
base margin includes the mode's vblank interval plus
`KWIN_DRM_OVERRIDE_SAFETY_MARGIN` (default 1000 microseconds). Additional adaptive
margin uses the time the atomic ioctl returns, although kernel application of
the commit can continue afterwards. This matches the observed boundary and
provides a specific mitigation to test; it is not proof that increasing the
margin eliminates all stalls.

After these measurements, the user approved installing the temporary service
drop-in at `~/.config/systemd/user/plasma-kwin_wayland.service.d/90-wavebench-margin.conf`.
Its source and checksum record remain in `kwin-margin-test/` under the raw
diagnostic directory. Installation and `daemon-reload` are verified; the current
KWin process was not restarted. Validation awaits a fresh Plasma login. It sets
`KWIN_DRM_OVERRIDE_SAFETY_MARGIN=2000`. The companion README specifies installation,
checksum-based rollback and matched acceptance measurements. A fresh Plasma
login is needed because KWin reads this setting at startup. Restarting the live
Wayland compositor can close applications and must not be done automatically.
If validated, permanent configuration belongs in the NixOS repository.

## Evidence and limitations

[Compact matched kernel/native evidence](native-deadlines-2026-10-22.json)
contains the five gap windows, exact timestamps and capture integrity results.
Raw traces, controllers and source snapshots remain under
`/home/okhsunrog/tmp_zfs/codex_tmp/cef-frame-pacing/`.

- pipeline-1 rejected by the KWin executable identity guard before kernel tracing;
  its native-only capture is not used as synchronized evidence.
- pipeline-2 had an overwritten early kernel buffer; its late 50 ms gap remained
  in retained coverage. It showed two no-damage acknowledgements and no new
  Chromium swaps. It does not establish the cause of that separate renderer stall.
- pipeline-2/3 mistakenly enabled xe GPU probes, which stayed empty because this
  machine uses i915. Their DRM/scheduler data remain useful, but they provide no
  GPU-fence evidence. pipeline-4 onward correct this explicitly.
- No GC overlaps the reported presentation-gap intervals. This alone does not
  rule out effects from earlier work, GPU contention or other causes elsewhere.
- Tracing adds overhead. The missed physical deadline is directly observed, but
  the frequency of these gaps must be remeasured with lighter traces after any
  mitigation. A margin change may increase latency and is desktop-wide.
