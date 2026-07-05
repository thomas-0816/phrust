#!/usr/bin/env python3
"""Summarize persistent-metadata attribution from a request profile."""

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

DEFAULT_OUT_DIR = REPO_ROOT / "target" / "performance" / "persistent-metadata"
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
    print(
        f"[{report['status']}] persistent metadata report wrote "
        f"{rel(out_dir / 'wordpress-root.md')}"
    )
    return 0 if report["status"] in {"pass", "skip"} else 1


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", default=os.environ.get("PHRUST_REQUEST_PROFILE_JSON", ""))
    parser.add_argument("--profile-dir", default=os.environ.get("PHRUST_REQUEST_PROFILE_DIR", ""))
    parser.add_argument("--out-dir", default=os.environ.get("PHRUST_PERSISTENT_METADATA_OUT", ""))
    parser.add_argument(
        "--limit",
        type=int,
        default=int(os.environ.get("PHRUST_PERSISTENT_METADATA_LIMIT", "25")),
    )
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
    metadata = as_dict(attribution.get("metadata"))
    includes = as_dict(attribution.get("includes"))
    execution = as_dict(attribution.get("execution"))
    clones = as_dict(attribution.get("clones"))
    arrays = as_dict(attribution.get("arrays"))

    return {
        "persistent_activity": {
            "persistent_engine_allocations": int_value(
                metadata.get("persistent_engine_allocations")
            ),
            "persistent_engine_bytes": int_value(metadata.get("persistent_engine_bytes")),
            "request_arena_allocations": int_value(metadata.get("request_arena_allocations")),
            "request_arena_bytes": int_value(metadata.get("request_arena_bytes")),
            "request_pool_resets": int_value(metadata.get("request_pool_resets")),
            "destructor_sensitive_arena_blocks": int_value(
                metadata.get("destructor_sensitive_arena_blocks")
            ),
            "include_resolution_hits": int_value(metadata.get("include_resolution_hits")),
            "include_resolution_misses": int_value(metadata.get("include_resolution_misses")),
            "include_compile_hits": int_value(metadata.get("include_compile_hits")),
            "include_compile_misses": int_value(metadata.get("include_compile_misses")),
        },
        "quickening_activity": {
            "quickening_attempts": int_value(metadata.get("quickening_attempts")),
            "quickening_specialized": int_value(metadata.get("quickening_specialized")),
            "quickening_guard_hits": int_value(metadata.get("quickening_guard_hits")),
            "quickening_guard_misses": int_value(metadata.get("quickening_guard_misses")),
            "quickening_guard_failures": int_value(metadata.get("quickening_guard_failures")),
            "quickening_fallback_calls": int_value(metadata.get("quickening_fallback_calls")),
            "quickening_dequickens": int_value(metadata.get("quickening_dequickens")),
            "quickening_megamorphic": int_value(metadata.get("quickening_megamorphic")),
            "quickening_disabled": int_value(metadata.get("quickening_disabled")),
        },
        "metadata_blockers": {
            "arena_fallback_allocations_by_reason": list_entries(
                metadata.get("arena_fallback_allocations_by_reason")
            )[:limit],
            "quickening_candidates_by_family": list_entries(
                metadata.get("quickening_candidates_by_family")
            )[:limit],
            "quickening_applied_by_family": list_entries(
                metadata.get("quickening_applied_by_family")
            )[:limit],
            "quickened_executions_by_family": list_entries(
                metadata.get("quickened_executions_by_family")
            )[:limit],
            "quickening_guard_failures_by_family": list_entries(
                metadata.get("quickening_guard_failures_by_family")
            )[:limit],
            "quickening_dequickened_by_reason": list_entries(
                metadata.get("quickening_dequickened_by_reason")
            )[:limit],
            "rich_fallback_functions_by_name": list_entries(
                execution.get("rich_fallback_functions_by_name")
            )[:limit],
            "dense_include_entry_fallback_by_path": list_entries(
                includes.get("dense_include_entry_fallback_by_path")
            )[:limit],
            "value_clone_by_source_family": list_entries(
                clones.get("value_clone_by_source_family")
            )[:limit],
            "array_handle_clone_by_source_family": list_entries(
                clones.get("array_handle_clone_by_source_family")
            )[:limit],
            "array_metadata_recomputes": int_value(arrays.get("array_metadata_recomputes")),
        },
        "hot_includes": sort_profile_entries(
            list_entries(includes.get("include_profiles_by_path")),
            "inclusive_nanos",
        )[:limit],
    }


def list_entries(value: Any) -> list[dict[str, Any]]:
    if not isinstance(value, list):
        return []
    return [entry for entry in value if isinstance(entry, dict)]


