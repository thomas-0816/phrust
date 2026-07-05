#!/usr/bin/env python3
"""Summarize array hot-path attribution from a request profile."""

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

DEFAULT_OUT_DIR = REPO_ROOT / "target" / "performance" / "array-hotpaths"
DEFAULT_PROFILE_GLOBS = (
    "target/performance/wordpress-root/*/request-profiles/*.json",
    "target/performance/wordpress-root-profile/*/*.json",
    "target/performance/server/request-profile/*.json",
)


def main() -> int:
    args = parse_args()
    if args.self_test:
        return self_test()
    out_dir = output_dir(args)
    report = run(args, out_dir)
    json_dump(report, out_dir / "wordpress-root.json")
    write_markdown(report, out_dir / "wordpress-root.md")
    print(f"[{report['status']}] array hotpath report wrote {rel(out_dir / 'wordpress-root.md')}")
    return 0 if report["status"] in {"pass", "skip"} else 1


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", default=os.environ.get("PHRUST_REQUEST_PROFILE_JSON", ""))
    parser.add_argument("--profile-dir", default=os.environ.get("PHRUST_REQUEST_PROFILE_DIR", ""))
    parser.add_argument("--out-dir", default=os.environ.get("PHRUST_ARRAY_HOTPATH_OUT", ""))
    parser.add_argument("--limit", type=int, default=int(os.environ.get("PHRUST_ARRAY_HOTPATH_LIMIT", "25")))
    parser.add_argument("--self-test", action="store_true")
    return parser.parse_args()


def output_dir(args: argparse.Namespace) -> Path:
    if args.out_dir:
        return repo_path(args.out_dir) or Path(args.out_dir).expanduser()
    return DEFAULT_OUT_DIR


def run(args: argparse.Namespace, out_dir: Path) -> dict[str, Any]:
    profile_path = resolve_profile(args)
    if profile_path is None:
        return {
            "status": "skip",
            "reason": "missing_request_profile",
            "searched": list(DEFAULT_PROFILE_GLOBS),
            "artifacts": artifacts(out_dir),
        }
    profile = json.loads(profile_path.read_text(encoding="utf-8"))
    return {
        "status": "pass",
        "profile": rel(profile_path),
        "request": profile.get("request", {}),
        "summary": summarize_profile(profile, max(args.limit, 1)),
        "artifacts": artifacts(out_dir),
    }


def resolve_profile(args: argparse.Namespace) -> Path | None:
    if args.profile:
        profile = repo_path(args.profile) or Path(args.profile).expanduser()
        return profile if profile.is_file() else None
    if args.profile_dir:
        profile_dir = repo_path(args.profile_dir) or Path(args.profile_dir).expanduser()
        return latest_profile_in_dir(profile_dir)
    candidates: list[Path] = []
    for pattern in DEFAULT_PROFILE_GLOBS:
        candidates.extend(path for path in REPO_ROOT.glob(pattern) if is_request_profile(path))
    if not candidates:
        return None
    return max(candidates, key=lambda path: path.stat().st_mtime)


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
    arrays = as_dict(attribution.get("arrays"))
    clones = as_dict(attribution.get("clones"))
    calls = as_dict(attribution.get("calls"))

    operation_profiles = sort_profile_entries(
        list_entries(arrays.get("operation_profiles_by_family")),
        "inclusive_nanos",
    )
    fast_hits = list_entries(arrays.get("array_fast_path_hits_by_family"))
    fallbacks = list_entries(arrays.get("array_fast_path_fallback_by_reason"))
    shapes = list_entries(arrays.get("array_shape_observed_by_kind"))
    packed_to_mixed = list_entries(arrays.get("packed_to_mixed_by_reason"))
    record_to_mixed = list_entries(arrays.get("record_to_mixed_by_reason"))
    foreach_clones = list_entries(arrays.get("foreach_clone_required_by_reason"))
    array_clone_sources = list_entries(clones.get("array_handle_clone_by_source_family"))
    value_clone_sources = list_entries(clones.get("value_clone_by_source_family"))
    builtin_profiles = sort_profile_entries(
        list_entries(calls.get("builtin_profiles_by_name")),
        "inclusive_nanos",
    )

    total_operation_count = sum_int_field(operation_profiles, "count")
    total_fast_hits = sum_entry_values(fast_hits)
    total_fallbacks = sum_entry_values(fallbacks)

    return {
        "array_activity": {
            "array_dim_fetches": int_value(arrays.get("array_dim_fetches")),
            "packed_dim_fast_path_hits": int_value(arrays.get("packed_dim_fast_path_hits")),
            "packed_dim_fast_path_misses": int_value(arrays.get("packed_dim_fast_path_misses")),
            "array_packed_append_fast_path_hits": int_value(
                arrays.get("array_packed_append_fast_path_hits")
            ),
            "array_packed_read_fast_path_hits": int_value(
                arrays.get("array_packed_read_fast_path_hits")
            ),
            "array_sequential_foreach_fast_path_hits": int_value(
                arrays.get("array_sequential_foreach_fast_path_hits")
            ),
            "array_count_fast_path_hits": int_value(arrays.get("array_count_fast_path_hits")),
            "array_packed_direct_gets": int_value(arrays.get("array_packed_direct_gets")),
            "array_mixed_indexed_gets": int_value(arrays.get("array_mixed_indexed_gets")),
            "array_linear_scan_fallbacks": int_value(arrays.get("array_linear_scan_fallbacks")),
            "array_metadata_recomputes": int_value(arrays.get("array_metadata_recomputes")),
            "array_packed_to_mixed_transitions": int_value(
                arrays.get("array_packed_to_mixed_transitions")
            ),
            "total_operation_count": total_operation_count,
            "total_fast_path_hits": total_fast_hits,
            "total_fast_path_fallbacks": total_fallbacks,
        },
        "numeric_string_keys": {
            "classify_calls": int_value(arrays.get("numeric_string_classify_calls")),
            "cache_hits": int_value(arrays.get("numeric_string_cache_hits")),
            "cache_misses": int_value(arrays.get("numeric_string_cache_misses")),
        },
        "top_arrays": {
            "operation_profiles_by_family": operation_profiles[:limit],
            "array_fast_path_hits_by_family": fast_hits[:limit],
            "array_fast_path_fallback_by_reason": fallbacks[:limit],
            "array_shape_observed_by_kind": shapes[:limit],
            "packed_to_mixed_by_reason": packed_to_mixed[:limit],
            "record_to_mixed_by_reason": record_to_mixed[:limit],
            "foreach_clone_required_by_reason": foreach_clones[:limit],
        },
        "clone_pressure": {
            "array_handle_clones": int_value(clones.get("array_handle_clones")),
            "value_clones": int_value(clones.get("value_clones")),
            "array_handle_clone_by_source_family": array_clone_sources[:limit],
            "value_clone_by_source_family": value_clone_sources[:limit],
        },
        "array_related_builtins": [
            entry
            for entry in builtin_profiles
            if str(entry.get("name", "")).startswith(("array_", "count", "isset", "empty"))
        ][:limit],
    }


