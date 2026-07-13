#!/usr/bin/env python3
"""Rank exact non-overlapping request work from schema-v2 request profiles."""

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

DEFAULT_OUT_DIR = REPO_ROOT / "target" / "performance" / "exclusive-work"
DEFAULT_PROFILE_GLOBS = (
    "target/performance/wordpress-root/*/request-profiles/*.json",
    "target/performance/wordpress-root-profile/*/*.json",
    "target/performance/server/request-profile/*.json",
)
WORK_FIELDS = (
    "value_clones",
    "refcounted_value_clones",
    "string_allocations",
    "array_handle_clones",
    "cow_separations",
    "reference_cell_creations",
    "frame_allocations",
    "frame_reuses",
    "register_files_allocated",
    "register_files_reused",
    "internal_function_dispatches",
    "symbol_map_lookups",
    "symbol_linear_fallbacks",
    "symbol_intern_hits",
    "symbol_intern_misses",
    "string_hash_cache_hits",
    "string_hash_cache_misses",
    "symbol_eq_fast_hits",
    "symbol_eq_byte_fallbacks",
    "array_dim_fetches",
    "numeric_string_classify_calls",
    "object_allocations",
    "property_accesses",
    "includes",
    "autoloads",
)
SYMBOL_HASH_FIELDS = WORK_FIELDS[11:19]
ARRAY_NUMERIC_FIELDS = ("array_dim_fetches", "numeric_string_classify_calls")
RANKING_FIELDS = (
    "exclusive_nanos",
    "exclusive_dense_instructions",
    "exclusive_rich_instructions",
    "exclusive_value_clones",
    "exclusive_refcounted_value_clones",
    "exclusive_array_handle_clones",
    "exclusive_symbol_hash_work",
    "exclusive_array_numeric_work",
    "exclusive_work_units_per_call",
)


def main() -> int:
    args = parse_args()
    if args.self_test:
        return self_test()
    out_dir = output_dir(args)
    report = run(args, out_dir)
    json_dump(report, out_dir / "wordpress-root.json")
    write_markdown(report, out_dir / "wordpress-root.md")
    print(
        f"[{report['status']}] exclusive work report wrote {rel(out_dir / 'wordpress-root.md')}"
    )
    return 0 if report["status"] in {"pass", "skip"} else 1


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--profile", default=os.environ.get("PHRUST_REQUEST_PROFILE_JSON", "")
    )
    parser.add_argument(
        "--profile-dir", default=os.environ.get("PHRUST_REQUEST_PROFILE_DIR", "")
    )
    parser.add_argument(
        "--out-dir", default=os.environ.get("PHRUST_EXCLUSIVE_WORK_OUT", "")
    )
    parser.add_argument("--limit", type=int, default=25)
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
    if int_value(profile.get("schema_version")) < 2:
        return {
            "status": "fail",
            "reason": "request_profile_schema_v2_required",
            "profile": rel(profile_path),
            "artifacts": artifacts(out_dir),
        }
    summary = summarize_profile(profile, max(args.limit, 1))
    return {
        "status": "pass",
        "profile": rel(profile_path),
        "request": profile.get("request", {}),
        "summary": summary,
        "artifacts": artifacts(out_dir),
    }


