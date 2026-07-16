#!/usr/bin/env python3
"""Render deterministic helper/ownership attribution from VM counters JSON."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", type=Path)
    parser.add_argument(
        "--out-dir", type=Path, default=Path("target/post-cutover/ssa-lifetimes")
    )
    parser.add_argument("--label", default="baseline")
    parser.add_argument("--self-test", action="store_true")
    return parser.parse_args()


def integer_map(document: dict[str, Any], key: str) -> dict[str, int]:
    value = document.get(key, {})
    if not isinstance(value, dict):
        raise ValueError(f"{key} must be a JSON object")
    return {str(name): int(count) for name, count in sorted(value.items())}


def profile(document: dict[str, Any]) -> dict[str, Any]:
    calls = integer_map(document, "runtime_helper_calls_by_id")
    times = integer_map(document, "runtime_helper_time_nanos_by_id")
    helpers = [
        {
            "helper_id": name,
            "calls": count,
            "exclusive_time_nanos": times.get(name, 0),
        }
        for name, count in sorted(calls.items(), key=lambda item: (-item[1], item[0]))
    ]
    total_calls = int(document.get("runtime_helper_calls", sum(calls.values())))
    total_time = int(document.get("runtime_helper_time_nanos", sum(times.values())))
    execution_time = int(document.get("native_execution_time_nanos", 0))
    targets = {
        "runtime_helper_calls": {"actual": total_calls, "maximum": 1_750_000},
        "local_read_calls": {"actual": calls.get("local_fetch", 0), "maximum": 150_000},
        "local_store_calls": {"actual": calls.get("local_store", 0), "maximum": 75_000},
        "truthy_calls": {"actual": calls.get("truthy", 0), "maximum": 75_000},
        "retain_release_calls": {
            "actual": calls.get("value_retain", 0) + calls.get("value_release", 0),
            "maximum": 250_000,
        },
        "object_root_scans": {
            "actual": int(document.get("runtime_helper_object_release_root_scans", 0)),
            "maximum": 500,
        },
    }
    for target in targets.values():
        target["passes"] = target["actual"] <= target["maximum"]
    helper_share = None if execution_time == 0 else total_time / execution_time
    return {
        "schema_version": 1,
        "runtime_helper_calls": total_calls,
        "runtime_helper_time_nanos": total_time,
        "native_execution_time_nanos": execution_time,
        "helper_exclusive_share": helper_share,
        "helper_exclusive_share_passes": None if helper_share is None else helper_share <= 0.55,
        "targets": targets,
        "helpers": helpers,
        "ir_operations": {
            "calls": integer_map(document, "runtime_helper_calls_by_ir_operation"),
            "exclusive_time_nanos": integer_map(
                document, "runtime_helper_time_nanos_by_ir_operation"
            ),
        },
        "functions": {
            "calls": integer_map(document, "runtime_helper_calls_by_function"),
            "exclusive_time_nanos": integer_map(
                document, "runtime_helper_time_nanos_by_function"
            ),
        },
        "local_reads": integer_map(document, "runtime_helper_local_read_by_reason"),
        "local_stores": integer_map(document, "runtime_helper_local_store_by_reason"),
        "truthiness": integer_map(document, "runtime_helper_truthy_by_value_class"),
        "retains": integer_map(document, "runtime_helper_retain_by_reason"),
        "releases": integer_map(document, "runtime_helper_release_by_reason"),
        "release_to_zero": int(document.get("runtime_helper_release_to_zero", 0)),
        "root_scans": int(document.get("runtime_helper_object_release_root_scans", 0)),
        "root_scans_by_reason": integer_map(
            document, "runtime_helper_object_release_root_scans_by_reason"
        ),
        "value_table": {
            "allocations": int(document.get("native_value_table_allocations", 0)),
            "reuses": int(document.get("native_value_table_reuses", 0)),
            "high_water": int(document.get("native_value_table_high_water", 0)),
        },
        "ssa": {
            "promoted_locals": int(document.get("native_ssa_promoted_locals", 0)),
            "promoted_registers": int(document.get("native_ssa_promoted_registers", 0)),
        },
        "ownership": {
            "moves": int(document.get("native_ownership_moves", 0)),
            "clones": int(document.get("native_ownership_clones", 0)),
            "escapes": int(document.get("native_ownership_escapes", 0)),
        },
        "slow_paths": integer_map(document, "native_slow_path_entries_by_reason"),
    }


def markdown(report: dict[str, Any]) -> str:
    lines = [
        "# Native helper and ownership profile",
        "",
        f"Total helper calls: {report['runtime_helper_calls']}",
        f"Exclusive helper time (ns): {report['runtime_helper_time_nanos']}",
        f"Release-to-zero count: {report['release_to_zero']}",
        f"Root index rebuilds: {report['root_scans']}",
        f"Helper exclusive share: {report['helper_exclusive_share']}",
        "",
        "| Helper | Calls | Exclusive ns |",
        "|---|---:|---:|",
    ]
    for helper in report["helpers"]:
        lines.append(
            f"| `{helper['helper_id']}` | {helper['calls']} | "
            f"{helper['exclusive_time_nanos']} |"
        )
    lines.extend(["", "## Tranche thresholds", "", "| Metric | Actual | Maximum | Pass |", "|---|---:|---:|:---:|"])
    for name, target in report["targets"].items():
        lines.append(
            f"| `{name}` | {target['actual']} | {target['maximum']} | "
            f"{'yes' if target['passes'] else 'no'} |"
        )
    for heading, key in (
        ("Local reads", "local_reads"),
        ("Local stores", "local_stores"),
        ("Truthiness", "truthiness"),
        ("Retains", "retains"),
        ("Releases", "releases"),
        ("Root rebuilds", "root_scans_by_reason"),
        ("Slow paths", "slow_paths"),
    ):
        lines.extend(["", f"## {heading}", ""])
        values = report[key]
        if values:
            lines.extend(f"- `{name}`: {count}" for name, count in values.items())
        else:
            lines.append("- none recorded")
    lines.append("")
    return "\n".join(lines)


def self_test() -> int:
    result = profile(
        {
            "runtime_helper_calls_by_id": {"truthy": 2, "local_fetch": 3},
            "runtime_helper_time_nanos_by_id": {"truthy": 7},
            "runtime_helper_release_to_zero": 1,
        }
    )
    assert result["runtime_helper_calls"] == 5
    assert result["helpers"][0]["helper_id"] == "local_fetch"
    assert "`local_fetch` | 3" in markdown(result)
    print("[pass] native helper report self-test")
    return 0


def main() -> int:
    args = parse_args()
    if args.self_test:
        return self_test()
    if args.input is None or not args.input.is_file():
        print("--input must name an existing counters JSON file", file=sys.stderr)
        return 2
    document = json.loads(args.input.read_text(encoding="utf-8"))
    report = profile(document)
    args.out_dir.mkdir(parents=True, exist_ok=True)
    json_path = args.out_dir / f"{args.label}-helper-profile.json"
    markdown_path = args.out_dir / f"{args.label}-helper-profile.md"
    json_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    markdown_path.write_text(markdown(report), encoding="utf-8")
    print(json_path)
    print(markdown_path)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
