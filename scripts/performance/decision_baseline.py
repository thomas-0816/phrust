#!/usr/bin/env python3
"""Build a local performance decision baseline from benchmark and app-flow data."""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_ENGINE = ROOT / "target/debug/php-vm"
DEFAULT_OUT_DIR = ROOT / "target/performance/decision"


def env_value(name: str, fallback: str | None = None, default: str = "") -> str:
    value = os.getenv(name)
    if value is not None:
        return value
    if fallback is not None:
        value = os.getenv(fallback)
        if value is not None:
            return value
    return default


def rel(path: Path) -> str:
    try:
        return path.resolve().relative_to(ROOT).as_posix()
    except ValueError:
        return path.as_posix()


def positive_int(value: str) -> int:
    parsed = int(value)
    if parsed < 0:
        raise argparse.ArgumentTypeError("value must be non-negative")
    return parsed


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--engine", type=Path, default=DEFAULT_ENGINE)
    parser.add_argument("--out-dir", type=Path, default=DEFAULT_OUT_DIR)
    parser.add_argument(
        "--benchmark-repetitions",
        type=positive_int,
        default=int(
            env_value(
                "PHRUST_DECISION_BENCH_REPETITIONS",
                "PHRUST_PERF_DECISION_ITERATIONS",
                "10",
            )
        ),
    )
    parser.add_argument(
        "--benchmark-warmups",
        type=positive_int,
        default=int(
            env_value(
                "PHRUST_DECISION_BENCH_WARMUPS",
                "PHRUST_PERF_DECISION_WARMUPS",
                "3",
            )
        ),
    )
    parser.add_argument(
        "--app-flow-iterations",
        type=positive_int,
        default=int(
            env_value(
                "PHRUST_DECISION_APP_FLOW_ITERATIONS",
                "PHRUST_PERF_DECISION_ITERATIONS",
                "10",
            )
        ),
    )
    parser.add_argument(
        "--app-flow-warmups",
        type=positive_int,
        default=int(
            env_value(
                "PHRUST_DECISION_APP_FLOW_WARMUPS",
                "PHRUST_PERF_DECISION_WARMUPS",
                "3",
            )
        ),
    )
    parser.add_argument(
        "--app-flow-scale",
        type=positive_int,
        default=int(
            env_value(
                "PHRUST_DECISION_APP_FLOW_SCALE",
                "PHRUST_PERF_DECISION_SCALE",
                "2",
            )
        ),
    )
    parser.add_argument(
        "--timeout",
        type=float,
        default=float(
            env_value(
                "PHRUST_DECISION_TIMEOUT",
                "PHRUST_PERF_DECISION_TIMEOUT",
                os.getenv("PHRUST_APP_FLOW_TIMEOUT", "30.0"),
            )
        ),
    )
    parser.add_argument("--smoke", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    return parser.parse_args()


def load_json(path: Path) -> dict[str, Any]:
    data = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(data, dict):
        raise SystemExit(f"{rel(path)}: root is not an object")
    return data


def run_command(command: list[str]) -> None:
    completed = subprocess.run(command, cwd=ROOT, text=True, check=False)
    if completed.returncode != 0:
        raise SystemExit(f"command failed ({completed.returncode}): {' '.join(command)}")


def metric_value(measurement: dict[str, Any], name: str) -> float | None:
    metrics = measurement.get("metrics")
    if not isinstance(metrics, list):
        return None
    for metric in metrics:
        if not isinstance(metric, dict) or metric.get("name") != name:
            continue
        value = metric.get("value")
        if isinstance(value, (int, float)):
            return float(value)
    return None


def phase_value(timings: dict[str, Any], name: str) -> float | None:
    phases = timings.get("phases")
    if not isinstance(phases, dict):
        return None
    value = phases.get(name)
    return float(value) if isinstance(value, (int, float)) else None


def count_value(timings: dict[str, Any], name: str) -> int | None:
    counts = timings.get("counts")
    if not isinstance(counts, dict):
        return None
    value = counts.get(name)
    return int(value) if isinstance(value, int) else None


def scenario_id(measurement: dict[str, Any]) -> str:
    scenario = measurement.get("scenario")
    if isinstance(scenario, dict) and isinstance(scenario.get("id"), str):
        return scenario["id"]
    return "<unknown>"


def benchmark_decisions(report: dict[str, Any]) -> list[dict[str, Any]]:
    measurements = [
        item
        for item in report.get("measurements", [])
        if isinstance(item, dict) and item.get("engine") == "rust-vm"
    ]
    rows: list[dict[str, Any]] = []
    for item in measurements:
        rows.append(
            {
                "scenario": scenario_id(item),
                "status": item.get("status"),
                "external_wall_ms": metric_value(item, "external_wall_ms"),
                "internal_total_ms": metric_value(item, "internal_total_ms"),
                "startup_external_ms": metric_value(item, "startup_external_ms"),
                "compile_total_ms": metric_value(item, "compile_total_ms"),
                "execute_ms": metric_value(item, "execute_ms"),
                "compile_share_percent": metric_value(item, "compile_share_percent"),
                "execute_share_percent": metric_value(item, "execute_share_percent"),
                "timing_warnings": item.get("timing_warnings", []),
                "phases": item.get("phase_timings", {}),
            }
        )
    return rows


def app_flow_decisions(summary: dict[str, Any]) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    for item in summary.get("rows", []):
        if not isinstance(item, dict) or item.get("status") == "skip":
            continue
        phases = item.get("phase_summary") if isinstance(item.get("phase_summary"), dict) else {}
        timings = item.get("phase_timings") if isinstance(item.get("phase_timings"), dict) else {}
        rows.append(
            {
                "scenario": item.get("scenario"),
                "row": item.get("row"),
                "status": item.get("status"),
                "correctness": item.get("correctness"),
                "median_ms": item.get("median_ms"),
                "ratio_vs_reference": item.get("ratio_vs_reference"),
                "compile_total_ms": phases.get("compile_total_ms"),
                "execute_ms": phases.get("execute_ms"),
                "compile_share_percent": phases.get("compile_share_percent"),
                "execute_share_percent": phases.get("execute_share_percent"),
                "timing_warnings": item.get("timing_warnings", []),
                "phases": timings,
            }
        )
    return rows


def top_rows(rows: list[dict[str, Any]], key: str, limit: int = 10) -> list[dict[str, Any]]:
    return sorted(
        [row for row in rows if isinstance(row.get(key), (int, float))],
        key=lambda row: float(row.get(key) or 0.0),
        reverse=True,
    )[:limit]


def top_phase_rows(rows: list[dict[str, Any]], phase: str, limit: int = 10) -> list[dict[str, Any]]:
    ranked: list[dict[str, Any]] = []
    for row in rows:
        timings = row.get("phases")
        if not isinstance(timings, dict):
            continue
        value = phase_value(timings, phase)
        if value is None:
            continue
        ranked.append({**row, phase: value})
    return sorted(ranked, key=lambda row: float(row.get(phase) or 0.0), reverse=True)[:limit]


def startup_rows(startup: dict[str, Any] | None) -> list[dict[str, Any]]:
    if startup is None:
        return []
    rows = startup.get("rows")
    if not isinstance(rows, list):
        return []
    return [row for row in rows if isinstance(row, dict)]


def build_summary(
    benchmark: dict[str, Any],
    app_flow: dict[str, Any],
    startup: dict[str, Any] | None,
    benchmark_path: Path,
    app_flow_path: Path,
    startup_path: Path | None,
) -> dict[str, Any]:
    benchmark_rows = benchmark_decisions(benchmark)
    app_flow_rows = app_flow_decisions(app_flow)
    startup_report_rows = startup_rows(startup)
    all_rows = [*benchmark_rows, *app_flow_rows]
    failures = [
        row
        for row in all_rows
        if row.get("status") not in (None, "pass") or row.get("correctness") == "fail"
    ]
    warnings = [
        warning
        for row in all_rows
        for warning in row.get("timing_warnings", [])
        if isinstance(warning, str)
    ]
    return {
        "schema_version": 1,
        "status": "fail" if failures else "pass",
        "inputs": {
            "benchmark": rel(benchmark_path),
            "app_flow": rel(app_flow_path),
            "startup": rel(startup_path) if startup_path is not None else None,
        },
        "benchmark_row_count": len(benchmark_rows),
        "app_flow_row_count": len(app_flow_rows),
        "startup_row_count": len(startup_report_rows),
        "failure_count": len(failures),
        "timing_warnings": warnings,
        "top_compile": top_rows(all_rows, "compile_total_ms"),
        "top_execute": top_rows(all_rows, "execute_ms"),
        "top_startup": top_rows(benchmark_rows, "startup_external_ms"),
        "startup_rows": startup_report_rows,
        "compile_phase_rankings": {
            phase: top_phase_rows(all_rows, phase)
            for phase in (
                "source_read_ms",
                "frontend_analyze_ms",
                "ir_lower_ms",
                "ir_verify_ms",
                "optimizer_ms",
                "cache_prepare_ms",
                "cache_load_ms",
                "cache_store_ms",
            )
        },
        "cache_counts": [
            {
                "scenario": row.get("scenario"),
                "row": row.get("row", "benchmark"),
                "cache_hit": count_value(row.get("phases", {}), "cache_hit"),
                "cache_miss": count_value(row.get("phases", {}), "cache_miss"),
                "cache_wrote": count_value(row.get("phases", {}), "cache_wrote"),
                "includes": count_value(row.get("phases", {}), "includes"),
            }
            for row in all_rows
            if isinstance(row.get("phases"), dict)
        ],
        "benchmark_rows": benchmark_rows,
        "app_flow_rows": app_flow_rows,
    }


def render_markdown(summary: dict[str, Any]) -> str:
    lines = [
        "# Performance Decision Baseline",
        "",
        "This local report ranks startup, compile, and execute costs from the",
        "benchmark smoke and app-flow matrix timing sidecars.",
        "",
        "## Inputs",
        "",
        "| Input | Path |",
        "| --- | --- |",
        f"| Benchmark | `{summary['inputs']['benchmark']}` |",
        f"| App flow | `{summary['inputs']['app_flow']}` |",
        f"| Startup | `{summary['inputs'].get('startup') or 'not run'}` |",
        "",
        "## Summary",
        "",
        "| Field | Value |",
        "| --- | --- |",
        f"| Status | `{summary['status']}` |",
        f"| Benchmark rows | {summary['benchmark_row_count']} |",
        f"| App-flow rows | {summary['app_flow_row_count']} |",
        f"| Startup rows | {summary['startup_row_count']} |",
        f"| Failures | {summary['failure_count']} |",
        f"| Timing warnings | {len(summary['timing_warnings'])} |",
        "",
    ]
    for title, key, rows in (
        ("Compile Priority", "compile_total_ms", summary["top_compile"]),
        ("Execute Priority", "execute_ms", summary["top_execute"]),
        ("Startup Priority", "startup_external_ms", summary["top_startup"]),
    ):
        lines.extend([f"## {title}", ""])
        if not rows:
            lines.extend(["No rows available.", ""])
            continue
        lines.extend(
            [
                "| Scenario | Row | ms | Status |",
                "| --- | --- | --- | --- |",
            ]
        )
        for row in rows:
            row_label = row.get("row", "benchmark")
            lines.append(
                f"| `{row.get('scenario', '<unknown>')}` | `{row_label}` | "
                f"{float(row.get(key) or 0.0):.3f} | `{row.get('status', '-')}` |"
            )
        lines.append("")
    lines.extend(["## Startup Matrix", ""])
    if summary["startup_rows"]:
        lines.extend(
            [
                "| Row | Profile | External ms | Internal ms | Startup ms | Binary bytes |",
                "| --- | --- | --- | --- | --- | --- |",
            ]
        )
        for row in summary["startup_rows"]:
            internal = row.get("internal_total_ms")
            startup = row.get("startup_external_ms")
            lines.append(
                f"| `{row.get('id', '<unknown>')}` | `{row.get('profile', '-')}` | "
                f"{float(row.get('external_wall_ms') or 0.0):.3f} | "
                f"{'n/a' if internal is None else f'{float(internal):.3f}'} | "
                f"{'n/a' if startup is None else f'{float(startup):.3f}'} | "
                f"{row.get('binary_size_bytes') or 'n/a'} |"
            )
    else:
        lines.append("Startup matrix was not available.")
    lines.append("")
    lines.extend(["## Compile And Cache Phase Rankings", ""])
    for phase, rows in summary["compile_phase_rankings"].items():
        lines.extend([f"### `{phase}`", ""])
        if not rows:
            lines.extend(["No rows available.", ""])
            continue
        lines.extend(["| Scenario | Row | ms |", "| --- | --- | --- |"])
        for row in rows:
            lines.append(
                f"| `{row.get('scenario', '<unknown>')}` | `{row.get('row', 'benchmark')}` | "
                f"{float(row.get(phase) or 0.0):.3f} |"
            )
        lines.append("")
    lines.extend(["## Cache Hit/Miss Counts", ""])
    cache_rows = [
        row
        for row in summary["cache_counts"]
        if any(row.get(key) is not None for key in ("cache_hit", "cache_miss", "cache_wrote", "includes"))
    ]
    if cache_rows:
        lines.extend(
            [
                "| Scenario | Row | Hit | Miss | Wrote | Includes |",
                "| --- | --- | --- | --- | --- | --- |",
            ]
        )
        for row in cache_rows:
            lines.append(
                f"| `{row.get('scenario', '<unknown>')}` | `{row.get('row', 'benchmark')}` | "
                f"{row.get('cache_hit', 'n/a')} | {row.get('cache_miss', 'n/a')} | "
                f"{row.get('cache_wrote', 'n/a')} | {row.get('includes', 'n/a')} |"
            )
    else:
        lines.append("No cache count rows were available.")
    lines.append("")
    if summary["timing_warnings"]:
        lines.extend(["## Timing Warnings", ""])
        lines.extend(f"- {warning}" for warning in summary["timing_warnings"])
        lines.append("")
    return "\n".join(lines)


def write_summary(summary: dict[str, Any], out_dir: Path) -> None:
    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / "summary.json").write_text(
        json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    (out_dir / "summary.md").write_text(render_markdown(summary), encoding="utf-8")


def run_self_test() -> int:
    benchmark = {
        "measurements": [
            {
                "scenario": {"id": "performance.perf_smoke.rust-vm.loop"},
                "engine": "rust-vm",
                "status": "pass",
                "metrics": [
                    {"name": "compile_total_ms", "value": 2.0},
                    {"name": "execute_ms", "value": 3.0},
                    {"name": "startup_external_ms", "value": 1.0},
                ],
            }
        ]
    }
    app_flow = {
        "rows": [
            {
                "scenario": "routing",
                "row": "phrust-default",
                "status": "pass",
                "correctness": "pass",
                "phase_summary": {"compile_total_ms": 4.0, "execute_ms": 5.0},
            }
        ]
    }
    startup = {
        "rows": [
            {
                "id": "debug-help",
                "profile": "debug",
                "external_wall_ms": 1.0,
                "internal_total_ms": None,
                "startup_external_ms": None,
                "binary_size_bytes": 100,
            }
        ]
    }
    benchmark["measurements"][0]["phase_timings"] = {
        "phases": {"frontend_analyze_ms": 2.0},
        "counts": {"cache_hit": 0, "cache_miss": 1},
    }
    summary = build_summary(
        benchmark,
        app_flow,
        startup,
        Path("bench.json"),
        Path("app.json"),
        Path("startup.json"),
    )
    markdown = render_markdown(summary)
    assert summary["status"] == "pass"
    assert summary["top_compile"][0]["scenario"] == "routing"
    assert summary["startup_row_count"] == 1
    assert summary["compile_phase_rankings"]["frontend_analyze_ms"][0]["frontend_analyze_ms"] == 2.0
    assert "Performance Decision Baseline" in markdown
    assert "Compile Priority" in markdown
    print("[pass] decision_baseline self-test")
    return 0


def run_baseline(args: argparse.Namespace) -> int:
    if args.timeout <= 0:
        raise SystemExit("--timeout must be positive")
    out_dir = args.out_dir if args.out_dir.is_absolute() else ROOT / args.out_dir
    benchmark_path = out_dir / "benchmark-summary.json"
    app_flow_summary_path = out_dir / "app-flow-summary.json"
    app_flow_dir = out_dir / "app-flows"
    app_flow_path = app_flow_dir / "matrix.json"
    startup_path = out_dir / "startup-summary.json"

    run_command(["cargo", "build", "-p", "php_vm_cli", "--bin", "php-vm"])
    run_command(
        [
            str(ROOT / "scripts/performance/bench_matrix.py"),
            "--engine",
            str(args.engine),
            "--out",
            str(benchmark_path),
            "--repetitions",
            str(max(args.benchmark_repetitions, 1)),
            "--warmups",
            str(args.benchmark_warmups),
            "--timeout",
            str(args.timeout),
        ]
    )
    app_flow_command = [
        str(ROOT / "scripts/performance/app_flow_matrix.py"),
        "--engine",
        str(args.engine),
        "--out-dir",
        str(app_flow_dir),
        "--iterations",
        str(max(args.app_flow_iterations, 1)),
        "--warmups",
        str(args.app_flow_warmups),
        "--scale",
        str(max(args.app_flow_scale, 1)),
        "--timeout",
        str(args.timeout),
        "--allow-missing-reference",
    ]
    if args.smoke:
        app_flow_command.append("--smoke")
    run_command(app_flow_command)
    shutil.copyfile(app_flow_path, app_flow_summary_path)
    run_command(
        [
            str(ROOT / "scripts/performance/startup_matrix.py"),
            "--debug-engine",
            str(args.engine),
            "--release-engine",
            str(ROOT / "target/release/php-vm"),
            "--out",
            str(startup_path),
            "--iterations",
            "1",
            "--warmups",
            "0",
            "--timeout",
            str(args.timeout),
        ]
    )

    summary = build_summary(
        load_json(benchmark_path),
        load_json(app_flow_summary_path),
        load_json(startup_path) if startup_path.is_file() else None,
        benchmark_path,
        app_flow_summary_path,
        startup_path if startup_path.is_file() else None,
    )
    write_summary(summary, out_dir)
    print(f"[pass] performance decision baseline wrote {rel(out_dir / 'summary.json')}")
    return 0 if summary["status"] == "pass" else 1


def main() -> int:
    args = parse_args()
    if args.self_test:
        return run_self_test()
    return run_baseline(args)


if __name__ == "__main__":
    sys.exit(main())
