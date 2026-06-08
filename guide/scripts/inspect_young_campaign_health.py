#!/usr/bin/env python3
from __future__ import annotations

import json
import sys
from pathlib import Path


def load_json(path: Path):
    if not path.exists():
        return None
    return json.loads(path.read_text())


def as_str(value):
    if value is None:
        return "-"
    if isinstance(value, float):
        return f"{value:.1f}"
    return str(value)


def metric_value(metric: dict, key: str, legacy_key: str | None = None):
    job_scoped = metric.get("job_scoped") or {}
    if key in job_scoped and job_scoped[key] is not None:
        return job_scoped[key]
    if legacy_key and legacy_key in metric:
        return metric.get(legacy_key)
    return metric.get(key, "-")


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: inspect_young_campaign_health.py <campaign_root>", file=sys.stderr)
        return 2

    campaign_root = Path(sys.argv[1]).resolve()
    manifest = load_json(campaign_root / "campaign_manifest.json") or {}
    receipts_dir = campaign_root / "projections" / "receipts"
    states_dir = campaign_root / "projections" / "states"
    metrics_dir = campaign_root / "metrics"
    health_dir = campaign_root / "health"
    shards_dir = campaign_root / "shards"

    receipt_paths = sorted(receipts_dir.glob("*.json"))
    state_paths = sorted(states_dir.glob("*.json"))
    metric_paths = sorted(metrics_dir.glob("*.json"))
    shard_paths = sorted(shards_dir.glob("*.json"))
    terminal_paths = sorted(health_dir.glob("*.terminal_summary.json"))

    print("PATINA Young Campaign Health")
    print(f"campaign_root: {campaign_root}")
    if manifest:
        print(f"created_at:    {manifest.get('created_at', '-')}")
        print(f"site_name:     {manifest.get('site_name', '-')}")
        print(f"shard_count:   {manifest.get('shard_count', '-')}")
        print(f"workers/shard: {manifest.get('workers_per_shard', '-')}")
    print(f"receipts:      {len(receipt_paths)}")
    print(f"states:        {len(state_paths)}")
    print(f"metrics:       {len(metric_paths)}")
    print(f"shards:        {len(shard_paths)}")
    print(f"terminal sums: {len(terminal_paths)}")
    print()

    rows = []
    for shard_path in shard_paths:
        shard = load_json(shard_path) or {}
        shard_id = shard.get("shard_id", shard_path.stem)
        receipt = None
        state = None
        metric = load_json(metrics_dir / f"{shard_id}.json") or {}
        heartbeat = load_json(health_dir / f"{shard_id}.heartbeat.json") or {}
        terminal = load_json(health_dir / f"{shard_id}.terminal_summary.json") or {}

        for path in receipt_paths:
            data = load_json(path)
            if data and data.get("job_id") == f"campaign-shard:{shard_id}":
                receipt = data
                break

        for path in state_paths:
            data = load_json(path)
            if data and data.get("job_id") == f"campaign-shard:{shard_id}":
                state = data
                break

        rows.append(
            {
                "shard_id": shard_id,
                "allocation": (receipt or {}).get("scheduler_job", {}).get("allocation_id", "-"),
                "receipt": (receipt or {}).get("state", "-"),
                "state": (state or {}).get("status", "-"),
                "host": metric.get("host", "-"),
                "nslots": metric.get("nslots", "-"),
                "proc_count": metric_value(metric, "process_count"),
                "rss_total_mb": metric_value(metric, "rss_total_mb"),
                "rss_peak_mb": metric_value(metric, "rss_peak_mb", "rss_max_mb"),
                "cpu_total": metric_value(metric, "cpu_percent_total"),
                "gpu_util": metric_value(metric, "gpu_util_percent", "gpu_util_percent"),
                "heartbeat": heartbeat.get("phase", "-"),
                "terminal": terminal.get("final_phase", "-"),
            }
        )

    if not rows:
        print("No shard records were found.")
        return 0

    headers = [
        ("shard_id", 10),
        ("allocation", 10),
        ("receipt", 11),
        ("state", 16),
        ("host", 18),
        ("nslots", 6),
        ("proc_count", 10),
        ("rss_total_mb", 12),
        ("rss_peak_mb", 11),
        ("cpu_total", 9),
        ("gpu_util", 8),
        ("heartbeat", 16),
        ("terminal", 10),
    ]

    print(
        " ".join(name.ljust(width) for name, width in headers)
    )
    print(
        " ".join("-" * width for _, width in headers)
    )
    for row in rows:
        print(
            " ".join(
                as_str(row[name]).ljust(width)[:width] for name, width in headers
            )
        )

    print()
    completed = sum(1 for row in rows if row["terminal"] == "completed")
    failed = sum(1 for row in rows if row["terminal"] == "failed")
    running = sum(1 for row in rows if row["state"] == "running_external")
    print(f"completed_shards: {completed}")
    print(f"failed_shards:    {failed}")
    print(f"running_shards:   {running}")
    print()
    print("This is a file-backed health inspector and TUI precursor, not the final hot-path coordinator.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
