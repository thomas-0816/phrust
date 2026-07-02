#!/usr/bin/env python3
"""Compare two Phrust performance ratchet reports."""

from __future__ import annotations

import argparse
import json
import math
import os
import sys
from pathlib import Path
from typing import Any

from ratchet_schema import ROOT, load_json, rel, validate_report, write_json


DEFAULT_BUDGETS = ROOT / "configs/performance/ratchet-budgets.json"
DEFAULT_BASELINE = ROOT / "target/performance/ratchet/baseline.json"
DEFAULT_CURRENT = ROOT / "target/performance/ratchet/current.json"
DEFAULT_MARKDOWN = ROOT / "target/performance/ratchet/compare.md"
DEFAULT_JSON = ROOT / "target/performance/ratchet/compare.json"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("baseline", nargs="?", type=Path, default=DEFAULT_BASELINE)
    parser.add_argument("current", nargs="?", type=Path, default=DEFAULT_CURRENT)
    parser.add_argument("--budgets", type=Path, default=DEFAULT_BUDGETS)
    parser.add_argument("--out", type=Path, default=DEFAULT_MARKDOWN)
    parser.add_argument("--json-out", type=Path, default=DEFAULT_JSON)
    parser.add_argument("--strict", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    return parser.parse_args()


def percent_change(baseline: float, current: float) -> float:
    if baseline == 0.0:
        if current == 0.0:
            return 0.0
        return math.inf if current > 0.0 else -math.inf
    return ((current - baseline) / abs(baseline)) * 100.0


def is_regression(change: float, lower_is_better: bool) -> bool:
    return change > 0.0 if lower_is_better else change < 0.0


def is_win(change: float, lower_is_better: bool) -> bool:
    return change < 0.0 if lower_is_better else change > 0.0


def scenario_map(report: dict[str, Any]) -> dict[str, dict[str, Any]]:
    scenarios = {}
    for item in report.get("scenarios", []):
        if isinstance(item, dict) and isinstance(item.get("id"), str):
            scenarios[item["id"]] = item
    return scenarios


def metric_policy(budgets: dict[str, Any], metric: str) -> dict[str, Any]:
    metrics = budgets.get("metrics") if isinstance(budgets.get("metrics"), dict) else {}
    policy = metrics.get(metric) if isinstance(metrics.get(metric), dict) else {}
    return {
        "lower_is_better": bool(policy.get("lower_is_better", True)),
        "severity": str(policy.get("severity", "warn")),
    }


def classify_change(
    *,
    baseline: float,
    current: float,
    metric: str,
    budgets: dict[str, Any],
    strict: bool,
) -> tuple[str, float]:
    policy = metric_policy(budgets, metric)
    change = percent_change(baseline, current)
    default_policy = budgets.get("default_policy", {})
    warn_pct = float(default_policy.get("wall_clock_regression_percent_warn", 15.0))
    fail_pct = float(default_policy.get("wall_clock_regression_percent_fail", 35.0))
    counter_fail_pct = float(default_policy.get("counter_regression_percent_fail", 25.0))
    lower_is_better = bool(policy["lower_is_better"])
    severity = str(policy["severity"])
    if is_win(change, lower_is_better) and abs(change) >= 1.0:
        return "win", change
    if not is_regression(change, lower_is_better) or abs(change) < warn_pct:
        return "neutral", change
    fail_threshold = counter_fail_pct if metric.startswith(("counter.", "instructions")) else fail_pct
    if severity == "fail" and abs(change) >= fail_threshold:
        return "hard_regression", change
    if severity == "fail_when_strict" and strict and abs(change) >= fail_threshold:
        return "hard_regression", change
    if strict and abs(change) >= fail_pct:
        return "hard_regression", change
    return "warning_regression", change


def compare_reports(
    baseline: dict[str, Any],
    current: dict[str, Any],
    budgets: dict[str, Any],
    strict: bool,
) -> tuple[dict[str, Any], int]:
    baseline_errors = validate_report(baseline)
    current_errors = validate_report(current)
    if baseline_errors or current_errors:
        return (
            {
                "schema_version": 1,
                "status": "invalid",
                "failures": [
                    *(f"baseline: {error}" for error in baseline_errors),
                    *(f"current: {error}" for error in current_errors),
                ],
                "comparisons": [],
            },
            1,
        )

    baseline_scenarios = scenario_map(baseline)
    current_scenarios = scenario_map(current)
    comparisons: list[dict[str, Any]] = []
    failures: list[str] = []
    warning_count = 0
    win_count = 0
    hard_count = 0

    for scenario_id in sorted(set(baseline_scenarios) & set(current_scenarios)):
        before = baseline_scenarios[scenario_id]
        after = current_scenarios[scenario_id]
        before_correct = before.get("correctness")
        after_correct = after.get("correctness")
        if before_correct == "pass" and after_correct == "fail":
            hard_count += 1
            failures.append(f"{scenario_id}: correctness regressed from pass to fail")
        before_metrics = before.get("metrics") if isinstance(before.get("metrics"), dict) else {}
        after_metrics = after.get("metrics") if isinstance(after.get("metrics"), dict) else {}
        for metric in sorted(set(before_metrics) & set(after_metrics)):
            before_value = before_metrics[metric]
            after_value = after_metrics[metric]
            if not isinstance(before_value, (int, float)) or not isinstance(after_value, (int, float)):
                continue
            classification, change = classify_change(
                baseline=float(before_value),
                current=float(after_value),
                metric=metric,
                budgets=budgets,
                strict=strict,
            )
            if classification == "hard_regression":
                hard_count += 1
                failures.append(f"{scenario_id} {metric}: hard regression {change:+.2f}%")
            elif classification == "warning_regression":
                warning_count += 1
            elif classification == "win":
                win_count += 1
            comparisons.append(
                {
                    "scenario_id": scenario_id,
                    "metric": metric,
                    "baseline": float(before_value),
                    "current": float(after_value),
                    "change_percent": change,
                    "classification": classification,
                    "lower_is_better": metric_policy(budgets, metric)["lower_is_better"],
                }
            )

    missing = sorted(set(baseline_scenarios) - set(current_scenarios))
    added = sorted(set(current_scenarios) - set(baseline_scenarios))
    cost_centers = sorted(
        (
            {
                "scenario_id": item["id"],
                "external_wall_ms.p50": float(item.get("metrics", {}).get("external_wall_ms.p50", 0.0)),
            }
            for item in current_scenarios.values()
        ),
        key=lambda item: item["external_wall_ms.p50"],
        reverse=True,
    )[:10]
    result = {
        "schema_version": 1,
        "status": "fail" if hard_count else "pass",
        "strict": strict,
        "summary": {
            "baseline_scenarios": len(baseline_scenarios),
            "current_scenarios": len(current_scenarios),
            "comparable_metrics": len(comparisons),
            "wins": win_count,
            "warning_regressions": warning_count,
            "hard_regressions": hard_count,
            "missing_in_current": missing,
            "added_in_current": added,
        },
        "comparisons": comparisons,
        "hard_regressions": [row for row in comparisons if row["classification"] == "hard_regression"],
        "warning_regressions": [row for row in comparisons if row["classification"] == "warning_regression"],
        "wins": [row for row in comparisons if row["classification"] == "win"],
        "cost_centers": cost_centers,
        "failures": failures,
        "next_recommended_category": next_category(comparisons, cost_centers, hard_count),
    }
    return result, 2 if hard_count else 0


def next_category(comparisons: list[dict[str, Any]], cost_centers: list[dict[str, Any]], hard_count: int) -> str:
    if hard_count:
        return "correctness-blocker"
    regressions = [row for row in comparisons if row["classification"].endswith("regression")]
    if regressions:
        worst = max(regressions, key=lambda row: abs(float(row["change_percent"])))
        metric = str(worst["metric"])
    elif cost_centers:
        metric = "external_wall_ms.p50"
    else:
        return "measurement-gap"
    if "startup" in metric:
        return "startup"
    if "compile" in metric or metric.startswith("phase:"):
        return "compile-transpile"
    if "execute" in metric or "instruction" in metric or metric.startswith("counter."):
        return "vm-execution"
    if "ttfb" in metric or "request_total" in metric:
        return "server-responsiveness"
    return "measurement-gap"


def format_pct(value: float) -> str:
    if math.isinf(value):
        return "+inf%" if value > 0 else "-inf%"
    return f"{value:+.2f}%"


def render_markdown(result: dict[str, Any]) -> str:
    summary = result["summary"]
    lines = [
        "# Performance Ratchet Comparison",
        "",
        "| Field | Value |",
        "| --- | ---: |",
        f"| Baseline scenarios | {summary['baseline_scenarios']} |",
        f"| Current scenarios | {summary['current_scenarios']} |",
        f"| Comparable metrics | {summary['comparable_metrics']} |",
        f"| Wins | {summary['wins']} |",
        f"| Warning regressions | {summary['warning_regressions']} |",
        f"| Hard regressions | {summary['hard_regressions']} |",
        "",
        f"Next recommended investigation category: `{result['next_recommended_category']}`",
        "",
        "## Worst Regressions",
        "",
        "| Scenario | Metric | Baseline | Current | Change | Classification |",
        "| --- | --- | ---: | ---: | ---: | --- |",
    ]
    regressions = sorted(
        [*result["hard_regressions"], *result["warning_regressions"]],
        key=lambda row: abs(float(row["change_percent"])),
        reverse=True,
    )[:20]
    for row in regressions:
        lines.append(
            f"| `{row['scenario_id']}` | `{row['metric']}` | {row['baseline']:.6g} | "
            f"{row['current']:.6g} | {format_pct(row['change_percent'])} | "
            f"`{row['classification']}` |"
        )
    lines.extend(["", "## Biggest Wins", "", "| Scenario | Metric | Baseline | Current | Change |", "| --- | --- | ---: | ---: | ---: |"])
    for row in sorted(result["wins"], key=lambda row: abs(float(row["change_percent"])), reverse=True)[:20]:
        lines.append(
            f"| `{row['scenario_id']}` | `{row['metric']}` | {row['baseline']:.6g} | "
            f"{row['current']:.6g} | {format_pct(row['change_percent'])} |"
        )
    lines.extend(["", "## Biggest Absolute Cost Centers", "", "| Scenario | p50 external ms |", "| --- | ---: |"])
    for row in result["cost_centers"]:
        lines.append(f"| `{row['scenario_id']}` | {row['external_wall_ms.p50']:.3f} |")
    if summary["missing_in_current"]:
        lines.extend(["", "## Missing Scenarios", ""])
        lines.extend(f"- `{item}`" for item in summary["missing_in_current"])
    if summary["added_in_current"]:
        lines.extend(["", "## Added Scenarios", ""])
        lines.extend(f"- `{item}`" for item in summary["added_in_current"])
    if result["failures"]:
        lines.extend(["", "## Failures", ""])
        lines.extend(f"- {failure}" for failure in result["failures"])
    return "\n".join(lines) + "\n"


def write_outputs(result: dict[str, Any], markdown_path: Path, json_path: Path) -> None:
    markdown_path.parent.mkdir(parents=True, exist_ok=True)
    markdown_path.write_text(render_markdown(result), encoding="utf-8")
    write_json(json_path, result)


def synthetic_report(value: float, correctness: str = "pass") -> dict[str, Any]:
    return {
        "schema_version": 1,
        "run_id": "synthetic",
        "created_by": "self-test",
        "environment": {"git_commit": "test", "platform": "test", "profile": "debug", "reference_php": "skipped"},
        "scenarios": [
            {
                "id": "cli.synthetic",
                "group": "cli",
                "kind": "startup",
                "correctness": correctness,
                "metrics": {"external_wall_ms.p50": value, "counter.instructions_executed": value},
                "phase_metrics": {},
                "counter_highlights": {},
                "artifacts": {},
            }
        ],
        "failures": [],
    }


def run_self_test() -> int:
    budgets = load_json(DEFAULT_BUDGETS)
    result, code = compare_reports(synthetic_report(100), synthetic_report(80), budgets, False)
    assert code == 0 and result["wins"]
    result, code = compare_reports(synthetic_report(100), synthetic_report(116), budgets, False)
    assert code == 0 and result["warning_regressions"]
    result, code = compare_reports(synthetic_report(100), synthetic_report(140), budgets, True)
    assert code == 2 and result["hard_regressions"]
    result, code = compare_reports(synthetic_report(100), synthetic_report(80, "fail"), budgets, False)
    assert code == 2 and result["failures"]
    missing_current = synthetic_report(100)
    missing_current["scenarios"] = []
    result, code = compare_reports(synthetic_report(100), missing_current, budgets, False)
    assert code == 0 and result["summary"]["missing_in_current"]
    result, code = compare_reports({"schema_version": 1}, synthetic_report(100), budgets, False)
    assert code == 1 and result["status"] == "invalid"
    print("[pass] ratchet_compare self-test")
    return 0


def main() -> int:
    args = parse_args()
    if args.self_test:
        return run_self_test()
    strict = args.strict or os.getenv("PHRUST_RATCHET_STRICT") == "1"
    budgets = load_json(args.budgets)
    result, code = compare_reports(load_json(args.baseline), load_json(args.current), budgets, strict)
    write_outputs(result, args.out, args.json_out)
    print(f"[{'fail' if code == 2 else 'pass'}] ratchet comparison wrote {rel(args.json_out)}")
    return code


if __name__ == "__main__":
    sys.exit(main())
