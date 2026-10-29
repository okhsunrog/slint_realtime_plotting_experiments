# KWin margin validation, 2026-10-29

The live KWin environment now contains `KWIN_DRM_OVERRIDE_SAFETY_MARGIN=2000`.
PID 3553 / commit thread 456370 replaced the previous session's processes.
The temporary service override remains installed. No compositor restart or
additional host tuning was performed during this validation.

The physical display remains 60 Hz / DPR 1.35. All five captures use the same
immutable application binary and CEF SDK as the earlier diagnosis, 20k samples/s,
batch 256, WebGPU/compute, render worker, Wayland frame source, FIFO off and a
4 ms pump limit. AgentsView was not running; no process was suspended.

## Result

| Capture | Kernel recording | Native frames | Intervals >25 ms | Max interval | Host CPU median / max |
| --- | --- | ---: | ---: | ---: | ---: |
| 1 | yes | 1798 | 1 | 50.003 ms | 9.0 / 17.9% |
| 2 | yes | 1756 | 29 | 150.001 ms | 16.2 / 36.8% |
| 3 | yes | 1799 | 1 | 33.331 ms | 7.3 / 12.9% |
| light-1 | no | 1800 | 0 | 16.678 ms | 6.0 / 8.1% |
| light-2 | no | 1800 | 0 | 16.677 ms | 6.1 / 12.6% |

The two lighter native traces total 3600 physical presentations without an
interval above 25 ms. Their swap-send to presentation medians are 22.38 and
21.78 ms; these are not end-to-end sample-to-pixel latency measurements.
All captures passed compute readback and sequence integrity, with no application
errors or Chromium trace loss. Kernel recordings had no overruns or dropped events.

The observed pipe-update timing moved in the expected direction:

- Old 1000 us baseline: median update-to-next-hardware-vblank headroom 1.43 ms,
  with two updates entering the unsafe scanline range or crossing the frame counter.
- New 2000 us captures: median headroom 2.37 / 2.42 / 2.43 ms; zero such updates
  among 1675 / 1754 / 1800 updates recorded inside the respective native samples.

This supports the intended deadline mitigation, but does not establish that all
stutters are fixed. The running KWin store build also changed after login, though
both builds report 6.8.0. There has not been a same-build 1000/2000 ABBA comparison.
The override remains an experiment, not a permanent NixOS setting.

## Remaining renderer stall

Capture 3's sole 33.331 ms gap has regular i915 updates throughout the gap,
around scanlines 1321–1328, well before the unsafe range 1495–1504. On the actual
render worker (renderer PID 2334364, trace TID 27), a major GC takes 3.79 ms,
followed by cppgc incremental sweeping tasks of 12.455 and 7.587 ms. The first
sweep overlaps the expected callback time; Chromium acknowledges no damage,
and the worker skips a RAF interval. Later sweeping tasks take another 5–8 ms.

This is a separate upstream-of-presentation delay worth investigating next.
The old analyzer's `gc_overlap_ms=0` does not rule it out: that metric keys on
specific event names and the presentation-gap interval, while the source-tagged
`ThreadControllerImpl::RunTask` events expose sweeping on the worker, including
work before the last presented frame. Do not label this case GC-free.

## Measurement caveats

Capture 2 encountered substantial browser, memory-reclaim and ZFS activity.
Its results are retained, but not ranked against the quieter baseline as an
equal-load test. Kernel tracing itself also allocates buffers and produces output.
The first two kernel recordings ended before the native sample did: capture 1's
only gap and capture 2's last gap are outside kernel coverage and may be affected
by trace teardown/export. The other 28 gaps in capture 2 have covered windows.
Capture 3 extends recording to 36 seconds, covering its complete gap window.
The final two captures omit kernel recording to reduce measurement disturbance.

[Full results and timing summaries](kwin-margin-validation-2026-10-29.json)
retain summaries of all five captures. Large raw traces were selected for removal
during [experiment closeout](CLOSEOUT.md); reproducible helper scripts are retained
under `/home/okhsunrog/tmp_zfs/codex_tmp/cef-frame-pacing/`.
