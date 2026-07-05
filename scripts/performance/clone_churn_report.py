#!/usr/bin/env python3
"""Summarize clone, COW, and reference churn from request profiles."""

from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path
from typing import Any

WORDPRESS_SCRIPT_DIR = Path(__file__).resolve().parents[1] / "wordpress"
sys.path.insert(0, str(WORDPRESS_SCRIPT_DIR))

from common import REPO_ROOT, json_dump, repo_path  # noqa: E402

DEFAULT_OUT_DIR = REPO_ROOT / "target" / "performance" / "clone-churn"
DEFAULT_PROFILE_GLOBS = (
    "target/performance/wordpress-root/*/request-profiles/*.json",
    "target/performance/wordpress-root-profile/*/*.json",
    "target/performance/server/request-profile/*.json",
)
TOTAL_FIELDS = (
    "value_clones",
    "array_handle_clones",
    "cow_separations",
    "reference_cell_creations",
    "by_ref_arg_cow_separations",
    "by_ref_arg_cow_separations_avoided",
)
SOURCE_LISTS = (
    "value_clone_by_source_family",
    "array_handle_clone_by_source_family",
    "cow_separation_by_source_family",
    "reference_cell_creation_by_source_family",
    "by_ref_arg_fallback_by_reason",
)


def main() -> int:
    args = parse_args()
    if args.self_test:
        return self_test()
    out_dir = output_dir(args)
    report = run(args, out_dir)
    json_dump(report, out_dir / "wordpress-root.json")
    write_markdown(report, out_dir / "wordpress-root.md")
    print(f"[{report['status']}] clone churn report wrote {rel(out_dir / 'wordpress-root.md')}")
    return 0 if report["status"] in {"pass", "skip"} else 1


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", default=os.environ.get("PHRUST_REQUEST_PROFILE_JSON", ""))
    parser.add_argument("--profile-dir", default=os.environ.get("PHRUST_REQUEST_PROFILE_DIR", ""))
    parser.add_argument("--baseline", default=os.environ.get("PHRUST_CLONE_CHURN_BASELINE", ""))
    parser.add_argument("--out-dir", default=os.environ.get("PHRUST_CLONE_CHURN_OUT", ""))
    parser.add_argument(
        "--limit",
        type=int,
        default=int(os.environ.get("PHRUST_CLONE_CHURN_LIMIT", "25")),
    )
    parser.add_argument("--self-test", action="store_true")
    return parser.parse_args()


def output_dir(args: argparse.Namespace) -> Path:
    if args.out_dir:
        return repo_path(args.out_dir) or Path(args.out_dir).expanduser()
    return DEFAULT_OUT_DIR


def run(args: argparse.Namespace, out_dir: Path) -> dict[str, Any]:
    profile_path = resolve_profile(args.profile, args.profile_dir)
    if profile_path is None:
        return {
            "status": "skip",
            "reason": "missing_request_profile",
            "searched": list(DEFAULT_PROFILE_GLOBS),
            "artifacts": artifacts(out_dir),
        }
    profile = json.loads(profile_path.read_text(encoding="utf-8"))
    limit = max(args.limit, 1)
    summary = summarize_profile(profile, limit)
    report: dict[str, Any] = {
        "status": "pass",
        "profile": rel(profile_path),
        "request": profile.get("request", {}),
        "summary": summary,
        "artifacts": artifacts(out_dir),
    }
    baseline_path = resolve_baseline(args.baseline)
    if baseline_path is not None:
        baseline = json.loads(baseline_path.read_text(encoding="utf-8"))
        report["baseline"] = rel(baseline_path)
        report["delta"] = compare_summaries(summarize_profile(baseline, limit), summary)
    elif args.baseline:
        report["baseline"] = args.baseline
        report["baseline_warning"] = "baseline_profile_not_found"
    return report


