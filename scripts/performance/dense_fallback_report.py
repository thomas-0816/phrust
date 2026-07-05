#!/usr/bin/env python3
"""Summarize dense-to-rich fallback attribution from a request profile."""

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

DEFAULT_OUT_DIR = REPO_ROOT / "target" / "performance" / "dense-fallbacks"
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
    print(f"[{report['status']}] dense fallback report wrote {rel(out_dir / 'wordpress-root.md')}")
    return 0 if report["status"] in {"pass", "skip"} else 1


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", default=os.environ.get("PHRUST_REQUEST_PROFILE_JSON", ""))
    parser.add_argument("--profile-dir", default=os.environ.get("PHRUST_REQUEST_PROFILE_DIR", ""))
    parser.add_argument("--out-dir", default=os.environ.get("PHRUST_DENSE_FALLBACK_OUT", ""))
    parser.add_argument("--limit", type=int, default=int(os.environ.get("PHRUST_DENSE_FALLBACK_LIMIT", "25")))
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
    summary = summarize_profile(profile, max(args.limit, 1))
    status = "pass" if summary["total_fallback_signals"] > 0 else "pass"
    return {
        "status": status,
        "profile": rel(profile_path),
        "request": profile.get("request", {}),
        "summary": summary,
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
    includes = as_dict(attribution.get("includes"))
    calls = as_dict(attribution.get("calls"))
    arrays = as_dict(attribution.get("arrays"))
    objects = as_dict(attribution.get("objects"))
    native = as_dict(attribution.get("native"))
    include_profiles = list_entries(includes.get("include_profiles_by_path"))
    function_profiles = list_entries(calls.get("function_profiles_by_name"))
    method_profiles = list_entries(calls.get("method_profiles_by_name"))
    builtin_profiles = list_entries(calls.get("builtin_profiles_by_name"))
    rich_fallback_functions = list_entries(execution.get("rich_fallback_functions_by_name"))
    dense_include_fallbacks_by_path = list_entries(includes.get("dense_include_entry_fallback_by_path"))
    dense_include_fallbacks_by_reason = list_entries(
        includes.get("dense_include_entry_fallback_by_reason")
    )
    dense_function_fallbacks_by_reason = list_entries(
        calls.get("dense_function_fallback_by_reason")
    )
    dense_call_fallbacks_by_reason = list_entries(calls.get("dense_call_fallback_by_reason"))
    dense_method_fallbacks_by_reason = list_entries(
        calls.get("dense_method_dispatch_fallback_by_reason")
    )
    total_signals = sum_ints(
        rich_fallback_functions,
        dense_include_fallbacks_by_path,
        dense_include_fallbacks_by_reason,
        dense_function_fallbacks_by_reason,
        dense_call_fallbacks_by_reason,
        dense_method_fallbacks_by_reason,
    )
    return {
        "total_fallback_signals": total_signals,
        "dense_vs_rich": {
            "dense_bytecode_instructions": int_value(execution.get("dense_bytecode_instructions")),
            "rich_instructions": int_value(execution.get("rich_instructions")),
            "include_rich_instructions": int_value(execution.get("include_rich_instructions")),
            "entry_rich_instructions": int_value(execution.get("entry_rich_instructions")),
            "dense_functions_executed": int_value(execution.get("dense_functions_executed")),
            "rich_fallback_functions_executed": int_value(
                execution.get("rich_fallback_functions_executed")
            ),
            "dense_include_entry_attempts": int_value(
                includes.get("dense_include_entry_attempts")
            ),
            "dense_include_entry_successes": int_value(
                includes.get("dense_include_entry_successes")
            ),
            "dense_include_entry_fallbacks": int_value(
                includes.get("dense_include_entry_fallbacks")
            ),
        },
        "top_fallbacks": {
            "rich_fallback_functions_by_name": rich_fallback_functions[:limit],
            "dense_include_entry_fallback_by_path": dense_include_fallbacks_by_path[:limit],
            "dense_include_entry_fallback_by_reason": dense_include_fallbacks_by_reason[:limit],
            "dense_function_fallback_by_reason": dense_function_fallbacks_by_reason[:limit],
            "dense_call_fallback_by_reason": dense_call_fallbacks_by_reason[:limit],
            "dense_method_dispatch_fallback_by_reason": dense_method_fallbacks_by_reason[:limit],
            "array_fast_path_fallback_by_reason": list_entries(
                arrays.get("array_fast_path_fallback_by_reason")
            )[:limit],
            "property_ic_fallback_reasons": list_entries(
                objects.get("property_ic_fallback_reasons")
            )[:limit],
            "native_eligibility_rejections_by_reason": list_entries(
                native.get("native_eligibility_rejections_by_reason")
            )[:limit],
        },
        "hot_boundaries": {
            "includes_by_inclusive_nanos": sort_profile_entries(include_profiles, "inclusive_nanos")[
                :limit
            ],
            "functions_by_inclusive_nanos": sort_profile_entries(
                function_profiles, "inclusive_nanos"
            )[:limit],
            "methods_by_inclusive_nanos": sort_profile_entries(method_profiles, "inclusive_nanos")[
                :limit
            ],
            "builtins_by_inclusive_nanos": sort_profile_entries(
                builtin_profiles, "inclusive_nanos"
            )[:limit],
        },
    }


def list_entries(value: Any) -> list[dict[str, Any]]:
    if not isinstance(value, list):
        return []
    return [entry for entry in value if isinstance(entry, dict)]


def sort_profile_entries(entries: list[dict[str, Any]], field: str) -> list[dict[str, Any]]:
    return sorted(entries, key=lambda entry: int_value(entry.get(field)), reverse=True)


def sum_ints(*entry_groups: list[dict[str, Any]]) -> int:
    total = 0
    for entries in entry_groups:
        for entry in entries:
            total += int_value(entry.get("value"))
    return total


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


def write_markdown(report: dict[str, Any], path: Path) -> None:
    summary = as_dict(report.get("summary"))
    dense_vs_rich = as_dict(summary.get("dense_vs_rich"))
    top = as_dict(summary.get("top_fallbacks"))
    hot = as_dict(summary.get("hot_boundaries"))
    lines = [
        "# Dense Fallback Report",
        "",
        f"- status: `{report['status']}`",
        f"- profile: `{report.get('profile', '')}`",
        f"- total fallback signals: `{summary.get('total_fallback_signals', 0)}`",
        "",
        "## Dense/Rich Split",
        "",
    ]
    for key, value in dense_vs_rich.items():
        lines.append(f"- `{key}`: `{value}`")
    lines.extend(["", "## Top Fallbacks", ""])
    for name, entries in top.items():
        lines.extend([f"### {name}", ""])
        append_entries(lines, entries)
    lines.extend(["", "## Hot Boundaries", ""])
    for name, entries in hot.items():
        lines.extend([f"### {name}", ""])
        append_entries(lines, entries)
    lines.append("")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(lines), encoding="utf-8")