def sort_profile_entries(entries: list[dict[str, Any]], field: str) -> list[dict[str, Any]]:
    return sorted(entries, key=lambda entry: int_value(entry.get(field)), reverse=True)


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
        "# WordPress Root Persistent Metadata Report",
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
    persistent = as_dict(summary.get("persistent_activity"))
    quickening = as_dict(summary.get("quickening_activity"))
    blockers = as_dict(summary.get("metadata_blockers"))
    lines.extend([
        f"- profile: `{report.get('profile', '')}`",
        f"- persistent engine allocations: `{persistent.get('persistent_engine_allocations', 0)}`",
        f"- persistent engine bytes: `{persistent.get('persistent_engine_bytes', 0)}`",
        f"- request arena allocations: `{persistent.get('request_arena_allocations', 0)}`",
        f"- request arena bytes: `{persistent.get('request_arena_bytes', 0)}`",
        f"- include resolution hits: `{persistent.get('include_resolution_hits', 0)}`",
        f"- include resolution misses: `{persistent.get('include_resolution_misses', 0)}`",
        f"- include compile hits: `{persistent.get('include_compile_hits', 0)}`",
        f"- include compile misses: `{persistent.get('include_compile_misses', 0)}`",
        f"- quickening attempts: `{quickening.get('quickening_attempts', 0)}`",
        f"- quickening specialized: `{quickening.get('quickening_specialized', 0)}`",
        f"- quickening guard failures: `{quickening.get('quickening_guard_failures', 0)}`",
        f"- quickening dequickens: `{quickening.get('quickening_dequickens', 0)}`",
        f"- array metadata recomputes: `{blockers.get('array_metadata_recomputes', 0)}`",
        "",
    ])
    for title, key in [
        ("Arena Fallbacks", "arena_fallback_allocations_by_reason"),
        ("Quickening Candidates", "quickening_candidates_by_family"),
        ("Quickening Applied", "quickening_applied_by_family"),
        ("Quickened Executions", "quickened_executions_by_family"),
        ("Quickening Guard Failures", "quickening_guard_failures_by_family"),
        ("Quickening Dequickens", "quickening_dequickened_by_reason"),
        ("Rich Fallback Functions", "rich_fallback_functions_by_name"),
        ("Dense Include Entry Fallbacks", "dense_include_entry_fallback_by_path"),
        ("Value Clone Sources", "value_clone_by_source_family"),
        ("Array Handle Clone Sources", "array_handle_clone_by_source_family"),
    ]:
        lines.extend(table_for_entries(title, list_entries(blockers.get(key))))
    lines.extend(table_for_entries("Hot Includes", list_entries(summary.get("hot_includes"))))
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
            "metadata": {
                "persistent_engine_allocations": 3,
                "persistent_engine_bytes": 2048,
                "request_arena_allocations": 5,
                "request_arena_bytes": 4096,
                "request_pool_resets": 1,
                "destructor_sensitive_arena_blocks": 1,
                "include_resolution_hits": 7,
                "include_resolution_misses": 2,
                "include_compile_hits": 6,
                "include_compile_misses": 1,
                "quickening_attempts": 8,
                "quickening_specialized": 4,
                "quickening_guard_hits": 9,
                "quickening_guard_misses": 2,
                "quickening_guard_failures": 1,
                "quickening_fallback_calls": 2,
                "quickening_dequickens": 1,
                "quickening_megamorphic": 1,
                "quickening_disabled": 0,
                "arena_fallback_allocations_by_reason": [
                    {"name": "destructor_sensitive", "value": 1}
                ],
                "quickening_candidates_by_family": [{"name": "dim_fetch", "value": 3}],
                "quickening_applied_by_family": [{"name": "dim_fetch", "value": 2}],
                "quickened_executions_by_family": [{"name": "dim_fetch", "value": 6}],
                "quickening_guard_failures_by_family": [{"name": "dim_fetch", "value": 1}],
                "quickening_dequickened_by_reason": [{"name": "megamorphic", "value": 1}],
            },
            "includes": {
                "include_profiles_by_path": [
                    {"name": "/app/bootstrap.php", "count": 1, "inclusive_nanos": 100}
                ],
                "dense_include_entry_fallback_by_path": [
                    {"name": "/app/bootstrap.php", "value": 1}
                ],
            },
            "execution": {
                "rich_fallback_functions_by_name": [{"name": "render", "value": 2}]
            },
            "clones": {
                "value_clone_by_source_family": [{"name": "return_value", "value": 3}],
                "array_handle_clone_by_source_family": [
                    {"name": "array_element_read", "value": 2}
                ],
            },
            "arrays": {"array_metadata_recomputes": 4},
        },
    }
    summary = summarize_profile(profile, 10)
    assert summary["persistent_activity"]["persistent_engine_allocations"] == 3
    assert summary["persistent_activity"]["include_compile_hits"] == 6
    assert summary["quickening_activity"]["quickening_attempts"] == 8
    assert summary["metadata_blockers"]["array_metadata_recomputes"] == 4
    assert summary["hot_includes"][0]["name"] == "/app/bootstrap.php"
    print("[pass] persistent_metadata_report self-test")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
