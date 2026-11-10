"""Score file retrieval separately from modality, exact text and temporal evidence.
Consumes JSON ground truth and JSONL {query:{id:...},response:{results:[...]}}.
No product code or model is imported; raw responses are treated as data.
"""
import argparse
import json
from collections import defaultdict
from pathlib import Path


def basename(value):
    return value.replace("\\", "/").rsplit("/", 1)[-1]


def evidence(hit):
    if hit.get("evidence"):
        return hit["evidence"]
    kind = hit.get("kind", "")
    return [{"source": "audio" if kind == "video_audio" else "video_ocr" if kind == "video" and hit.get("text") else "video_frame" if kind == "video" else "text",
             "seconds": hit.get("seconds", -1), "page": hit.get("page", 0), "text": hit.get("text", "")}]


def evidence_checks(query, hit):
    """Keep component scores as well as joint support; all checks require a correct file."""
    timed = bool(query.get("time_ranges"))
    paged = bool(query.get("page_ranges"))
    exact = bool(query.get("exact_match"))
    ordered = bool(query.get("ordered"))
    checks = {"timestamp": None, "modality": None, "exact_text": None, "page": None, "order": None}
    applicable = {"timestamp": timed, "modality": timed, "exact_text": exact, "page": paged, "order": ordered}
    if not any(applicable.values()):
        return checks, None
    correct = hit is not None and basename(hit.get("path", "")) in query["expected_files"]
    if not correct:
        return {k: False if applicable[k] else None for k in checks}, False
    name = basename(hit["path"])
    points = evidence(hit)
    token = query.get("exact_text", query["query"]).casefold()
    if exact:
        checks["exact_text"] = any(token in e.get("text", "").casefold() for e in points)
    def modality(e, r):
        source = e.get("source", "")
        if r.get("modality") == "visual":
            return source in ("video_frame", "video_ocr", "image", "image_ocr")
        if r.get("modality") == "audio":
            return source == "audio"
        return True
    joint = []
    if timed:
        ranges = [r for r in query["time_ranges"] if r["file"] == name]
        checks["modality"] = bool(ranges) and all(any(modality(e, r) for e in points) for r in ranges)
        checks["timestamp"] = bool(ranges) and all(any(r["start_seconds"] <= e.get("seconds", -1) <= r["end_seconds"] for e in points) for r in ranges)
        for r in ranges:
            joint.append([e.get("seconds", -1) for e in points if modality(e, r)
                          and r["start_seconds"] <= e.get("seconds", -1) <= r["end_seconds"]
                          and (not exact or token in e.get("text", "").casefold())])
    if paged:
        ranges = [r for r in query["page_ranges"] if r["file"] == name]
        checks["page"] = bool(ranges) and all(any(r["start_page"] <= e.get("page", 0) <= r["end_page"] for e in points) for r in ranges)
    if ordered:
        last = -1
        checks["order"] = bool(joint)
        for candidates in joint:
            later = [t for t in candidates if t > last]
            if not later:
                checks["order"] = False
                break
            last = min(later)
    supported = all(v for v in checks.values() if v is not None)
    if timed:
        supported = supported and bool(joint) and all(joint)
    return checks, bool(supported)


def score(truth, responses, category_scoped=False):
    queries = {q["id"]: q for q in truth["queries"]}
    files = {f["file"]: f for f in truth["files"]}
    rows = []
    seen = set()
    for raw in responses:
        qid = raw["query"]["id"]
        if qid in seen or qid not in queries:
            raise ValueError("Unknown or duplicate query ID: " + qid)
        seen.add(qid)
        q = queries[qid]
        response = raw["response"]
        if response.get("ok") is False or raw.get("error"):
            hits = []
        else:
            hits = response.get("results", response.get("hits", []))
        unique = []
        paths = set()
        for hit in hits:
            name = basename(hit.get("path", hit.get("file_path", "")))
            if category_scoped and (name not in files or files[name]["category"] != q["category"]):
                continue
            if name not in paths:
                paths.add(name)
                unique.append((name, hit))
        rank = next((i + 1 for i, (name, _) in enumerate(unique) if name in q["expected_files"]), None)
        top = unique[0] if unique else None
        checks, supported = evidence_checks(q, top[1] if top else None)
        rows.append({"id": qid, "group": q.get("group", q.get("category", "other")), "rank": rank,
                     "top1": rank == 1, "top5": rank is not None and rank <= 5, "evidence_top1": supported, "checks": checks})
    missing = set(queries) - seen
    if missing:
        raise ValueError("Missing query responses: " + ", ".join(sorted(missing)))
    def aggregate(items):
        annotated = [r for r in items if r["evidence_top1"] is not None]
        components = {key: {"queries": sum(r["checks"][key] is not None for r in items), "top1": sum(r["checks"][key] is True for r in items)} for key in ("timestamp", "modality", "exact_text", "page", "order")}
        return {"components": components, "queries": len(items), "top1": sum(r["top1"] for r in items), "top5": sum(r["top5"] for r in items),
                "evidence_queries": len(annotated), "evidence_top1": sum(r["evidence_top1"] for r in annotated)}
    groups = defaultdict(list)
    for row in rows:
        groups[row["group"]].append(row)
    return {"category_scoped": category_scoped, "overall": aggregate(rows), "groups": {k: aggregate(v) for k, v in groups.items()}, "rows": rows}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--truth", type=Path, required=True)
    parser.add_argument("--responses", type=Path, required=True)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--category-scoped", action="store_true", help="Reproduce category-filtered reports; default keeps all distractors")
    args = parser.parse_args()
    result = score(json.loads(args.truth.read_text(encoding="utf-8-sig")),
                   [json.loads(line) for line in args.responses.read_text(encoding="utf-8-sig").splitlines() if line.strip()], args.category_scoped)
    text = json.dumps(result, ensure_ascii=False, indent=2)
    if args.output:
        args.output.write_text(text, encoding="utf-8")
    print(json.dumps(result["overall"]))

if __name__ == "__main__":
    main()
