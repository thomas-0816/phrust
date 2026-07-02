#!/usr/bin/env python3
"""Shared helpers for Phrust performance ratchet reports."""

from __future__ import annotations

import argparse
import json
import math
import os
import platform
import statistics
import subprocess
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
COMPILE_PHASES = (
    "cache_prepare_ms",
    "cache_load_ms",
    "cache_store_ms",
    "source_read_ms",
    "frontend_analyze_ms",
    "ir_lower_ms",
    "ir_verify_ms",
    "optimizer_ms",
    "bytecode_lower_ms",
    "bytecode_layout_ms",
    "superinstruction_select_ms",
    "vm_construct_ms",
    "diagnostics_ms",
    "counters_write_ms",
    "timings_write_ms",
)


def rel(path: Path) -> str:
    try:
        return path.resolve().relative_to(ROOT).as_posix()
    except ValueError:
        return path.as_posix()


def git_commit() -> str:
    try:
        completed = subprocess.run(
            ["git", "rev-parse", "--short=12", "HEAD"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            timeout=2.0,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired):
        return "unknown"
    return completed.stdout.strip() if completed.returncode == 0 else "unknown"


def executable(path: Path) -> bool:
    return path.is_file() and os.access(path, os.X_OK)


def reference_status() -> str:
    env_path = os.getenv("REFERENCE_PHP")
    if env_path:
        return "available" if executable(Path(env_path)) else "missing"
    default = ROOT / "third_party/php-src/sapi/cli/php"
    return "available" if executable(default) else "skipped"


def environment(profile: str = "debug") -> dict[str, Any]:
    return {
        "git_commit": git_commit(),
        "platform": platform.platform(),
        "profile": profile,
        "reference_php": reference_status(),
        "python": platform.python_version(),
    }


def load_json(path: Path) -> dict[str, Any]:
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except OSError as error:
        raise SystemExit(f"{rel(path)}: {error}") from error
    except json.JSONDecodeError as error:
        raise SystemExit(f"{rel(path)}: invalid JSON: {error}") from error
    if not isinstance(data, dict):
        raise SystemExit(f"{rel(path)}: root must be a JSON object")
    return data


def write_json(path: Path, data: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def percentile(values: list[float], pct: float) -> float:
    if not values:
        return 0.0
    ordered = sorted(values)
    if len(ordered) == 1:
        return ordered[0]
    rank = (len(ordered) - 1) * pct
    lower = math.floor(rank)
    upper = math.ceil(rank)
    if lower == upper:
        return ordered[lower]
    weight = rank - lower
    return ordered[lower] * (1.0 - weight) + ordered[upper] * weight


def distribution_metrics(prefix: str, values: list[float]) -> dict[str, float]:
    if not values:
        return {}
    return {
        f"{prefix}.min": min(values),
        f"{prefix}.p50": statistics.median(values),
        f"{prefix}.p95": percentile(values, 0.95),
        f"{prefix}.p99": percentile(values, 0.99),
        f"{prefix}.max": max(values),
    }


def phase_ms(timings: dict[str, Any], name: str) -> float:
    phases = timings.get("phases")
    if not isinstance(phases, dict):
        return 0.0
    value = phases.get(name)
    return float(value) if isinstance(value, (int, float)) else 0.0


def compile_total_ms(timings: dict[str, Any]) -> float:
    return sum(phase_ms(timings, name) for name in COMPILE_PHASES)


def timing_metrics(external_ms: list[float], timing_reports: list[dict[str, Any]]) -> dict[str, float]:
    metrics = distribution_metrics("external_wall_ms", external_ms)
    if not timing_reports:
        return metrics
    internal_values = [
        float(item.get("total_internal_ms", 0.0))
        for item in timing_reports
        if isinstance(item.get("total_internal_ms"), (int, float))
    ]
    if internal_values:
        metrics.update(distribution_metrics("internal_total_ms", internal_values))
        startup_values = [
            max(external - internal, 0.0)
            for external, internal in zip(external_ms, internal_values, strict=False)
        ]
        metrics.update(distribution_metrics("startup_external_ms", startup_values))
    compile_values = [compile_total_ms(item) for item in timing_reports]
    execute_values = [phase_ms(item, "execute_ms") for item in timing_reports]
    metrics.update(distribution_metrics("compile_total_ms", compile_values))
    metrics.update(distribution_metrics("execute_ms", execute_values))
    return metrics


def phase_metric_map(timing_reports: list[dict[str, Any]]) -> dict[str, float]:
    values: dict[str, list[float]] = {}
    for report in timing_reports:
        phases = report.get("phases")
        if not isinstance(phases, dict):
            continue
        for name, value in phases.items():
            if isinstance(name, str) and isinstance(value, (int, float)):
                values.setdefault(name, []).append(float(value))
    return {
        f"phase:{name}.p50": statistics.median(items)
        for name, items in sorted(values.items())
        if items
    }


def counter_highlights(counters: dict[str, Any]) -> dict[str, int]:
    highlights: dict[str, int] = {}
    for key, value in counters.items():
        if not isinstance(key, str) or not isinstance(value, int) or value == 0:
            continue
        if any(
            marker in key
            for marker in (
                "instruction",
                "cache",
                "quickening",
                "fallback",
                "guard",
                "fast",
                "dispatch",
                "call",
                "native",
            )
        ):
            highlights[key] = value
    return highlights


def make_report(
    *,
    run_id: str,
    created_by: str,
    scenarios: list[dict[str, Any]],
    failures: list[str] | None = None,
    profile: str = "debug",
) -> dict[str, Any]:
    return {
        "schema_version": 1,
        "run_id": run_id,
        "created_by": created_by,
        "environment": environment(profile),
        "scenarios": scenarios,
        "failures": failures or [],
    }


def validate_report(data: dict[str, Any]) -> list[str]:
    failures: list[str] = []
    if data.get("schema_version") != 1:
        failures.append("schema_version must be 1")
    for field in ("run_id", "created_by"):
        if not isinstance(data.get(field), str) or not data.get(field):
            failures.append(f"{field} must be a non-empty string")
    if not isinstance(data.get("environment"), dict):
        failures.append("environment must be an object")
    scenarios = data.get("scenarios")
    if not isinstance(scenarios, list):
        failures.append("scenarios must be an array")
        return failures
    for index, scenario in enumerate(scenarios):
        if not isinstance(scenario, dict):
            failures.append(f"scenarios[{index}] must be an object")
            continue
        for field in ("id", "group", "kind", "correctness"):
            if not isinstance(scenario.get(field), str) or not scenario.get(field):
                failures.append(f"scenarios[{index}].{field} must be a non-empty string")
        if scenario.get("correctness") not in {"pass", "fail", "skip"}:
            failures.append(f"scenarios[{index}].correctness is invalid")
        metrics = scenario.get("metrics")
        if not isinstance(metrics, dict):
            failures.append(f"scenarios[{index}].metrics must be an object")
        else:
            for name, value in metrics.items():
                if not isinstance(name, str) or not isinstance(value, (int, float)):
                    failures.append(f"scenarios[{index}].metrics has non-numeric metric {name!r}")
        for field in ("phase_metrics", "counter_highlights", "artifacts"):
            if not isinstance(scenario.get(field, {}), dict):
                failures.append(f"scenarios[{index}].{field} must be an object")
    if not isinstance(data.get("failures", []), list):
        failures.append("failures must be an array")
    return failures


def render_report_markdown(report: dict[str, Any], title: str) -> str:
    failures = report.get("failures", [])
    scenarios = [item for item in report.get("scenarios", []) if isinstance(item, dict)]
    slowest = sorted(
        scenarios,
        key=lambda item: float(item.get("metrics", {}).get("external_wall_ms.p50", 0.0)),
        reverse=True,
    )[:10]
    lines = [
        f"# {title}",
        "",
        "| Field | Value |",
        "| --- | --- |",
        f"| Run | `{report.get('run_id', 'unknown')}` |",
        f"| Scenarios | {len(scenarios)} |",
        f"| Failures | {len(failures)} |",
        f"| Git commit | `{report.get('environment', {}).get('git_commit', 'unknown')}` |",
        "",
        "## Slowest Scenarios",
        "",
        "| Scenario | Group | Kind | Correctness | p50 external ms | p95 external ms | Compile p50 | Execute p50 |",
        "| --- | --- | --- | --- | ---: | ---: | ---: | ---: |",
    ]
    for item in slowest:
        metrics = item.get("metrics", {})
        lines.append(
            f"| `{item.get('id', '')}` | `{item.get('group', '')}` | `{item.get('kind', '')}` | "
            f"`{item.get('correctness', '')}` | "
            f"{float(metrics.get('external_wall_ms.p50', 0.0)):.3f} | "
            f"{float(metrics.get('external_wall_ms.p95', 0.0)):.3f} | "
            f"{float(metrics.get('compile_total_ms.p50', 0.0)):.3f} | "
            f"{float(metrics.get('execute_ms.p50', 0.0)):.3f} |"
        )
    if failures:
        lines.extend(["", "## Failures", ""])
        lines.extend(f"- {failure}" for failure in failures)
    return "\n".join(lines) + "\n"


def validate_paths(paths: list[Path]) -> int:
    failed = False
    for path in paths:
        data = load_json(path)
        failures = validate_report(data)
        if failures:
            failed = True
            print(f"[fail] {rel(path)} invalid:")
            for failure in failures:
                print(f"  - {failure}")
        else:
            print(f"[pass] {rel(path)}")
    return 1 if failed else 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--validate", nargs="+", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        report = make_report(
            run_id="self-test",
            created_by="ratchet_schema.py",
            scenarios=[
                {
                    "id": "self",
                    "group": "cli",
                    "kind": "startup",
                    "correctness": "pass",
                    "metrics": {"external_wall_ms.p50": 1.0},
                    "phase_metrics": {},
                    "counter_highlights": {},
                    "artifacts": {},
                }
            ],
        )
        assert validate_report(report) == []
        assert "Slowest Scenarios" in render_report_markdown(report, "Self Test")
        print("[pass] ratchet_schema self-test")
        return 0
    if args.validate:
        return validate_paths(args.validate)
    parser.print_help()
    return 1


if __name__ == "__main__":
    sys.exit(main())