def append_entries(lines: list[str], value: Any) -> None:
    entries = value if isinstance(value, list) else []
    if not entries:
        lines.extend(["_No entries._", ""])
        return
    lines.extend(["| name | value | count | inclusive_ms | rich_instructions | dense_instructions |", "| --- | ---: | ---: | ---: | ---: | ---: |"])
    for entry in entries:
        name = str(entry.get("name", ""))
        value = int_value(entry.get("value"))
        count = int_value(entry.get("count"))
        inclusive_ms = int_value(entry.get("inclusive_nanos")) / 1_000_000.0
        rich = int_value(entry.get("rich_instructions"))
        dense = int_value(entry.get("dense_instructions"))
        lines.append(
            f"| `{escape_pipe(name)}` | {value} | {count} | {inclusive_ms:.3f} | {rich} | {dense} |"
        )
    lines.append("")


def escape_pipe(value: str) -> str:
    return value.replace("|", "\\|")


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


def self_test() -> int:
    profile = {
        "request": {"path": "/", "status": 200},
        "attribution": {
            "execution": {
                "dense_bytecode_instructions": 11,
                "rich_instructions": 17,
                "include_rich_instructions": 13,
                "entry_rich_instructions": 4,
                "dense_functions_executed": 2,
                "rich_fallback_functions_executed": 1,
                "rich_fallback_functions_by_name": [{"name": "render", "value": 3}],
            },
            "includes": {
                "dense_include_entry_attempts": 4,
                "dense_include_entry_successes": 2,
                "dense_include_entry_fallbacks": 2,
                "dense_include_entry_fallback_by_reason": [
                    {"name": "unsupported_opcode", "value": 2}
                ],
                "dense_include_entry_fallback_by_path": [
                    {"name": "/srv/app/template.php", "value": 2}
                ],
                "include_profiles_by_path": [
                    {
                        "name": "/srv/app/template.php",
                        "count": 1,
                        "inclusive_nanos": 3_000_000,
                        "rich_instructions": 13,
                        "dense_instructions": 5,
                    }
                ],
            },
            "calls": {
                "function_profiles_by_name": [
                    {"name": "render", "count": 3, "inclusive_nanos": 2_000_000}
                ],
                "builtin_profiles_by_name": [
                    {"name": "count", "count": 7, "inclusive_nanos": 1_000_000}
                ],
                "dense_call_fallback_by_reason": [{"name": "unknown_function", "value": 1}],
                "dense_function_fallback_by_reason": [
                    {"name": "unsupported_terminator", "value": 4}
                ],
            },
            "arrays": {"array_fast_path_fallback_by_reason": []},
            "objects": {"property_ic_fallback_reasons": []},
            "native": {"native_eligibility_rejections_by_reason": []},
        },
    }
    summary = summarize_profile(profile, 10)
    assert summary["total_fallback_signals"] == 12
    assert summary["dense_vs_rich"]["dense_include_entry_fallbacks"] == 2
    assert summary["hot_boundaries"]["includes_by_inclusive_nanos"][0]["name"].endswith(
        "template.php"
    )
    print("[pass] dense_fallback_report self-test")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
