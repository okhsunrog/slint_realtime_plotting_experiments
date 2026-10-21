import collections
import json
import sys
from pathlib import Path

r = Path(".")
tag = sys.argv[1]
es = json.loads((r / (tag + "-trace.json")).read_text())["traceEvents"]
d = json.loads((r / (tag + "-analysis.json")).read_text())
lo, hi = d["sample"]
records = {}
for e in es:
    a = e.get("args", {}).get("chrome_graphics_pipeline", {})
    if a.get("step") == "STEP_BACKEND_SEND_BUFFER_SWAP":
        records[a["display_trace_id"]] = {
            "frame_id": a.get("backend_frame_id"),
            "time": e["ts"],
        }
feedback = [
    e
    for e in es
    if e["name"] == "Wavebench.PresentationFeedback" and lo <= e["ts"] < hi
]
failed = [e for e in feedback if e["args"]["flags"] & 16]
store = [e for e in es if e["name"] == "StoreFeedback"]
handlers = [
    e
    for e in es
    if e["name"] == "WaylandFrameManager::HandleFeedback" and e.get("ph") == "X"
]
rows = []
for e in failed:
    a = e["args"]
    record = records.get(a["display_trace_id"], {})
    fid = record.get("frame_id")
    matches = [v for v in store if v.get("args", {}).get("frame_id") == fid]
    parent = [
        {
            "discarded": h["args"].get("discarded"),
            "start": h["ts"],
            "duration": h.get("dur", 0),
        }
        for v in matches
        for h in handlers
        if h["pid"] == v["pid"]
        and h["tid"] == v["tid"]
        and h["ts"] <= v["ts"] <= h["ts"] + h.get("dur", 0)
    ]
    rows.append(
        {
            "display_trace_id": a["display_trace_id"],
            "frame_id": fid,
            "failed_at": e["ts"],
            "record": record,
            "store": [{"ts": v["ts"], "args": v["args"]} for v in matches],
            "handler": parent,
        }
    )
result = {
    "tag": tag,
    "feedback": len(feedback),
    "failed": len(failed),
    "successful_protocol_callbacks": d["wayland_presented_count"],
    "wp_discarded_handlers": sum(
        h["args"].get("discarded") is True for h in handlers if lo <= h["ts"] < hi
    ),
    "failed_frames": rows,
    "wayland_events": dict(
        collections.Counter(
            e["name"]
            for e in es
            if "wayland" in e.get("cat", "") and lo <= e.get("ts", 0) < hi
        )
    ),
}
(r / (tag + "-feedback.json")).write_text(json.dumps(result, indent=2))
print(json.dumps(result, indent=2))
