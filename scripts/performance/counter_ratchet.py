#!/usr/bin/env python3
"""Normalize VM counters and optional instruction counts into ratchet scenarios."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

from ratchet_schema import ROOT, make_report, rel, render_report_markdown, validate_report, write_json


COUNTER_MAP = {
    "instructions_executed": "counter.instructions_executed",
    "bytecode_instructions_executed": "counter.bytecode_instructions_executed",
    "function_calls": "counter.function_calls",
    "method_calls": "counter.method_calls",
    "internal_function_dispatches": "counter.internal_function_dispatches",
    "internal_function_dispatch_cache_hits": "counter.internal_function_dispatch_cache_hits",
    "inline_cache_hits": "counter.inline_cache_hits",
    "inline_cache_misses": "counter.inline_cache_misses",
    "quickening_specialized": "counter.quickening_specialized",
    "quickening_guard_hits": "counter.quickening_guard_hits",
    "quickening_guard_misses": "counter.quickening_guard_misses",
    "output_fast_appends": "counter.output_fast_appends",
    "string_concat_fast_path_hits": "counter.string_concat_fast_path_hits",
    "packed_fetch_fast_hits": "counter.packed_fetch_fast_hits",
    "array_fast_path_fallbacks": "counter.array_fast_path_fallbacks",
    "native_executions": "counter.native_executions",
    "native_fallbacks": "counter.native_fallbacks",
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--benchmark", type=Path, default=ROOT / "target/performance/benchmark-smoke.json")
    parser.add_argument("--app-flow", type=Path, default=ROOT / "target/performance/ratchet/app-flow/current.json")
    parser.add_argument("--server", type=Path, default=ROOT / "target/performance/ratchet/server/current.json")
    parser.add_argument("--callgrind", type=Path, default=ROOT / "target/performance/callgrind/summary.json")
    parser.add_argument("--out", type=Path, default=ROOT / "target/performance/ratchet/counters/current.json")
    parser.add_argument("--markdown-out", type=Path)
    parser.add_argument("--self-test", action="store_true")
    return parser.parse_args()


def load_optional(path: Path) -> tuple[dict[str, Any] | None, str | None]:
    path = path if path.is_absolute() else ROOT / path
    if not path.is_file():
        return None, f"missing optional input: {rel(path)}"
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        return None, f"malformed optional input: {rel(path)}: {error}"
    return data if isinstance(data, dict) else None, None


def normalize_counter_metrics(counters: dict[str, Any]) -> dict[str, float]:
    metrics: dict[str, float] = {}
    for source, target in COUNTER_MAP.items():
        value = counters.get(source)
        if isinstance(value, (int, float)):
            metrics[target] = float(value)
    for key, value in counters.items():
        if isinstance(key, str) and isinstance(value, (int, float)) and (
            "fallback" in key or "deopt" in key or "guard" in key
        ):
            metrics.setdefault(f"counter.{key}", float(value))
    if "counter.instructions_executed" in metrics:
        metrics["instructions_executed"] = metrics["counter.instructions_executed"]
    return metrics


def benchmark_scenarios(report: dict[str, Any]) -> list[dict[str, Any]]:
    rows = []
    for item in report.get("measurements", []):
        if not isinstance(item, dict) or not isinstance(item.get("vm_counters"), dict):
            continue
        scenario = item.get("scenario") if isinstance(item.get("scenario"), dict) else {}
        scenario_id = scenario.get("id", "benchmark.unknown")
        metrics = normalize_counter_metrics(item["vm_counters"])
        rows.append(
            {
                "id": f"counter.{scenario_id}",
                "group": "instruction",
                "kind": "counter",
                "correctness": "pass" if item.get("status") == "pass" else "fail",
                "metrics": metrics,
                "phase_metrics": {},
                "counter_highlights": {k.removeprefix("counter."): int(v) for k, v in metrics.items() if k.startswith("counter.")},
                "artifacts": {"source": "benchmark"},
            }
        )
    return rows


def ratchet_counter_scenarios(report: dict[str, Any], source: str) -> list[dict[str, Any]]:
    rows = []
    for item in report.get("scenarios", []):
        if not isinstance(item, dict):
            continue
        counters = item.get("counter_highlights") if isinstance(item.get("counter_highlights"), dict) else {}
        metrics = normalize_counter_metrics(counters)
        if not metrics:
            continue
        rows.append(
            {
                "id": f"counter.{item.get('id')}",
                "group": "instruction",
                "kind": "counter",
                "correctness": item.get("correctness", "skip"),
                "metrics": metrics,
                "phase_metrics": {},
                "counter_highlights": counters,
                "artifacts": {"source": source},
            }
        )
    return rows


def callgrind_scenarios(report: dict[str, Any]) -> list[dict[str, Any]]:
    rows = []
    for item in report.get("measurements", []):
        if not isinstance(item, dict):
            continue
        instructions = item.get("instructions")
        name = item.get("scenario") or item.get("name")
        if isinstance(instructions, int) and isinstance(name, str):
            rows.append(
                {
                    "id": f"instruction.callgrind.{name}",
                    "group": "instruction",
                    "kind": "counter",
                    "correctness": "pass",
                    "metrics": {"instruction_count.callgrind": float(instructions)},
                    "phase_metrics": {},
                    "counter_highlights": {},
                    "artifacts": {"source": "callgrind"},
                }
            )
    return rows


def build(args: argparse.Namespace) -> dict[str, Any]:
    scenarios: list[dict[str, Any]] = []
    failures: list[str] = []
    skipped: list[str] = []
    benchmark, reason = load_optional(args.benchmark)
    if benchmark is not None:
        scenarios.extend(benchmark_scenarios(benchmark))
    elif reason:
        skipped.append(reason)
    for path, label in ((args.app_flow, "app-flow"), (args.server, "server")):
        data, reason = load_optional(path)
        if data is not None:
            scenarios.extend(ratchet_counter_scenarios(data, label))
        elif reason:
            skipped.append(reason)
    callgrind, reason = load_optional(args.callgrind)
    if callgrind is not None:
        scenarios.extend(callgrind_scenarios(callgrind))
    elif reason:
        skipped.append(reason)
    if not scenarios:
        scenarios.append(
            {
                "id": "counter.measurement-gap",
                "group": "instruction",
                "kind": "counter",
                "correctness": "skip",
                "metrics": {},
                "phase_metrics": {},
                "counter_highlights": {},
                "artifacts": {"skipped": skipped},
            }
        )
    report = make_report(
        run_id="counter-ratchet",
        created_by="counter_ratchet.py",
        scenarios=scenarios,
        failures=failures,
    )
    report["skipped_inputs"] = skipped
    return report


def render_counter_markdown(report: dict[str, Any]) -> str:
    lines = [
        render_report_markdown(report, "Counter and Instruction Ratchet").rstrip(),
        "",
        "## Low-Hanging Fruit",
        "",
    ]
    scenarios = report.get("scenarios", [])
    heavy = sorted(
        [item for item in scenarios if isinstance(item, dict)],
        key=lambda item: float(item.get("metrics", {}).get("counter.instructions_executed", item.get("metrics", {}).get("instructions_executed", 0.0))),
        reverse=True,
    )[:5]
    if heavy:
        lines.extend(f"- Inspect `{item.get('id')}` for high instruction or fallback counters." for item in heavy)
    else:
        lines.append("- Run benchmark/app-flow ratchet first to collect counter evidence.")
    if report.get("skipped_inputs"):
        lines.extend(["", "## Skipped Inputs", ""])
        lines.extend(f"- {item}" for item in report["skipped_inputs"])
    return "\n".join(lines) + "\n"


def run_self_test() -> int:
    metrics = normalize_counter_metrics({"instructions_executed": 10, "quickening_guard_misses": 2})
    assert metrics["counter.instructions_executed"] == 10
    assert metrics["counter.quickening_guard_misses"] == 2
    print("[pass] counter_ratchet self-test")
    return 0


def main() -> int:
    args = parse_args()
    if args.self_test:
        return run_self_test()
    report = build(args)
    errors = validate_report(report)
    if errors:
        raise SystemExit("; ".join(errors))
    out = args.out if args.out.is_absolute() else ROOT / args.out
    markdown = args.markdown_out or out.with_suffix(".md")
    markdown = markdown if markdown.is_absolute() else ROOT / markdown
    write_json(out, report)
    markdown.parent.mkdir(parents=True, exist_ok=True)
    markdown.write_text(render_counter_markdown(report), encoding="utf-8")
    print(f"[pass] counter ratchet wrote {rel(out)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
