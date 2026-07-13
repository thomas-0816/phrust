#!/usr/bin/env python3
"""Summarize call, builtin, and frame hot-path attribution from a request profile."""

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

DEFAULT_OUT_DIR = REPO_ROOT / "target" / "performance" / "call-hotpaths"
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
        f"[{report['status']}] call hotpath report wrote {rel(out_dir / 'wordpress-root.md')}"
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
        "--out-dir", default=os.environ.get("PHRUST_CALL_HOTPATH_OUT", "")
    )
    parser.add_argument(
        "--limit",
        type=int,
        default=int(os.environ.get("PHRUST_CALL_HOTPATH_LIMIT", "25")),
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
        candidates.extend(
            path for path in REPO_ROOT.glob(pattern) if is_request_profile(path)
        )
    if not candidates:
        return None
    return max(candidates, key=lambda path: path.stat().st_mtime)


def latest_profile_in_dir(profile_dir: Path | None) -> Path | None:
    if profile_dir is None or not profile_dir.is_dir():
        return None
    candidates = [
        path for path in profile_dir.glob("*.json") if is_request_profile(path)
    ]
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
    calls = as_dict(attribution.get("calls"))
    clones = as_dict(attribution.get("clones"))

    function_profiles = sort_profile_entries(
        list_entries(calls.get("function_profiles_by_name")),
        "exclusive_nanos",
    )
    method_profiles = sort_profile_entries(
        list_entries(calls.get("method_profiles_by_name")),
        "exclusive_nanos",
    )
    builtin_profiles = sort_profile_entries(
        list_entries(calls.get("builtin_profiles_by_name")),
        "exclusive_nanos",
    )

    return {
        "call_activity": {
            "function_calls": int_value(calls.get("function_calls")),
            "method_calls": int_value(calls.get("method_calls")),
            "internal_function_dispatches": int_value(
                calls.get("internal_function_dispatches")
            ),
            "internal_function_dispatch_cache_hits": int_value(
                calls.get("internal_function_dispatch_cache_hits")
            ),
            "internal_function_dispatch_cache_misses": int_value(
                calls.get("internal_function_dispatch_cache_misses")
            ),
            "function_call_ic_hits": int_value(calls.get("function_call_ic_hits")),
            "function_call_ic_misses": int_value(calls.get("function_call_ic_misses")),
            "method_ic_hits": int_value(calls.get("method_ic_hits")),
            "method_ic_misses": int_value(calls.get("method_ic_misses")),
            "builtin_call_ic_hits": int_value(calls.get("builtin_call_ic_hits")),
            "builtin_call_ic_misses": int_value(calls.get("builtin_call_ic_misses")),
            "dense_direct_call_hits": int_value(calls.get("dense_direct_call_hits")),
            "dense_method_call_hits": int_value(calls.get("dense_method_call_hits")),
            "dense_static_call_hits": int_value(calls.get("dense_static_call_hits")),
            "dense_call_ic_hits": int_value(calls.get("dense_call_ic_hits")),
            "dense_call_ic_misses": int_value(calls.get("dense_call_ic_misses")),
        },
        "frame_activity": {
            "frame_allocations": int_value(calls.get("frame_allocations")),
            "frame_reuses": int_value(calls.get("frame_reuses")),
            "frames_allocated": int_value(calls.get("frames_allocated")),
            "frames_reused": int_value(calls.get("frames_reused")),
            "register_files_allocated": int_value(
                calls.get("register_files_allocated")
            ),
            "register_files_reused": int_value(calls.get("register_files_reused")),
            "tiny_frame_candidates": int_value(calls.get("tiny_frame_candidates")),
            "specialized_frame_hits": int_value(calls.get("specialized_frame_hits")),
            "arg_array_avoided": int_value(calls.get("arg_array_avoided")),
            "heap_frame_avoided": int_value(calls.get("heap_frame_avoided")),
            "direct_arg_frame_hits": int_value(calls.get("direct_arg_frame_hits")),
            "direct_method_frame_hits": int_value(
                calls.get("direct_method_frame_hits")
            ),
            "direct_closure_frame_hits": int_value(
                calls.get("direct_closure_frame_hits")
            ),
            "direct_constructor_frame_hits": int_value(
                calls.get("direct_constructor_frame_hits")
            ),
            "argument_vector_allocations_avoided": int_value(
                calls.get("argument_vector_allocations_avoided")
            ),
        },
        "top_calls": {
            "functions_by_exclusive_nanos": function_profiles[:limit],
            "methods_by_exclusive_nanos": method_profiles[:limit],
            "builtins_by_exclusive_nanos": builtin_profiles[:limit],
            "dense_call_fallback_by_reason": list_entries(
                calls.get("dense_call_fallback_by_reason")
            )[:limit],
            "dense_function_fallback_by_reason": list_entries(
                calls.get("dense_function_fallback_by_reason")
            )[:limit],
            "dense_method_dispatch_fallback_by_reason": list_entries(
                calls.get("dense_method_dispatch_fallback_by_reason")
            )[:limit],
            "builtin_fast_stub_hits": list_entries(calls.get("builtin_fast_stub_hits"))[
                :limit
            ],
            "builtin_fast_stub_misses": list_entries(
                calls.get("builtin_fast_stub_misses")
            )[:limit],
            "builtin_fast_stub_fallback_by_reason": list_entries(
                calls.get("builtin_fast_stub_fallback_by_reason")
            )[:limit],
            "intrinsic_hits": list_entries(calls.get("intrinsic_hits"))[:limit],
            "intrinsic_misses": list_entries(calls.get("intrinsic_misses"))[:limit],
            "frame_reuse_blocked_by_reason": list_entries(
                calls.get("frame_reuse_blocked_by_reason")
            )[:limit],
            "call_frame_layout_observed": list_entries(
                calls.get("call_frame_layout_observed")
            )[:limit],
            "generic_frame_fallback_by_reason": list_entries(
                calls.get("generic_frame_fallback_by_reason")
            )[:limit],
            "direct_frame_fallback_by_reason": list_entries(
                calls.get("direct_frame_fallback_by_reason")
            )[:limit],
        },
        "clone_pressure": {
            "value_clones": int_value(clones.get("value_clones")),
            "array_handle_clones": int_value(clones.get("array_handle_clones")),
            "value_clone_by_kind": list_entries(clones.get("value_clone_by_kind"))[
                :limit
            ],
            "value_clone_by_source_family": list_entries(
                clones.get("value_clone_by_source_family")
            )[:limit],
            "value_clone_by_source_family_and_kind": as_dict(
                clones.get("value_clone_by_source_family_and_kind")
            ),
            "string_allocation_by_source_family": list_entries(
                clones.get("string_allocation_by_source_family")
            )[:limit],
            "array_handle_clone_by_source_family": list_entries(
                clones.get("array_handle_clone_by_source_family")
            )[:limit],
            "by_ref_arg_fallback_by_reason": list_entries(
                clones.get("by_ref_arg_fallback_by_reason")
            )[:limit],
        },
    }


def list_entries(value: Any) -> list[dict[str, Any]]:
    if not isinstance(value, list):
        return []
    return [entry for entry in value if isinstance(entry, dict)]


def sort_profile_entries(
    entries: list[dict[str, Any]], field: str
) -> list[dict[str, Any]]:
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
        "# WordPress Root Call Hotpath Report",
        "",
        f"- status: `{report['status']}`",
    ]
    if report["status"] == "skip":
        lines.extend(
            [
                f"- reason: `{report.get('reason', 'unknown')}`",
                "",
            ]
        )
        path.write_text("\n".join(lines), encoding="utf-8")
        return

    summary = as_dict(report.get("summary"))
    calls = as_dict(summary.get("call_activity"))
    frames = as_dict(summary.get("frame_activity"))
    clones = as_dict(summary.get("clone_pressure"))
    lines.extend(
        [
            f"- profile: `{report.get('profile', '')}`",
            f"- function calls: `{calls.get('function_calls', 0)}`",
            f"- method calls: `{calls.get('method_calls', 0)}`",
            f"- internal dispatches: `{calls.get('internal_function_dispatches', 0)}`",
            f"- frame allocations: `{frames.get('frame_allocations', 0)}`",
            f"- frame reuses: `{frames.get('frame_reuses', 0)}`",
            f"- specialized frame hits: `{frames.get('specialized_frame_hits', 0)}`",
            f"- argument vectors avoided: `{frames.get('argument_vector_allocations_avoided', 0)}`",
            f"- value clones: `{clones.get('value_clones', 0)}`",
            f"- array-handle clones: `{clones.get('array_handle_clones', 0)}`",
            "",
        ]
    )
    top = as_dict(summary.get("top_calls"))
    for title, key in [
        ("Functions By Exclusive Time", "functions_by_exclusive_nanos"),
        ("Methods By Exclusive Time", "methods_by_exclusive_nanos"),
        ("Builtins By Exclusive Time", "builtins_by_exclusive_nanos"),
        ("Dense Call Fallbacks", "dense_call_fallback_by_reason"),
        ("Dense Function Fallbacks", "dense_function_fallback_by_reason"),
        ("Dense Method Dispatch Fallbacks", "dense_method_dispatch_fallback_by_reason"),
        ("Builtin Fast Stub Hits", "builtin_fast_stub_hits"),
        ("Builtin Fast Stub Misses", "builtin_fast_stub_misses"),
        ("Builtin Fast Stub Fallbacks", "builtin_fast_stub_fallback_by_reason"),
        ("Intrinsic Hits", "intrinsic_hits"),
        ("Intrinsic Misses", "intrinsic_misses"),
        ("Frame Reuse Blockers", "frame_reuse_blocked_by_reason"),
        ("Observed Frame Layouts", "call_frame_layout_observed"),
        ("Generic Frame Fallbacks", "generic_frame_fallback_by_reason"),
        ("Direct Frame Fallbacks", "direct_frame_fallback_by_reason"),
    ]:
        lines.extend(table_for_entries(title, list_entries(top.get(key))))

    lines.extend(
        table_for_entries(
            "Value Clone Kinds",
            list_entries(clones.get("value_clone_by_kind")),
        )
    )
    lines.extend(
        table_for_entries(
            "Value Clone Sources",
            list_entries(clones.get("value_clone_by_source_family")),
        )
    )
    lines.extend(
        table_for_entries(
            "Array Handle Clone Sources",
            list_entries(clones.get("array_handle_clone_by_source_family")),
        )
    )
    lines.extend(
        table_for_entries(
            "String Allocation Sources",
            list_entries(clones.get("string_allocation_by_source_family")),
        )
    )
    lines.extend(
        table_for_entries(
            "By-Ref Argument Fallbacks",
            list_entries(clones.get("by_ref_arg_fallback_by_reason")),
        )
    )
    path.write_text("\n".join(lines), encoding="utf-8")


