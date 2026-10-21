# CEF GTK reader and Wavebench scrolling validation

The GTK-owned reader removes the measured reader barrier, but did not give a consistent frame-pacing improvement across all workloads. It remains opt-in. Buffered scrolling is a separate frontend change; residual native/engine frame stalls remain.

## Native reader experiment

Identical SDK and executable for OFF/ON/ON/OFF, separately for WebGPU/WebGL2 at 1M/4M three-channel samples/s. Binary WebSocket, batch 16384, visible 262144, 1280x368 physical canvas, DPR1, isolated virtual 60Hz KWin, hardware Intel Arc. Each run: 5s warmup + 30s sample. No builds, screenshots or driver inspection during accepted samples. Actual wayland-fd presence/absence checked in every report.

| Rate | Renderer | OFF intervals >25ms | ON intervals >25ms | Worst OFF/ON interval, ms |
|---|---|---|---|---|
| 1000000 | webgpu | 39 / 19 | 44 / 8 | 50.1 / 316.7 |
| 1000000 | webgl | 29 / 16 | 24 / 10 | 133.4 / 116.6 |
| 4000000 | webgpu | 48 / 29 | 20 / 9 | 300.0 / 100.0 |
| 4000000 | webgl | 61 / 9 | 63 / 11 | 250.0 / 83.4 |

All 16 primary reports: correct geometry, visible document, no recorded JS errors, transport sequence gaps 0; WebGPU compute check passed. The two samples per condition and variable host scheduling do not support a universal speedup claim. In particular, WebGPU 4M improved in this sample, while WebGL 4M did not consistently improve.

## Direct GTK reader probe

These instrumented runs are separate from the performance table. Only the browser UI thread is recorded; the probe wraps dynamic GTK/libwayland read_events and reader_cond waits. CEF bundles its own hidden Wayland functions.

| Mode | GTK reads | Longest read, ms | reader_cond waits | Longest wait, ms |
|---|---|---|---|---|
| baseline | 4345 | 3.006 | 3519 | 3.001 |
| patched | 6519 | 0.781 | 0 | 0.000 |

The previously recorded 23-61ms GTK waits were not reproduced in this control. Their mechanism is removed in the ON probe; that does not explain every remaining RAF stall.

## Buffered scrolling

The final sample clock targets two batches plus one 60Hz interval and limits backlog to one additional batch. It catches up after excess lag, then resumes the configured sample rate. Transport packets are retained; the live display may make a corrective jump on recovery. Five focused clock tests pass.

Packet/clocked tests below use the same bounded-clock executable and GTK reader mode. One sample per condition, packet then clocked: their FPS differences are not treated as evidence of a pacing gain. The outcome is sample-head movement per RAF timestamp. Ordinary intervals are 10-25ms; long native stalls are not hidden in the raw RAF summaries.

| Test | Ordinary intervals within 2 samples of clock rate | No-packet frames that still moved | Buffer p50 / max, ms |
|---|---|---|---|
| motion-webgl-1000000-clocked | 100.00% | 8 / 8 | 49.40 / 59.08 |
| motion-webgl-1000000-packet | 0.00% | 0 / 3 | 0.00 / 0.00 |
| motion-webgl-4000000-clocked | 99.49% | 0 / 0 | 22.20 / 28.67 |
| motion-webgl-4000000-packet | 0.00% | 0 / 2 | 0.00 / 0.00 |
| motion-webgpu-1000000-clocked | 100.00% | 9 / 9 | 46.31 / 57.32 |
| motion-webgpu-1000000-packet | 0.00% | 0 / 6 | 0.00 / 0.00 |
| motion-webgpu-4000000-clocked | 99.38% | 0 / 1 | 18.83 / 28.68 |
| motion-webgpu-4000000-packet | 0.00% | 0 / 7 | 0.00 / 0.00 |

## GUI and scale checks

- GTK-reader mode: native HTML select popup opened and closed with Escape; graph updates continued while it was open. ASCII field editing and Ctrl+A worked with paced EIS events; frequency 11 Hz was applied and the waveform updated.
- Unpaced synthetic input was unreliable (missed/combined actions); it was not accepted as a passed input test. Paced key/button down/up events were used for the accepted checks. This is a virtual-session check, not physical keyboard acceptance.
- Freeze retained the displayed history while transport continued. KWin scripting resized the test window from 1440x1000 to 1100x800; frozen GPU content rebuilt correctly and the layout reflowed. Apply/restart resumed live rendering.
- 125%: isolated 2160x1620 virtual output, JS DPR1.25, canvas1280x369, hardware WebGPU and 102-extrema compute PASS, no recorded JS errors. The initial exact-size assertion was relaxed only for fractional-scale smoke checks: CSS integer client-size rounding accounts for the one-pixel height difference.
- 150%: same isolated output, JS DPR1.5, canvas1281x369, hardware WebGPU and compute PASS, no recorded JS errors. Both dimensions are within one pixel of the fixed diagnostic target. These 5-second runs are functional scale smoke checks, not performance comparisons.
- Final CEF and WebKitGTK release binaries were rebuilt. WebKitGTK/WebGL2 with the new scrolling frontend passed a separate 5-second smoke check.
- GUI and fractional-scale applications closed normally; both owned virtual sessions were stopped. The user's desktop configuration was not changed.


## Reproduction and provenance

- SDK archive SHA256: b20026d7ec4eaefbf7035427e4e34b3f4ccbf7d32f06d8814fb93d494891f836.
- Original libcef.so SHA256: a510fd54890b575f832700509bbd1775db9ce711ae463f749b2373960c5eb94d.
- Reader patch SHA256: 1ad8496c4e63c3a3e276b72ff804c43fe7d82908a97121ecb04e7a40adc3c147.
- CEF base a09c27de3 + recorded Chromium reader overlay; Chromium b859317bf11f6be47f9b7799ec690a0a42a1fb33. Generated SDK version label is the CEF base, not a claim of an unmodified build.
- Per-run raw JSON, host samples, mode/thread proof, journals, reader CSV and executable hashes are retained in the local task artifact directory; the release includes this summarized validation. Native experiment source: project 756e9b3; final clock/selector source: project ad74d32.
- Initial SDK-v2 comparisons with ignored CLI switches are INVALID and retained in invalid-native-mode/. They are not used in any table above. Initial unbounded-clock reports are retained in unbounded-clock/. A launcher requireFile startup failure is excluded and documented in ../VALIDATION.md.
- RAF callback timing in a virtual compositor is not physical display scanout timing. WebKitGTK's prior rare 192ms event was not investigated by these changes.
