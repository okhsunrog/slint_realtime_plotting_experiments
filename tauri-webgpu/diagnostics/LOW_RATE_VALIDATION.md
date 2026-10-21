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
