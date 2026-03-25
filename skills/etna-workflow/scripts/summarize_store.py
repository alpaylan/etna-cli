#!/usr/bin/env python3
"""Summarize ETNA store.jsonl with deduped logical trials."""

from __future__ import annotations

import argparse
import json
from collections import Counter
from pathlib import Path
from statistics import median
from typing import Any, Dict, Iterable, List, Optional, Tuple


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Summarize ETNA store.jsonl")
    parser.add_argument("--store", required=True, help="Path to store.jsonl")
    parser.add_argument("--experiment", help="Experiment name filter (optional)")
    return parser.parse_args()


def parse_ns(value: Any) -> Optional[int]:
    if isinstance(value, str) and value.endswith("ns"):
        try:
            return int(value[:-2])
        except ValueError:
            return None
    return None


def load_rows(path: Path, experiment: Optional[str]) -> List[Dict[str, Any]]:
    rows: List[Dict[str, Any]] = []
    with path.open() as f:
        for line in f:
            raw = json.loads(line)
            data = raw.get("data", raw)
            if experiment and data.get("experiment") != experiment:
                continue
            rows.append(data)
    return rows


def dedupe_latest(rows: Iterable[Dict[str, Any]]) -> List[Dict[str, Any]]:
    latest: Dict[Tuple[Any, ...], Dict[str, Any]] = {}
    for row in rows:
        key = (
            row.get("workload"),
            tuple(row.get("mutations", [])),
            row.get("property"),
            row.get("strategy"),
            row.get("trial"),
        )
        prev = latest.get(key)
        ts = row.get("timestamp", "")
        if prev is None or ts > prev.get("timestamp", ""):
            latest[key] = row
    return list(latest.values())


def pct(num: int, den: int) -> float:
    return round((100.0 * num / den), 1) if den else 0.0


def summarize(rows: List[Dict[str, Any]]) -> None:
    print(f"raw_rows={len(rows)}")
    clean = dedupe_latest(rows)
    print(f"dedup_rows={len(clean)}")
    print()

    combos = Counter((r.get("workload"), r.get("strategy")) for r in clean)
    print("workload/strategy row counts:")
    for (workload, strategy), n in sorted(combos.items()):
        print(f"  {workload}/{strategy}: {n}")
    print()

    print(
        "workload,strategy,n,failed,timed_out,passed,"
        "fail_pct,timeout_pct,pass_pct,kill_on_completed_pct,median_fail_s"
    )
    workloads = sorted({r.get("workload") for r in clean})
    for workload in workloads:
        for strategy in ["hegel", "quickcheck", "proptest"]:
            subset = [
                r for r in clean
                if r.get("workload") == workload and r.get("strategy") == strategy
            ]
            if not subset:
                continue
            status = Counter(r.get("status") for r in subset)
            failed = status.get("failed", 0)
            timed_out = status.get("timed_out", 0)
            passed = status.get("passed", 0)
            done = failed + passed
            kill_done = round((100.0 * failed / done), 1) if done else None
            fail_ns = [
                parse_ns(r.get("time"))
                for r in subset
                if r.get("status") == "failed" and parse_ns(r.get("time")) is not None
            ]
            med_fail_s = round(median(fail_ns) / 1e9, 3) if fail_ns else None
            kill_done_s = "NA" if kill_done is None else str(kill_done)
            med_fail_s_s = "NA" if med_fail_s is None else str(med_fail_s)
            print(
                f"{workload},{strategy},{len(subset)},{failed},{timed_out},{passed},"
                f"{pct(failed, len(subset))},{pct(timed_out, len(subset))},"
                f"{pct(passed, len(subset))},{kill_done_s},{med_fail_s_s}"
            )


def main() -> None:
    args = parse_args()
    path = Path(args.store)
    if not path.exists():
        raise SystemExit(f"store not found: {path}")
    rows = load_rows(path, args.experiment)
    summarize(rows)


if __name__ == "__main__":
    main()