def summarize_profile(profile: dict[str, Any], limit: int) -> dict[str, Any]:
    attribution = as_dict(profile.get("attribution"))
    calls = as_dict(attribution.get("calls"))
    includes = as_dict(attribution.get("includes"))
    boundaries: list[dict[str, Any]] = []
    for kind, entries in (
        ("function", calls.get("function_profiles_by_name")),
        ("method", calls.get("method_profiles_by_name")),
        ("builtin", calls.get("builtin_profiles_by_name")),
        ("include", includes.get("include_profiles_by_path")),
    ):
        boundaries.extend(
            normalize_boundary(kind, entry) for entry in list_entries(entries)
        )
    rankings = {
        field: sorted(boundaries, key=lambda entry: metric(entry, field), reverse=True)[
            :limit
        ]
        for field in RANKING_FIELDS
    }
    totals = as_dict(attribution.get("exclusive_work_totals"))
    accounted = {
        field: sum(
            int_value(as_dict(entry.get("exclusive_work")).get(field))
            for entry in boundaries
        )
        for field in WORK_FIELDS
    }
    unattributed = {
        field: {
            "total": int_value(totals.get(field)),
            "accounted": accounted[field],
            "unattributed": max(int_value(totals.get(field)) - accounted[field], 0),
            "overattributed": max(accounted[field] - int_value(totals.get(field)), 0),
        }
        for field in WORK_FIELDS
    }
    return {
        "boundary_count": len(boundaries),
        "rankings": rankings,
        "unattributed_work": unattributed,
        "operation_timer_accounting": "secondary_overlapping_inclusive",
    }


def normalize_boundary(kind: str, entry: dict[str, Any]) -> dict[str, Any]:
    normalized = dict(entry)
    normalized["kind"] = kind
    work = as_dict(entry.get("exclusive_work"))
    count = max(int_value(entry.get("count")), 1)
    normalized.update(
        {
            "exclusive_value_clones": int_value(work.get("value_clones")),
            "exclusive_refcounted_value_clones": int_value(
                work.get("refcounted_value_clones")
            ),
            "exclusive_array_handle_clones": int_value(work.get("array_handle_clones")),
            "exclusive_symbol_hash_work": sum(
                int_value(work.get(field)) for field in SYMBOL_HASH_FIELDS
            ),
            "exclusive_array_numeric_work": sum(
                int_value(work.get(field)) for field in ARRAY_NUMERIC_FIELDS
            ),
            "exclusive_work_units_per_call": sum(
                int_value(work.get(field)) for field in WORK_FIELDS
            )
            / count,
        }
    )
    return normalized


def metric(entry: dict[str, Any], field: str) -> float:
    value = entry.get(field, 0)
    return float(value) if isinstance(value, (int, float)) else 0.0


def reconcile_fixture(parent: dict[str, Any], children: list[dict[str, Any]]) -> bool:
    for inclusive, exclusive in (
        ("inclusive_nanos", "exclusive_nanos"),
        ("inclusive_rich_instructions", "exclusive_rich_instructions"),
        ("inclusive_dense_instructions", "exclusive_dense_instructions"),
    ):
        if int_value(parent.get(inclusive)) != int_value(parent.get(exclusive)) + sum(
            int_value(child.get(inclusive)) for child in children
        ):
            return False
    parent_inclusive = as_dict(parent.get("inclusive_work"))
    parent_exclusive = as_dict(parent.get("exclusive_work"))
    return all(
        int_value(parent_inclusive.get(field))
        == int_value(parent_exclusive.get(field))
        + sum(
            int_value(as_dict(child.get("inclusive_work")).get(field))
            for child in children
        )
        for field in WORK_FIELDS
    )


def resolve_profile(args: argparse.Namespace) -> Path | None:
    if args.profile:
        path = repo_path(args.profile) or Path(args.profile).expanduser()
        return path if path.is_file() else None
    if args.profile_dir:
        directory = repo_path(args.profile_dir) or Path(args.profile_dir).expanduser()
        candidates = (
            request_profiles(directory.glob("*.json")) if directory.is_dir() else []
        )
    else:
        candidates = []
        for pattern in DEFAULT_PROFILE_GLOBS:
            candidates.extend(request_profiles(REPO_ROOT.glob(pattern)))
    return (
        max(candidates, key=lambda path: path.stat().st_mtime) if candidates else None
    )


def request_profiles(paths: Any) -> list[Path]:
    candidates = []
    for path in paths:
        if path.name in {"summary.json", "wordpress-root.json"} or not path.is_file():
            continue
        try:
            profile = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            continue
        if isinstance(profile, dict) and isinstance(profile.get("attribution"), dict):
            candidates.append(path)
    return candidates


