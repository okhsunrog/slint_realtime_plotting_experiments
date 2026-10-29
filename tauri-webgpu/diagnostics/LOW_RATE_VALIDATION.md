# Physical screen check at 20,000 samples/s

Measured on 2026-10-22 with the corrected source clock (`dd3a23f`), hardware
WebGPU/compute, binary WebSocket, Worker reclamation, the physical 60 Hz panel
and 135% scale. [Full results](lowrate-validation-2026-10-22.json) retain all
captures, including the run affected by competing desktop work.

Three 60-second captures ran without Chromium tracing or DevTools. The usual
five-second probe warmup was retained, and raw startup observations were saved.
Steady movement was analyzed from 20 seconds of stream age: the default
262,144-sample visible history needs about 13.1 seconds to fill at this rate.
The steady interval in each capture was therefore about 45 seconds.

| Samples per batch | Nominal packet period | Display buffer median / p95 | Steady head stalls | RAF gaps over 25 ms | Host CPU max |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 16,384 | 819.2 ms | 1269 / 1638 ms | 0 | 0 | 9.6% |
| 256 | 12.8 ms | 38 / 49 ms | 0 | 17 | 55.8% |
| 1,024 | 51.2 ms | 110 / 133 ms | 0 | 0 | 10.3% |

The display-buffer metric measures distance behind the received sample head;
it is not a direct hardware acquisition latency. A large batch can provide
regular scrolling while displaying old data. For responsiveness at 20k/s,
256-sample batches are preferable to the 16,384-sample throughput preset.

The 256-sample capture still had RAF intervals up to 50 ms during background
CPU activity. Lower data throughput therefore does not guarantee freedom from
frame delays elsewhere in the system. Data-head movement remained continuous;
that is distinct from the regularity of actual updates on the display.

A subsequent 30-second native-presentation control used 256-sample batches
with the session indexer temporarily paused under an independent resume timer.
It verified 1800 RAF callbacks and 1800 successful native presentations, no
compositor discards, and a maximum native interval of 16.678 ms. Host CPU maximum
was 6.2%. Steady plotted-head movement also had no stalls. This supports a
contribution from competing desktop work, without attributing every possible
delay to that process. The indexer was verified resumed and the timer removed.

All captures passed hardware compute and stream-sequence checks. No application
code, display configuration, SDK or system tuning changed for this comparison.

## Repeated WebGPU check and new interactive defaults

The CEF frontend now defaults to 20,000 samples/s and 256 samples per packet.
WebGPU with compute reduction was already enabled during the preceding tests
and remains the default. WebGL2 is available as an explicit comparison option.
The application was rebuilt; the CEF SDK was unchanged.

Two further untraced 60-second captures used the rebuilt application, hardware
Intel Xe-LPG WebGPU, compute reduction, binary WebSocket and Worker reclamation.
As above, steady movement is ranked from 20 seconds of stream age (45 seconds
per capture); full raw startup data remain in the local diagnostic directory.

| Run | Steady RAF frames | RAF intervals over 25 ms | Flat-head intervals | Display buffer median / p95 | Host CPU max |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1 | 2698 | 2 (max 33.4 ms) | 3 | 42.1 / 48.9 ms | 89.0% |
| 2 | 2700 | 0 (max 16.8 ms) | 0 | 36.9 / 42.6 ms | 11.4% |

The loaded first capture also had two empty-buffer observations and a 69.3 ms
maximum consume gap. The second had neither empty buffers nor slow-head
intervals. Both passed the actual wrapped-ring/spike compute check and had
zero stream sequence gaps. These captures do not establish immunity to host
load or identify a specific cause for every delay.

A subsequent 30-second native-presentation capture with the indexer temporarily
paused under the previously authorized automatic recovery procedure recorded
1800 successful native presentations, no gaps or discards, and a maximum native
interval of 16.678 ms. Its steady 15-second motion segment had no head stalls,
slow-head intervals or empty buffers; the display buffer ranged from 34.1 to
47.3 ms. Host CPU maximum was 7.3%. The indexer was verified resumed and its
recovery timer removed.

[Repeat results and binary identity](webgpu-20k-repeat-2026-10-22.json) retain
the loaded capture alongside the quieter run and native control. Raw captures
are under `/home/okhsunrog/tmp_zfs/codex_tmp/cef-frame-pacing/`, with prefixes
`webgpu-restored-20k-` and `physical-indexer-paused-webgpu20k-repeat`.