def list_entries(value: Any) -> list[dict[str, Any]]:
    if not isinstance(value, list):
        return []
    return [entry for entry in value if isinstance(entry, dict)]


def sort_profile_entries(entries: list[dict[str, Any]], field: str) -> list[dict[str, Any]]:
    return sorted(entries, key=lambda entry: int_value(entry.get(field)), reverse=True)


def sum_entry_values(entries: list[dict[str, Any]]) -> int:
    return sum(int_value(entry.get("value")) for entry in entries)


def sum_int_field(entries: list[dict[str, Any]], field: str) -> int:
    return sum(int_value(entry.get(field)) for entry in entries)


def int_value(value: Any) -> int:
    if isinstance(value, bool):
        return int(value)
    if isinstance(value, int):
        return value
    if isinstance(value, float):
        return int(value)
    return 0


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
        "# WordPress Root Array Hotpath Report",
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
    activity = as_dict(summary.get("array_activity"))
    numeric = as_dict(summary.get("numeric_string_keys"))
    clones = as_dict(summary.get("clone_pressure"))
    lines.extend([
        f"- profile: `{report.get('profile', '')}`",
        f"- array operation count: `{activity.get('total_operation_count', 0)}`",
        f"- fast-path hits: `{activity.get('total_fast_path_hits', 0)}`",
        f"- fast-path fallbacks: `{activity.get('total_fast_path_fallbacks', 0)}`",
        f"- packed direct gets: `{activity.get('array_packed_direct_gets', 0)}`",
        f"- mixed indexed gets: `{activity.get('array_mixed_indexed_gets', 0)}`",
        f"- linear scan fallbacks: `{activity.get('array_linear_scan_fallbacks', 0)}`",
        f"- metadata recomputes: `{activity.get('array_metadata_recomputes', 0)}`",
        f"- array-handle clones: `{clones.get('array_handle_clones', 0)}`",
        f"- value clones: `{clones.get('value_clones', 0)}`",
        f"- numeric-string classify calls: `{numeric.get('classify_calls', 0)}`",
        f"- numeric-string cache hits: `{numeric.get('cache_hits', 0)}`",
        f"- numeric-string cache misses: `{numeric.get('cache_misses', 0)}`",
        "",
    ])
    top = as_dict(summary.get("top_arrays"))
    for title, key in [
        ("Array Operation Families", "operation_profiles_by_family"),
        ("Fast-Path Hits", "array_fast_path_hits_by_family"),
        ("Fast-Path Fallbacks", "array_fast_path_fallback_by_reason"),
        ("Observed Shapes", "array_shape_observed_by_kind"),
        ("Packed-To-Mixed Reasons", "packed_to_mixed_by_reason"),
        ("Record-To-Mixed Reasons", "record_to_mixed_by_reason"),
        ("Foreach Clone Reasons", "foreach_clone_required_by_reason"),
    ]:
        lines.extend(table_for_entries(title, list_entries(top.get(key))))

    lines.extend(table_for_entries(
        "Array Handle Clone Sources",
        list_entries(clones.get("array_handle_clone_by_source_family")),
    ))
    lines.extend(table_for_entries(
        "Value Clone Sources",
        list_entries(clones.get("value_clone_by_source_family")),
    ))
    lines.extend(table_for_entries(
        "Array-Related Builtins",
        list_entries(summary.get("array_related_builtins")),
    ))
    path.write_text("\n".join(lines), encoding="utf-8")