def resolve_profile(profile_arg: str, profile_dir_arg: str) -> Path | None:
    if profile_arg:
        profile = repo_path(profile_arg) or Path(profile_arg).expanduser()
        return profile if profile.is_file() else None
    if profile_dir_arg:
        profile_dir = repo_path(profile_dir_arg) or Path(profile_dir_arg).expanduser()
        return latest_profile_in_dir(profile_dir)
    candidates: list[Path] = []
    for pattern in DEFAULT_PROFILE_GLOBS:
        candidates.extend(path for path in REPO_ROOT.glob(pattern) if is_request_profile(path))
    if not candidates:
        return None
    return max(candidates, key=lambda path: path.stat().st_mtime)


def resolve_baseline(baseline_arg: str) -> Path | None:
    if not baseline_arg:
        return None
    baseline = repo_path(baseline_arg) or Path(baseline_arg).expanduser()
    return baseline if baseline.is_file() else None


def latest_profile_in_dir(profile_dir: Path | None) -> Path | None:
    if profile_dir is None or not profile_dir.is_dir():
        return None
    candidates = [path for path in profile_dir.glob("*.json") if is_request_profile(path)]
    if not candidates:
        return None
    return max(candidates, key=lambda path: path.stat().st_mtime)


def is_request_profile(path: Path) -> bool:
    if not path.is_file() or path.name in {"summary.json", "wordpress-root.json"}:
        return False
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return False
    return isinstance(data, dict) and isinstance(data.get("attribution"), dict)


def summarize_profile(profile: dict[str, Any], limit: int) -> dict[str, Any]:
    attribution = as_dict(profile.get("attribution"))
    clones = as_dict(attribution.get("clones"))
    calls = as_dict(attribution.get("calls"))
    arrays = as_dict(attribution.get("arrays"))
    totals = {field: int_value(clones.get(field)) for field in TOTAL_FIELDS}
    sources = {
        name: list_entries(clones.get(name))[:limit]
        for name in SOURCE_LISTS
    }
    return {
        "totals": totals,
        "source_families": sources,
        "hot_clone_sources": ranked_sources(sources, limit),
        "call_clone_context": {
            "function_calls": int_value(calls.get("function_calls")),
            "method_calls": int_value(calls.get("method_calls")),
            "internal_function_dispatches": int_value(calls.get("internal_function_dispatches")),
            "frame_allocations": int_value(calls.get("frame_allocations")),
            "frame_reuses": int_value(calls.get("frame_reuses")),
            "argument_vector_allocations_avoided": int_value(
                calls.get("argument_vector_allocations_avoided")
            ),
        },
        "array_clone_context": {
            "array_dim_fetches": int_value(arrays.get("array_dim_fetches")),
            "array_linear_scan_fallbacks": int_value(arrays.get("array_linear_scan_fallbacks")),
            "array_metadata_recomputes": int_value(arrays.get("array_metadata_recomputes")),
            "array_fast_path_fallback_by_reason": list_entries(
                arrays.get("array_fast_path_fallback_by_reason")
            )[:limit],
            "foreach_clone_required_by_reason": list_entries(
                arrays.get("foreach_clone_required_by_reason")
            )[:limit],
        },
    }


def ranked_sources(sources: dict[str, list[dict[str, Any]]], limit: int) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    for group, entries in sources.items():
        for entry in entries:
            rows.append({
                "group": group,
                "name": str(entry.get("name", "")),
                "value": entry_metric(entry),
            })
    return sorted(rows, key=lambda row: row["value"], reverse=True)[:limit]


def compare_summaries(baseline: dict[str, Any], current: dict[str, Any]) -> dict[str, Any]:
    baseline_totals = as_dict(baseline.get("totals"))
    current_totals = as_dict(current.get("totals"))
    totals = {}
    for field in TOTAL_FIELDS:
        before = int_value(baseline_totals.get(field))
        after = int_value(current_totals.get(field))
        totals[field] = {
            "baseline": before,
            "current": after,
            "delta": after - before,
            "ratio": ratio(after, before),
        }
    return {"totals": totals}


def ratio(current: int, baseline: int) -> float | None:
    if baseline == 0:
        return None
    return current / baseline


def list_entries(value: Any) -> list[dict[str, Any]]:
    if not isinstance(value, list):
        return []
    return [entry for entry in value if isinstance(entry, dict)]


