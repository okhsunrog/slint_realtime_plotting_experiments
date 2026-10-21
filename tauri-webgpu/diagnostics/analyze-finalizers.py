import collections
import itertools
import json
import statistics
import sys
from pathlib import Path

r = Path.cwd()
tag = sys.argv[1]
d = json.loads((r / (tag + ".json")).read_text())
es = json.loads((r / (tag + "-trace.json")).read_text())["traceEvents"]
mark = next(e for e in es if e["name"] == "wavebench-clock-sync")
pid = mark["pid"]
offset = mark["ts"] - mark["args"]["data"]["startTime"] * 1000
lo = offset + d["start"] * 1000
hi = offset + d["end"] * 1000
main = mark["tid"]


def clipped(e):
    return max(0, min(hi, e["ts"] + e.get("dur", 0)) - max(lo, e["ts"])) / 1000


objects = [
    e
    for e in es
    if e["pid"] == pid
    and e["tid"] == main
    and e["name"] == "CppGC.FinalizeObject"
    and e.get("ph") == "X"
    and lo <= e["ts"] < hi
]
bytype = collections.defaultdict(list)
for e in objects:
    bytype[
        (e.get("args", {}).get("type", "?"), e.get("args", {}).get("gc_info_index", -1))
    ].append(e)
types = []
for (name, index), a in bytype.items():
    types.append(
        {
            "type": name,
            "index": index,
            "count": len(a),
            "total_ms": sum(map(clipped, a)),
            "max_ms": max(map(clipped, a)),
            "over1ms": sum(clipped(e) > 1 for e in a),
        }
    )
types.sort(key=lambda x: x["total_ms"], reverse=True)
phases = {}
for name in [
    "CppGC.FinalizePage",
    "CppGC.SweepFinalizeSweptPages",
    "CppGC.SweepOnAllocation",
]:
    a = [
        e
        for e in es
        if e["pid"] == pid
        and e["tid"] == main
        and e["name"] == name
        and e.get("ph") == "X"
        and lo <= e["ts"] < hi
    ]
    phases[name] = {
        "count": len(a),
        "total_ms": sum(map(clipped, a)),
        "max_ms": max(map(clipped, a), default=0),
    }
gc = [
    e
    for e in es
    if e["pid"] == pid
    and e["tid"] == main
    and e["name"] == "ThreadControllerImpl::RunTask"
    and e.get("ph") == "X"
    and "cppgc" in e.get("args", {}).get("src_file", "")
    and e["ts"] < hi
    and e["ts"] + e.get("dur", 0) > lo
]


def merge_intervals(events):
    spans = sorted(
        (max(lo, e["ts"]), min(hi, e["ts"] + e.get("dur", 0)))
        for e in events
        if e["ts"] < hi and e["ts"] + e.get("dur", 0) > lo
    )
    merged = []
    for start, end in spans:
        if merged and start <= merged[-1][1]:
            merged[-1][1] = max(end, merged[-1][1])
        else:
            merged.append([start, end])
    return merged


cppgc_spans = merge_intervals(
    [
        e
        for e in es
        if e["pid"] == pid
        and e["tid"] == main
        and e.get("ph") == "X"
        and e["name"].startswith("CppGC.")
    ]
)
v8_spans = merge_intervals(
    [
        e
        for e in es
        if e["pid"] == pid
        and e["tid"] == main
        and e.get("ph") == "X"
        and ("v8.gc" in e.get("cat", "") or e["name"].startswith("V8.GC"))
    ]
)


def span_overlap(start, end, spans):
    return sum(max(0, min(end, b) - max(start, a)) / 1000 for a, b in spans)


feedback = [
    e
    for e in es
    if e["name"] == "Wavebench.PresentationFeedback" and lo <= e["ts"] < hi
]
flags = collections.Counter(str(e["args"].get("flags")) for e in feedback)
valid = [
    e
    for e in feedback
    if e["args"].get("raw_timestamp_us", 0) > 0
    and not (e["args"].get("flags", 16) & 16)
    and not (e["args"].get("sanitized_flags", 16) & 16)
]
wp = [e for e in es if e["name"] == "Wavebench.WaylandPresented" and lo <= e["ts"] < hi]
times = sorted({e["ts"] for e in wp})
gaps = []
steps = collections.defaultdict(dict)
for e in es:
    a = e.get("args", {}).get("chrome_graphics_pipeline", {})
    ident = a.get("display_trace_id")
    step = a.get("step")
    if ident is not None and step in [
        "STEP_BUFFER_SWAP_POST_SUBMIT",
        "STEP_BACKEND_SEND_BUFFER_SWAP",
    ]:
        steps[ident][step] = e["ts"]
