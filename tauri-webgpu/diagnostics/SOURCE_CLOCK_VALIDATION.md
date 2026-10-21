# Source clock and plotted-head stalls

The producer previously used `deadline = max(deadline + period, now)` after
every batch. Whenever scheduling, generation or a credit wait consumed the
available time, this permanently shifted the sample schedule. The frontend
continued to advance at the configured sample rate. Repeated short delays could
therefore empty its roughly 25 ms receive buffer, even without a single long
delivery gap or delayed RAF callback. Rebuffering then held the displayed head
while the renderer continued drawing regular frames.

`ProducerClock` now preserves the original deadline and catches up after short
delays. The four-packet credit window still bounds queued data. After a long
pause, catchup debt is capped at the larger of 250 ms or four packet periods;
discarded schedule time and clock rebases remain visible in backend statistics.
Sample indices remain sequential and the frontend never draws beyond received
samples. The display buffer was not enlarged by this change.

`WAVEBENCH_SOURCE_CLOCK=catchup` is the default. `reset` retains the old policy
for comparisons with the same executable. Neither mode needs another CEF build.

## Measurements on 2026-10-22

[Recorded results](producer-clock-validation-2026-10-22.json) retain the flow,
clock and indexer-control captures. They used the physical 60 Hz panel at 135%
scale, hardware WebGPU with compute, a 4 million samples/s WebSocket stream and
16,384-sample batches. The screen was checked unlocked before and after captures.

A quiet 60-second old-policy flow capture had 3600 regular RAF callbacks but two
stationary-head intervals. The maximum consumed-packet gap was only 14.1 ms,
while discarded source-clock time increased by 36.59 ms during the sample.
Host CPU maximum was 12.0%. This distinguishes cumulative schedule loss from
a single delivery pause exceeding the entire display buffer.

The same-binary reset/catchup/catchup/reset series produced the following results:

| Clock policy | Stationary-head intervals | Discarded schedule time during sample | Host CPU maximum |
| --- | ---: | ---: | ---: |
| Reset | 24 | 816.80 ms | 74.3% |
| Catchup | 0 | 0 ms | 12.4% |
| Catchup | 0 | 0 ms | 9.8% |
| Reset | 50 | 1290.06 ms | 66.6% |

Each sample lasted 60 seconds. The reset samples had substantial background load,
so the counts are **not** an equal-load estimate of improvement. Both corrected
samples maintained all 3600 RAF callbacks and continuous movement, with no empty
buffer intervals, sequence gaps or compute errors. Measured delivery rates were
approximately 4.0001 and 3.9998 million samples/s. Worker receipts still sometimes
took about 25 ms; their existence alone did not imply a stalled plot.

The deterministic regression test uses the actual frontend `PlaybackClock` and
producer clock. With regular RAF and repeated 15 ms producer delays, the old
policy reproduces stationary-head intervals and the corrected policy eliminates
them. Simulated generation still takes time; catchup never moves simulated time
backwards. Separate tests check the long-pause cap and credit-bound enforcement.

## Competing desktop work

Read-only I/O sampling identified `agentsview-linu` writing 20–29 MB/s to its
SQLite database during high ZFS worker CPU activity. Temporarily pausing that
process, with an independent resume timer, gave a 45-second old-policy capture
with 2700 regular callbacks, continuous plot movement and CPU maximum 8.0%.
After resuming it, the same old-policy capture had CPU up to 70.7%, eight RAF
gaps and 21 stationary-head intervals. This supports a contribution from the
indexer under those conditions; it does not attribute every previous delay to it.

A final 30-second corrected-clock capture with the indexer temporarily paused
used a reduced Chromium trace to verify actual compositor presentation:

- 1800 RAF callbacks and 1800 successful native presentations.
- No stationary-head intervals, empty buffer intervals or compositor discards.
- Maximum native presentation interval 16.678 ms; hardware compute passed.
- No sequence gaps, with host CPU maximum 10.4%.

Both pauses were followed by a verified process resume and timer removal. No
compositor, filesystem, network or scheduler settings were changed. The source
fix prevents cumulative drift; it cannot supply unavailable data or guarantee
smooth rendering during arbitrary CPU/GPU starvation.

## Checks

Nine Rust tests passed, including the shared playback-clock tests, the new
clock regression and bounded-credit test. Five motion-analysis tests passed,
including detection of starvation hidden by regular RAF. The real Worker
transfer/copy/receipt regression passed. Both CEF and WebKitGTK release targets
built. A short WebKitGTK/WebGL2 compatibility capture delivered 600 callbacks
without a RAF gap, with compute explicitly disabled.

The normal CEF package and the experimental native pump preset remain separate
choices. This application-level correction is independent of that SDK choice;
the current SDK and retained cloud build cache do not need rebuilding for it.
