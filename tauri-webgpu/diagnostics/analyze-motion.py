"""Analyze movement separately from RAF and compositor presentation cadence."""
import json
import sys
from pathlib import Path
from urllib.parse import parse_qs


def analyze(report):
    query = parse_qs(report.get("query", "").lstrip("?"))
    rate = int(query.get("rate", [1_000_000])[0])
    motion = report.get("motion", [])
    stalls, slow, depleted = [], [], []
    ratios, offsets = [], []
    for a, b in zip(motion, motion[1:]):
        dt, dh, dr = b[0] - a[0], b[1] - a[1], b[2] - a[2]
        if dt <= 0 or dh < 0 or dr < 0 or b[1] > b[2]:
            raise ValueError("Invalid motion timeline or display beyond received data")
        ratio = dh / (dt * rate / 1000)
        ratios.append(ratio)
        offsets.append(b[3] * 1000 / rate)
        row = {"timestamp": b[0], "interval_ms": dt, "head_delta": dh,
               "received_delta": dr, "buffer_ms": offsets[-1], "speed_ratio": ratio}
        if dh == 0:
            stalls.append({**row, "starved": b[1] == b[2] and dr == 0})
        if ratio < 0.9:
            slow.append(row)
        if b[3] == 0:
            depleted.append(row)
    posted, receipts, consumed = {}, [], []
    for at, kind, seq, size, inflight in report.get("flow", []):
        if kind == "consumed":
            posted[seq] = at
            consumed.append(at)
        elif kind == "receipt" and seq in posted:
            receipts.append(at - posted.pop(seq))
    return {"mode": query.get("motion", ["clocked"])[0], "rate": rate,
            "motion_frames": len(motion), "flat_head_intervals": len(stalls),
            "flat_head_intervals_ms": sum(x["interval_ms"] for x in stalls),
            "starved_flat_intervals": sum(x["starved"] for x in stalls),
            "flat_with_raf_under25": sum(x["interval_ms"] <= 25 for x in stalls),
            "slow_intervals": len(slow), "empty_buffer_intervals": len(depleted),
            "speed_ratio_min": min(ratios, default=None),
            "speed_ratio_max": max(ratios, default=None),
            "buffer_min_ms": min(offsets, default=None),
            "buffer_max_ms": max(offsets, default=None),
            "matched_worker_receipts": len(receipts),
            "worker_receipt_max_ms": max(receipts, default=None),
            "consume_gap_max_ms": max((b-a for a,b in zip(consumed, consumed[1:])), default=None),
            "stalls": stalls[:100], "backend": report.get("backend")}


if __name__ == "__main__":
    tag = sys.argv[1]
    path = Path(tag)
    report = json.loads(path.with_suffix(".json").read_text())
    result = analyze(report)
    path.with_name(path.name + "-motion-analysis.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))