backends = []
for ident, a in steps.items():
    if len(a) == 2:
        start = a["STEP_BUFFER_SWAP_POST_SUBMIT"]
        end = a["STEP_BACKEND_SEND_BUFFER_SWAP"]
        if lo <= start < hi and end >= start:
            backends.append(
                {"id": ident, "start": start, "end": end, "ms": (end - start) / 1000}
            )


def overlap(start, end, a):
    return max(0, min(end, a["ts"] + a.get("dur", 0)) - max(start, a["ts"])) / 1000


for start, end in itertools.pairwise(times):
    if end - start > 25000:
        g = [e for e in gc if overlap(start, end, e) > 0]
        native = [
            e for e in backends if e["ms"] > 8 and e["start"] < end and e["end"] > start
        ]
        gaps.append(
            {
                "start": start,
                "end": end,
                "ms": (end - start) / 1000,
                "gc_overlap_ms": span_overlap(start, end, cppgc_spans),
                "v8_gc_overlap_ms": span_overlap(start, end, v8_spans),
                "gc_task_max_ms": max((e["dur"] / 1000 for e in g), default=0),
                "backend_delays": native,
            }
        )
intervals = [(b - a) / 1000 for a, b in itertools.pairwise(times)]
clock = json.loads(
    json.loads((r / (tag + "-clock.json")).read_text())["result"]["value"]
)
host = json.loads((r / (tag + "-host.json")).read_text())
cpu = [
    x["cpu_busy_percent"]
    for x in host
    if clock["timeOrigin"] / 1000 + d["start"] / 1000
    <= x["wall_time"]
    <= clock["timeOrigin"] / 1000 + d["end"] / 1000
]
result = {
    "tag": tag,
    "sample": [lo, hi],
    "pid": pid,
    "tid": main,
    "delivery": d["delivery"],
    "raf": d["summary"],
    "errors": d["errors"],
    "status": d["endText"],
    "object_types": types,
    "phases": phases,
    "gc_task_max_ms": max((clipped(e) for e in gc), default=0),
    "cppgc_total_union_ms": sum((b - a) / 1000 for a, b in cppgc_spans),
    "cppgc_longest_union_ms": max(((b - a) / 1000 for a, b in cppgc_spans), default=0),
    "v8_total_union_ms": sum((b - a) / 1000 for a, b in v8_spans),
    "feedback_count": len(feedback),
    "feedback_flags": flags,
    "valid_feedback_count": len(valid),
    "wayland_presented_count": len(wp),
    "wayland_flags": collections.Counter(
        str(e.get("args", {}).get("protocol_flags")) for e in wp
    ),
    "unique_presentation_count": len(times),
    "feedback_timestamp_difference_max_us": max(
        (abs(e["ts"] - e["args"]["raw_timestamp_us"]) for e in valid), default=0
    ),
    "presentation_max_ms": max(intervals, default=0),
    "presentation_over25": len(gaps),
    "presentation_gaps": gaps,
    "backend_sample_count": len(backends),
    "backend_over8": [e for e in backends if e["ms"] > 8],
    "host_cpu_median": statistics.median(cpu) if cpu else None,
    "host_cpu_max": max(cpu, default=0),
}
(r / (tag + "-analysis.json")).write_text(json.dumps(result, indent=2))
print(
    json.dumps(
        {
            k: v
            for k, v in result.items()
            if k not in ["presentation_gaps", "backend_over8", "object_types"]
        },
        indent=2,
    )
)
print("TYPES", json.dumps(types[:20], indent=2))
print(
    "GAPS", json.dumps(sorted(gaps, key=lambda x: x["ms"], reverse=True)[:10], indent=2)
)