def table_for_entries(title: str, entries: list[dict[str, Any]]) -> list[str]:
    lines = [f"## {title}", ""]
    if not entries:
        lines.extend(["No entries.", ""])
        return lines
    all_keys = set().union(*(entry.keys() for entry in entries))
    numeric_column = "value" if "value" in all_keys else "count" if "count" in all_keys else ""
    extra_column = "inclusive_nanos" if "inclusive_nanos" in all_keys else ""
    headers = ["name"]
    if numeric_column:
        headers.append(numeric_column)
    if extra_column and extra_column not in headers:
        headers.append(extra_column)
    lines.append("| " + " | ".join(headers) + " |")
    lines.append("| " + " | ".join(["---"] * len(headers)) + " |")
    for entry in entries:
        row = [str(entry.get(header, "")) for header in headers]
        lines.append("| " + " | ".join(row) + " |")
    lines.append("")
    return lines


def self_test() -> int:
    profile = {
        "request": {"path": "/"},
        "attribution": {
            "arrays": {
                "array_dim_fetches": 7,
                "packed_dim_fast_path_hits": 3,
                "packed_dim_fast_path_misses": 2,
                "array_packed_append_fast_path_hits": 5,
                "array_packed_read_fast_path_hits": 4,
                "array_sequential_foreach_fast_path_hits": 1,
                "array_count_fast_path_hits": 6,
                "array_packed_direct_gets": 12,
                "array_mixed_indexed_gets": 4,
                "array_linear_scan_fallbacks": 1,
                "array_metadata_recomputes": 2,
                "array_packed_to_mixed_transitions": 2,
                "numeric_string_classify_calls": 10,
                "numeric_string_cache_hits": 8,
                "numeric_string_cache_misses": 2,
                "operation_profiles_by_family": [
                    {"name": "dim_fetch", "count": 9, "inclusive_nanos": 90},
                    {"name": "append", "count": 3, "inclusive_nanos": 30},
                ],
                "array_fast_path_hits_by_family": [
                    {"name": "packed_append", "value": 5},
                    {"name": "record_shape_fetch", "value": 4},
                ],
                "array_fast_path_fallback_by_reason": [
                    {"name": "numeric_string_key", "value": 3}
                ],
                "array_shape_observed_by_kind": [
                    {"name": "packed_list", "value": 6}
                ],
                "packed_to_mixed_by_reason": [{"name": "string_key", "value": 2}],
                "record_to_mixed_by_reason": [{"name": "int_key", "value": 1}],
                "foreach_clone_required_by_reason": [{"name": "by_ref", "value": 1}],
            },
            "clones": {
                "array_handle_clones": 11,
                "value_clones": 22,
                "array_handle_clone_by_source_family": [
                    {"name": "array_element_read", "value": 7}
                ],
                "value_clone_by_source_family": [
                    {"name": "stack_register_local_move", "value": 4}
                ],
            },
            "calls": {
                "builtin_profiles_by_name": [
                    {"name": "count", "count": 2, "inclusive_nanos": 20},
                    {"name": "strlen", "count": 1, "inclusive_nanos": 10},
                ]
            },
        },
    }
    summary = summarize_profile(profile, 10)
    assert summary["array_activity"]["total_operation_count"] == 12
    assert summary["array_activity"]["total_fast_path_hits"] == 9
    assert summary["array_activity"]["total_fast_path_fallbacks"] == 3
    assert summary["array_activity"]["array_packed_direct_gets"] == 12
    assert summary["array_activity"]["array_linear_scan_fallbacks"] == 1
    assert summary["clone_pressure"]["array_handle_clones"] == 11
    assert summary["array_related_builtins"][0]["name"] == "count"
    print("[pass] array_hotpath_report self-test")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