def list_entries(value: Any) -> list[dict[str, Any]]:
    return (
        [entry for entry in value if isinstance(entry, dict)]
        if isinstance(value, list)
        else []
    )


def as_dict(value: Any) -> dict[str, Any]:
    return value if isinstance(value, dict) else {}


def int_value(value: Any) -> int:
    return int(value) if isinstance(value, (int, float)) else 0


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
    lines = ["# WordPress Exclusive Work Report", "", f"- status: `{report['status']}`"]
    if report["status"] != "pass":
        lines.extend([f"- reason: `{report.get('reason', 'unknown')}`", ""])
        path.write_text("\n".join(lines), encoding="utf-8")
        return
    summary = as_dict(report.get("summary"))
    lines.extend(
        [
            f"- profile: `{report.get('profile', '')}`",
            f"- boundaries: `{summary.get('boundary_count', 0)}`",
            "- operation timers: secondary, overlapping, inclusive diagnostics",
            "",
        ]
    )
    rankings = as_dict(summary.get("rankings"))
    for field in RANKING_FIELDS:
        lines.extend(boundary_table(field, list_entries(rankings.get(field))))
    lines.extend(unattributed_table(as_dict(summary.get("unattributed_work"))))
    path.write_text("\n".join(lines), encoding="utf-8")


def boundary_table(field: str, entries: list[dict[str, Any]]) -> list[str]:
    lines = [
        f"## {field}",
        "",
        "| kind | name | count | value |",
        "| --- | --- | --- | --- |",
    ]
    for entry in entries:
        lines.append(
            f"| {entry.get('kind', '')} | {entry.get('name', '')} | "
            f"{entry.get('count', 0)} | {entry.get(field, 0)} |"
        )
    lines.append("")
    return lines


def unattributed_table(values: dict[str, Any]) -> list[str]:
    lines = [
        "## Unattributed Work",
        "",
        "| metric | total | accounted | unattributed | overattributed |",
        "| --- | --- | --- | --- | --- |",
    ]
    for field in WORK_FIELDS:
        row = as_dict(values.get(field))
        lines.append(
            f"| {field} | {row.get('total', 0)} | {row.get('accounted', 0)} | "
            f"{row.get('unattributed', 0)} | {row.get('overattributed', 0)} |"
        )
    lines.append("")
    return lines


def fixture_boundary(name: str, inclusive: int, exclusive: int) -> dict[str, Any]:
    return {
        "name": name,
        "count": 1,
        "inclusive_nanos": inclusive,
        "exclusive_nanos": exclusive,
        "inclusive_rich_instructions": inclusive,
        "exclusive_rich_instructions": exclusive,
        "inclusive_dense_instructions": inclusive,
        "exclusive_dense_instructions": exclusive,
        "inclusive_work": {field: inclusive for field in WORK_FIELDS},
        "exclusive_work": {field: exclusive for field in WORK_FIELDS},
    }


def self_test() -> int:
    child = fixture_boundary("child", 30, 30)
    parent = fixture_boundary("parent", 100, 70)
    assert reconcile_fixture(parent, [child])
    broken = fixture_boundary("broken", 100, 60)
    assert not reconcile_fixture(broken, [child])
    profile = {
        "schema_version": 2,
        "attribution": {
            "calls": {
                "function_profiles_by_name": [parent],
                "method_profiles_by_name": [],
                "builtin_profiles_by_name": [child],
            },
            "includes": {"include_profiles_by_path": []},
            "exclusive_work_totals": {field: 100 for field in WORK_FIELDS},
        },
    }
    summary = summarize_profile(profile, 10)
    rankings = as_dict(summary["rankings"])
    assert list_entries(rankings["exclusive_nanos"])[0]["name"] == "parent"
    assert (
        list_entries(rankings["exclusive_refcounted_value_clones"])[0]["name"]
        == "parent"
    )
    assert summary["unattributed_work"]["value_clones"]["unattributed"] == 0
    print("[pass] exclusive_work_report self-test")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
