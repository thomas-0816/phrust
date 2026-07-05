#!/usr/bin/env python3
"""Summarize native-region attribution from a request profile."""

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

DEFAULT_OUT_DIR = REPO_ROOT / "target" / "performance" / "native-regions"
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
    print(f"[{report['status']}] native region report wrote {rel(out_dir / 'wordpress-root.md')}")
    return 0 if report["status"] in {"pass", "skip"} else 1


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", default=os.environ.get("PHRUST_REQUEST_PROFILE_JSON", ""))
    parser.add_argument("--profile-dir", default=os.environ.get("PHRUST_REQUEST_PROFILE_DIR", ""))
    parser.add_argument("--out-dir", default=os.environ.get("PHRUST_NATIVE_REGION_OUT", ""))
    parser.add_argument("--limit", type=int, default=int(os.environ.get("PHRUST_NATIVE_REGION_LIMIT", "25")))
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
    execution = as_dict(attribution.get("execution"))
    native = as_dict(attribution.get("native"))
    calls = as_dict(attribution.get("calls"))
    arrays = as_dict(attribution.get("arrays"))
    objects = as_dict(attribution.get("objects"))
    includes = as_dict(attribution.get("includes"))

    return {
        "native_activity": {
            "native_candidates": int_value(native.get("native_candidates")),
            "native_compiled_regions": int_value(native.get("native_compiled_regions")),
            "native_executions": int_value(native.get("native_executions")),
            "native_compile_budget_rejections": int_value(
                native.get("native_compile_budget_rejections")
            ),
            "jit_compile_attempts": int_value(native.get("jit_compile_attempts")),
            "jit_compiled": int_value(native.get("jit_compiled")),
            "jit_executed": int_value(native.get("jit_executed")),
            "jit_bailouts": int_value(native.get("jit_bailouts")),
            "jit_side_exits": int_value(native.get("jit_side_exits")),
            "jit_guard_failures": int_value(native.get("jit_guard_failures")),
            "jit_tiering_budget_rejections": int_value(
                native.get("jit_tiering_budget_rejections")
            ),
        },
        "execution_mix": {
            "rich_instructions": int_value(execution.get("rich_instructions")),
            "dense_instructions": int_value(execution.get("dense_instructions")),
            "rich_fallback_functions_by_name": list_entries(
                execution.get("rich_fallback_functions_by_name")
            )[:limit],
            "dense_instruction_families": list_entries(
                execution.get("dense_instruction_families")
            )[:limit],
            "opcode_families": list_entries(execution.get("opcode_families"))[:limit],
        },
        "native_blockers": {
            "native_eligibility_rejections_by_reason": list_entries(
                native.get("native_eligibility_rejections_by_reason")
            )[:limit],
            "native_side_exits_by_reason": list_entries(
                native.get("native_side_exits_by_reason")
            )[:limit],
            "jit_side_exit_reasons": list_entries(native.get("jit_side_exit_reasons"))[:limit],
            "jit_blacklist_reasons": list_entries(native.get("jit_blacklist_reasons"))[:limit],
            "dense_include_entry_fallback_by_path": list_entries(
                includes.get("dense_include_entry_fallback_by_path")
            )[:limit],
            "dense_function_fallback_by_reason": list_entries(
                calls.get("dense_function_fallback_by_reason")
            )[:limit],
            "dense_call_fallback_by_reason": list_entries(
                calls.get("dense_call_fallback_by_reason")
            )[:limit],
            "dense_method_dispatch_fallback_by_reason": list_entries(
                calls.get("dense_method_dispatch_fallback_by_reason")
            )[:limit],
            "array_linear_scan_fallbacks": int_value(arrays.get("array_linear_scan_fallbacks")),
            "array_metadata_recomputes": int_value(arrays.get("array_metadata_recomputes")),
            "array_lookup_fallback_by_reason": list_entries(
                arrays.get("array_lookup_fallback_by_reason")
            )[:limit],
            "property_ic_fallback_by_reason": list_entries(
                objects.get("property_ic_fallback_by_reason")
            )[:limit],
        },
        "hot_boundaries": {
            "includes_by_inclusive_nanos": sort_profile_entries(
                list_entries(includes.get("include_profiles_by_path")),
                "inclusive_nanos",
            )[:limit],
            "functions_by_inclusive_nanos": sort_profile_entries(
                list_entries(calls.get("function_profiles_by_name")),
                "inclusive_nanos",
            )[:limit],
            "methods_by_inclusive_nanos": sort_profile_entries(
                list_entries(calls.get("method_profiles_by_name")),
                "inclusive_nanos",
            )[:limit],
            "builtins_by_inclusive_nanos": sort_profile_entries(
                list_entries(calls.get("builtin_profiles_by_name")),
                "inclusive_nanos",
            )[:limit],
        },
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
        "# WordPress Root Native Region Report",
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
    native = as_dict(summary.get("native_activity"))
    mix = as_dict(summary.get("execution_mix"))
    blockers = as_dict(summary.get("native_blockers"))
    lines.extend([
        f"- profile: `{report.get('profile', '')}`",
        f"- native candidates: `{native.get('native_candidates', 0)}`",
        f"- native compiled regions: `{native.get('native_compiled_regions', 0)}`",
        f"- native executions: `{native.get('native_executions', 0)}`",
        f"- native budget rejections: `{native.get('native_compile_budget_rejections', 0)}`",
        f"- JIT compile attempts: `{native.get('jit_compile_attempts', 0)}`",
        f"- JIT compiled: `{native.get('jit_compiled', 0)}`",
        f"- JIT executed: `{native.get('jit_executed', 0)}`",
        f"- JIT side exits: `{native.get('jit_side_exits', 0)}`",
        f"- rich instructions: `{mix.get('rich_instructions', 0)}`",
        f"- dense instructions: `{mix.get('dense_instructions', 0)}`",
        f"- array linear scan fallbacks: `{blockers.get('array_linear_scan_fallbacks', 0)}`",
        f"- array metadata recomputes: `{blockers.get('array_metadata_recomputes', 0)}`",
        "",
    ])
    for title, key in [
        ("Native Eligibility Rejections", "native_eligibility_rejections_by_reason"),
        ("Native Side Exits", "native_side_exits_by_reason"),
        ("JIT Side Exits", "jit_side_exit_reasons"),
        ("JIT Blacklists", "jit_blacklist_reasons"),
        ("Dense Include Entry Fallbacks", "dense_include_entry_fallback_by_path"),
        ("Dense Function Fallbacks", "dense_function_fallback_by_reason"),
        ("Dense Call Fallbacks", "dense_call_fallback_by_reason"),
        ("Dense Method Dispatch Fallbacks", "dense_method_dispatch_fallback_by_reason"),
        ("Array Lookup Fallbacks", "array_lookup_fallback_by_reason"),
        ("Property IC Fallbacks", "property_ic_fallback_by_reason"),
    ]:
        lines.extend(table_for_entries(title, list_entries(blockers.get(key))))

    for title, key in [
        ("Rich Fallback Functions", "rich_fallback_functions_by_name"),
        ("Dense Instruction Families", "dense_instruction_families"),
        ("Opcode Families", "opcode_families"),
    ]:
        lines.extend(table_for_entries(title, list_entries(mix.get(key))))

    boundaries = as_dict(summary.get("hot_boundaries"))
    for title, key in [
        ("Includes By Inclusive Time", "includes_by_inclusive_nanos"),
        ("Functions By Inclusive Time", "functions_by_inclusive_nanos"),
        ("Methods By Inclusive Time", "methods_by_inclusive_nanos"),
        ("Builtins By Inclusive Time", "builtins_by_inclusive_nanos"),
    ]:
        lines.extend(table_for_entries(title, list_entries(boundaries.get(key))))
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
            "execution": {
                "rich_instructions": 120,
                "dense_instructions": 8,
                "rich_fallback_functions_by_name": [{"name": "render", "value": 4}],
                "dense_instruction_families": [{"name": "echo", "value": 8}],
                "opcode_families": [{"name": "calls", "value": 22}],
            },
            "native": {
                "native_candidates": 9,
                "native_compiled_regions": 2,
                "native_executions": 7,
                "native_compile_budget_rejections": 1,
                "jit_compile_attempts": 3,
                "jit_compiled": 2,
                "jit_executed": 4,
                "jit_bailouts": 1,
                "jit_side_exits": 2,
                "jit_guard_failures": 1,
                "jit_tiering_budget_rejections": 1,
                "native_eligibility_rejections_by_reason": [{"name": "by_ref", "value": 2}],
                "native_side_exits_by_reason": [{"name": "array_shape", "value": 1}],
                "jit_side_exit_reasons": [{"name": "guard", "value": 1}],
                "jit_blacklist_reasons": [{"name": "unstable", "value": 1}],
            },
            "includes": {
                "dense_include_entry_fallback_by_path": [{"name": "/app/bootstrap.php", "value": 1}],
                "include_profiles_by_path": [
                    {"name": "/app/bootstrap.php", "count": 1, "inclusive_nanos": 200}
                ],
            },
            "calls": {
                "dense_function_fallback_by_reason": [{"name": "dynamic", "value": 2}],
                "dense_call_fallback_by_reason": [{"name": "unknown_callable", "value": 1}],
                "dense_method_dispatch_fallback_by_reason": [{"name": "magic", "value": 1}],
                "function_profiles_by_name": [
                    {"name": "render", "count": 1, "inclusive_nanos": 150}
                ],
                "method_profiles_by_name": [
                    {"name": "Renderer::run", "count": 1, "inclusive_nanos": 120}
                ],
                "builtin_profiles_by_name": [
                    {"name": "count", "count": 2, "inclusive_nanos": 80}
                ],
            },
            "arrays": {
                "array_linear_scan_fallbacks": 5,
                "array_metadata_recomputes": 3,
                "array_lookup_fallback_by_reason": [{"name": "mixed_key", "value": 4}],
            },
            "objects": {
                "property_ic_fallback_by_reason": [{"name": "dynamic_property", "value": 2}],
            },
        },
    }
    summary = summarize_profile(profile, 10)
    assert summary["native_activity"]["native_candidates"] == 9
    assert summary["native_activity"]["jit_side_exits"] == 2
    assert summary["execution_mix"]["dense_instructions"] == 8
    assert summary["native_blockers"]["array_linear_scan_fallbacks"] == 5
    assert summary["hot_boundaries"]["functions_by_inclusive_nanos"][0]["name"] == "render"
    print("[pass] native_region_report self-test")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
