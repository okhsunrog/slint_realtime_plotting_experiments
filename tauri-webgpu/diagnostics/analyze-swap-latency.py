#!/usr/bin/env python3
"""Match native buffer-swap sends to hardware presentation feedback.

Run after analyze-finalizers.py: uv run --no-project analyze-swap-latency.py TAG
This is backend swap-send to presentation latency, not data-to-pixel latency.
Trace timestamps and raw presentation timestamps must share CLOCK_MONOTONIC.
Python's JSON decoder preserves the 64-bit display_trace_id integers.
"""

import json
import statistics
import sys
from pathlib import Path


def analyze(events, sample):
    lo, hi = sample
    swaps = {}
    for event in events:
        args = event.get("args", {}).get("chrome_graphics_pipeline", {})
        if args.get("step") == "STEP_BACKEND_SEND_BUFFER_SWAP":
            identifier = args["display_trace_id"]
            if identifier in swaps:
                raise ValueError(f"Duplicate swap ID: {identifier}")
            swaps[identifier] = event["ts"]

    values = []
    missing = 0
    for event in events:
        if event.get("name") != "Wavebench.PresentationFeedback":
            continue
        if not lo <= event["ts"] < hi:
            continue
        args = event["args"]
        if args["flags"] & 16 or args.get("sanitized_flags", 16) & 16:
            continue
        start = swaps.get(args["display_trace_id"])
        if start is None:
            missing += 1
            continue
        delta = (args["raw_timestamp_us"] - start) / 1000
        if delta < 0:
            raise ValueError(f"Negative presentation latency: {delta}")
        values.append(delta)
    if not values:
        raise ValueError("No valid matched presentations")
    values.sort()
    return {
        "matched": len(values),
        "missing": missing,
        "swap_to_present_p50_ms": statistics.median(values),
        "swap_to_present_p95_ms": values[round((len(values) - 1) * 0.95)],
        "swap_to_present_max_ms": max(values),
    }


if __name__ == "__main__":
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    for tag in sys.argv[1:]:
        sample = json.loads(Path(tag + "-analysis.json").read_text())["sample"]
        events = json.loads(Path(tag + "-trace.json").read_text())["traceEvents"]
        result = {"tag": tag, **analyze(events, sample)}
        Path(tag + "-latency.json").write_text(json.dumps(result, indent=2) + "\n")
        print(json.dumps(result))