def table_for_entries(title: str, entries: list[dict[str, Any]]) -> list[str]:
    lines = [f"## {title}", ""]
    if not entries:
        lines.extend(["No entries.", ""])
        return lines
    all_keys = set().union(*(entry.keys() for entry in entries))
    numeric_column = (
        "value" if "value" in all_keys else "count" if "count" in all_keys else ""
    )
    extra_column = "exclusive_nanos" if "exclusive_nanos" in all_keys else ""
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
            "calls": {
                "function_calls": 11,
                "method_calls": 5,
                "internal_function_dispatches": 17,
                "internal_function_dispatch_cache_hits": 13,
                "internal_function_dispatch_cache_misses": 4,
                "function_call_ic_hits": 7,
                "function_call_ic_misses": 2,
                "method_ic_hits": 3,
                "method_ic_misses": 1,
                "builtin_call_ic_hits": 6,
                "builtin_call_ic_misses": 2,
                "dense_direct_call_hits": 8,
                "dense_method_call_hits": 4,
                "dense_static_call_hits": 2,
                "dense_call_ic_hits": 9,
                "dense_call_ic_misses": 3,
                "frame_allocations": 10,
                "frame_reuses": 8,
                "frames_allocated": 10,
                "frames_reused": 8,
                "register_files_allocated": 10,
                "register_files_reused": 8,
                "tiny_frame_candidates": 6,
                "specialized_frame_hits": 4,
                "arg_array_avoided": 3,
                "heap_frame_avoided": 2,
                "direct_arg_frame_hits": 5,
                "direct_method_frame_hits": 4,
                "direct_closure_frame_hits": 3,
                "direct_constructor_frame_hits": 2,
                "argument_vector_allocations_avoided": 14,
                "function_profiles_by_name": [
                    {
                        "name": "render",
                        "count": 2,
                        "inclusive_nanos": 200,
                        "exclusive_nanos": 120,
                    }
                ],
                "method_profiles_by_name": [
                    {
                        "name": "Service::run",
                        "count": 1,
                        "inclusive_nanos": 100,
                        "exclusive_nanos": 80,
                    }
                ],
                "builtin_profiles_by_name": [
                    {
                        "name": "count",
                        "count": 3,
                        "inclusive_nanos": 90,
                        "exclusive_nanos": 70,
                    }
                ],
                "dense_call_fallback_by_reason": [{"name": "dynamic", "value": 2}],
                "dense_function_fallback_by_reason": [
                    {"name": "unsupported", "value": 1}
                ],
                "dense_method_dispatch_fallback_by_reason": [
                    {"name": "magic", "value": 1}
                ],
                "builtin_fast_stub_hits": [{"name": "count", "value": 3}],
                "builtin_fast_stub_misses": [{"name": "strlen", "value": 1}],
                "builtin_fast_stub_fallback_by_reason": [
                    {"name": "count.arity", "value": 1}
                ],
                "intrinsic_hits": [{"name": "strlen", "value": 2}],
                "intrinsic_misses": [{"name": "array_map", "value": 1}],
                "frame_reuse_blocked_by_reason": [{"name": "by_ref_param", "value": 2}],
                "call_frame_layout_observed": [
                    {"name": "known_function_frame", "value": 4}
                ],
                "generic_frame_fallback_by_reason": [{"name": "variadic", "value": 1}],
                "direct_frame_fallback_by_reason": [
                    {"name": "dynamic_callable", "value": 1}
                ],
            },
            "clones": {
                "value_clones": 22,
                "array_handle_clones": 11,
                "value_clone_by_kind": [
                    {"name": "scalar_or_uninitialized", "value": 11},
                    {"name": "array_handle", "value": 11},
                ],
                "value_clone_by_source_family": [
                    {"name": "call_argument_snapshot", "value": 7}
                ],
                "array_handle_clone_by_source_family": [
                    {"name": "return_value", "value": 3}
                ],
                "value_clone_by_source_family_and_kind": {
                    "return_value": [{"name": "array_handle", "value": 3}]
                },
                "string_allocation_by_source_family": [
                    {"name": "return_value", "value": 2}
                ],
                "by_ref_arg_fallback_by_reason": [{"name": "temporary", "value": 1}],
            },
        },
    }
    summary = summarize_profile(profile, 10)
    assert summary["call_activity"]["function_calls"] == 11
    assert summary["frame_activity"]["specialized_frame_hits"] == 4
    assert summary["frame_activity"]["argument_vector_allocations_avoided"] == 14
    assert summary["top_calls"]["functions_by_exclusive_nanos"][0]["name"] == "render"
    assert summary["clone_pressure"]["value_clones"] == 22
    assert summary["clone_pressure"]["value_clone_by_kind"][1]["name"] == "array_handle"
    print("[pass] call_hotpath_report self-test")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