def int_value(value: Any) -> int:
    if isinstance(value, bool):
        return int(value)
    if isinstance(value, int):
        return value
    if isinstance(value, float):
        return int(value)
    return 0


def entry_metric(entry: dict[str, Any]) -> int:
    value = int_value(entry.get("value"))
    if value:
        return value
    return int_value(entry.get("count"))


def as_dict(value: Any) -> dict[str, Any]:
    return value if isinstance(value, dict) else {}


def artifacts(out_dir: Path) -> dict[str, str]:
    return {
        "json": rel(out_dir / "wordpress-root.json"),
        "markdown": rel(out_dir / "wordpress-root.md"),
    }


def rel(path: Path) -> str:
    try:
        return str(path.relative_to(REPO_ROOT))
    except ValueError:
        return str(path)


def write_markdown(report: dict[str, Any], path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    lines = [
        "# WordPress Root Clone Churn Report",
        "",
        f"- status: `{report['status']}`",
    ]
    if report["status"] == "skip":
        lines.extend([
            f"- reason: `{report.get('reason', 'unknown')}`",
            "",
        ])
        path.write_text("\n".join(lines), encoding="utf-8")
        return
    summary = as_dict(report.get("summary"))
    totals = as_dict(summary.get("totals"))
    lines.extend([
        f"- profile: `{report.get('profile', '')}`",
        f"- value clones: `{totals.get('value_clones', 0)}`",
        f"- array-handle clones: `{totals.get('array_handle_clones', 0)}`",
        f"- COW separations: `{totals.get('cow_separations', 0)}`",
        f"- reference-cell creations: `{totals.get('reference_cell_creations', 0)}`",
        f"- by-ref COW separations: `{totals.get('by_ref_arg_cow_separations', 0)}`",
        f"- by-ref COW separations avoided: `{totals.get('by_ref_arg_cow_separations_avoided', 0)}`",
        "",
    ])
    if "delta" in report:
        lines.extend(delta_table(as_dict(report.get("delta")).get("totals")))
    lines.extend(table_for_entries(
        "Top Clone/COW/Reference Sources",
        list_entries(summary.get("hot_clone_sources")),
        headers=["group", "name", "value"],
    ))
    sources = as_dict(summary.get("source_families"))
    for title, key in [
        ("Value Clone Sources", "value_clone_by_source_family"),
        ("Array Handle Clone Sources", "array_handle_clone_by_source_family"),
        ("COW Separation Sources", "cow_separation_by_source_family"),
        ("Reference Cell Creation Sources", "reference_cell_creation_by_source_family"),
        ("By-Ref Argument Fallbacks", "by_ref_arg_fallback_by_reason"),
    ]:
        lines.extend(table_for_entries(title, list_entries(sources.get(key))))
    lines.extend(context_section("Call Clone Context", as_dict(summary.get("call_clone_context"))))
    lines.extend(context_section("Array Clone Context", as_dict(summary.get("array_clone_context"))))
    path.write_text("\n".join(lines), encoding="utf-8")


def delta_table(value: Any) -> list[str]:
    totals = as_dict(value)
    lines = ["## Baseline Delta", ""]
    if not totals:
        lines.extend(["No baseline delta available.", ""])
        return lines
    lines.append("| metric | baseline | current | delta | ratio |")
    lines.append("| --- | --- | --- | --- | --- |")
    for field in TOTAL_FIELDS:
        row = as_dict(totals.get(field))
        ratio_value = row.get("ratio")
        ratio_text = "" if ratio_value is None else f"{ratio_value:.4f}"
        lines.append(
            f"| {field} | {row.get('baseline', 0)} | {row.get('current', 0)} | "
            f"{row.get('delta', 0)} | {ratio_text} |"
        )
    lines.append("")
    return lines


def table_for_entries(
    title: str,
    entries: list[dict[str, Any]],
    headers: list[str] | None = None,
) -> list[str]:
    lines = [f"## {title}", ""]
    if not entries:
        lines.extend(["No entries.", ""])
        return lines
    if headers is None:
        all_keys = set().union(*(entry.keys() for entry in entries))
        numeric_column = "value" if "value" in all_keys else "count" if "count" in all_keys else ""
        headers = ["name"]
        if numeric_column:
            headers.append(numeric_column)
    lines.append("| " + " | ".join(headers) + " |")
    lines.append("| " + " | ".join(["---"] * len(headers)) + " |")
    for entry in entries:
        row = [str(entry.get(header, "")) for header in headers]
        lines.append("| " + " | ".join(row) + " |")
    lines.append("")
    return lines


def context_section(title: str, values: dict[str, Any]) -> list[str]:
    lines = [f"## {title}", ""]
    if not values:
        lines.extend(["No entries.", ""])
        return lines
    for key, value in values.items():
        if isinstance(value, list):
            continue
        lines.append(f"- `{key}`: `{value}`")
    lines.append("")
    for key, value in values.items():
        if isinstance(value, list):
            lines.extend(table_for_entries(key, list_entries(value)))
    return lines


def self_test() -> int:
    baseline = {
        "request": {"path": "/"},
        "attribution": {
            "clones": {
                "value_clones": 100,
                "array_handle_clones": 40,
                "cow_separations": 8,
                "reference_cell_creations": 6,
                "by_ref_arg_cow_separations": 4,
                "by_ref_arg_cow_separations_avoided": 1,
                "value_clone_by_source_family": [
                    {"name": "call_argument_snapshot", "value": 70}
                ],
                "array_handle_clone_by_source_family": [
                    {"name": "array_element_read", "value": 30}
                ],
                "cow_separation_by_source_family": [
                    {"name": "array_write", "value": 5}
                ],
                "reference_cell_creation_by_source_family": [
                    {"name": "by_ref_argument_binding", "value": 4}
                ],
                "by_ref_arg_fallback_by_reason": [
                    {"name": "temporary", "value": 2}
                ],
            },
            "calls": {"function_calls": 3, "frame_allocations": 2},
            "arrays": {"array_dim_fetches": 9},
        },
    }
    current = {
        "request": {"path": "/"},
        "attribution": {
            "clones": {
                "value_clones": 50,
                "array_handle_clones": 25,
                "cow_separations": 8,
                "reference_cell_creations": 3,
                "by_ref_arg_cow_separations": 2,
                "by_ref_arg_cow_separations_avoided": 5,
                "value_clone_by_source_family": [
                    {"name": "call_argument_snapshot", "count": 25}
                ],
                "array_handle_clone_by_source_family": [
                    {"name": "array_element_read", "value": 20}
                ],
                "cow_separation_by_source_family": [
                    {"name": "array_write", "value": 8}
                ],
                "reference_cell_creation_by_source_family": [
                    {"name": "by_ref_argument_binding", "value": 3}
                ],
                "by_ref_arg_fallback_by_reason": [
                    {"name": "temporary", "value": 1}
                ],
            },
            "calls": {
                "function_calls": 3,
                "method_calls": 2,
                "internal_function_dispatches": 4,
                "frame_allocations": 2,
                "argument_vector_allocations_avoided": 7,
            },
            "arrays": {
                "array_dim_fetches": 9,
                "array_linear_scan_fallbacks": 1,
                "array_fast_path_fallback_by_reason": [
                    {"name": "numeric_string_key", "value": 2}
                ],
            },
        },
    }
    before = summarize_profile(baseline, 10)
    after = summarize_profile(current, 10)
    delta = compare_summaries(before, after)
    assert after["totals"]["value_clones"] == 50
    assert after["hot_clone_sources"][0]["group"] == "value_clone_by_source_family"
    assert after["call_clone_context"]["argument_vector_allocations_avoided"] == 7
    assert after["array_clone_context"]["array_linear_scan_fallbacks"] == 1
    assert delta["totals"]["value_clones"]["delta"] == -50
    assert delta["totals"]["array_handle_clones"]["ratio"] == 0.625
    print("[pass] clone_churn_report self-test")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
