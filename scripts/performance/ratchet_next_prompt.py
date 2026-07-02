#!/usr/bin/env python3
"""Generate the next focused performance prompt from ratchet evidence."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

from ratchet_schema import ROOT, load_json, rel


CATEGORIES = {
    "startup",
    "compile-transpile",
    "include-cache",
    "vm-execution",
    "server-responsiveness",
    "counter-instruction-regression",
    "correctness-blocker",
    "measurement-gap",
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--ratchet", action="append", type=Path, default=[])
    parser.add_argument("--compare", type=Path)
    parser.add_argument("--out", type=Path, default=ROOT / "target/performance/ratchet/next-performance-prompt.md")
    parser.add_argument("--self-test", action="store_true")
    return parser.parse_args()


def load_optional(path: Path | None) -> dict[str, Any] | None:
    if path is None:
        return None
    path = path if path.is_absolute() else ROOT / path
    if not path.is_file():
        return None
    return load_json(path)


def scenario_candidates(reports: list[dict[str, Any]]) -> list[dict[str, Any]]:
    rows = []
    for report in reports:
        for item in report.get("scenarios", []):
            if isinstance(item, dict):
                rows.append(item)
    return rows


def classify(reports: list[dict[str, Any]], compare: dict[str, Any] | None) -> tuple[str, dict[str, Any], str]:
    scenarios = scenario_candidates(reports)
    failing = [item for item in scenarios if item.get("correctness") == "fail"]
    if failing:
        return "correctness-blocker", failing[0], "correctness failure outranks speed work"
    if not scenarios:
        return "measurement-gap", {}, "no ratchet reports were available"
    if compare is not None:
        hard = compare.get("hard_regressions")
        if isinstance(hard, list) and hard:
            row = hard[0]
            metric = str(row.get("metric", ""))
            if metric.startswith("counter.") or "instruction" in metric:
                return "counter-instruction-regression", row, "deterministic counter regression"
    scored: list[tuple[float, str, dict[str, Any], str]] = []
    for item in scenarios:
        metrics = item.get("metrics") if isinstance(item.get("metrics"), dict) else {}
        group = str(item.get("group", ""))
        external = float(metrics.get("external_wall_ms.p50", metrics.get("request_total_ms.p50", 0.0)))
        startup = float(metrics.get("startup_external_ms.p50", 0.0))
        compile_ms = float(metrics.get("compile_total_ms.p50", 0.0))
        execute = float(metrics.get("execute_ms.p50", 0.0))
        ttfb = float(metrics.get("ttfb_ms.p95", metrics.get("ttfb_ms.p50", 0.0)))
        counters = item.get("counter_highlights") if isinstance(item.get("counter_highlights"), dict) else {}
        instruction = float(
            metrics.get(
                "counter.instructions_executed",
                metrics.get("instructions_executed", counters.get("instructions_executed", 0.0)),
            )
        )
        if group == "server" and ttfb > 0:
            scored.append((ttfb, "server-responsiveness", item, "server TTFB or tail latency dominates"))
        if external > 0 and startup / external >= 0.35:
            scored.append((startup, "startup", item, "startup is a large share of external wall time"))
        if compile_ms >= execute and compile_ms > 0:
            category = "include-cache" if any("cache" in key for key in metrics) else "compile-transpile"
            scored.append((compile_ms, category, item, "compile/transpile phase dominates"))
        if execute > compile_ms and execute > 0:
            scored.append((execute, "vm-execution", item, "execution phase dominates"))
        if instruction > 0:
            scored.append((instruction / 1000.0, "vm-execution", item, "instruction counters are high"))
    if not scored:
        return "measurement-gap", scenarios[0], "available reports lack timing or counter metrics"
    scored.sort(key=lambda row: row[0], reverse=True)
    _, category, item, reason = scored[0]
    return category, item, reason


def prompt(category: str, evidence: dict[str, Any], reason: str, inputs: list[Path], compare: Path | None) -> str:
    metrics = evidence.get("metrics") if isinstance(evidence.get("metrics"), dict) else {}
    counters = evidence.get("counter_highlights") if isinstance(evidence.get("counter_highlights"), dict) else {}
    scenario = evidence.get("scenario_id") or evidence.get("id") or "unknown"
    metric_lines = []
    for key in (
        "external_wall_ms.p50",
        "startup_external_ms.p50",
        "compile_total_ms.p50",
        "execute_ms.p50",
        "ttfb_ms.p95",
        "request_total_ms.p95",
        "counter.instructions_executed",
    ):
        if key in metrics:
            metric_lines.append(f"- {key}: {metrics[key]}")
    counter_lines = [f"- {key}: {value}" for key, value in list(counters.items())[:8]]
    artifact_lines = [f"- {rel(path if path.is_absolute() else ROOT / path)}" for path in inputs]
    if compare is not None:
        artifact_lines.append(f"- {rel(compare if compare.is_absolute() else ROOT / compare)}")
    if not metric_lines:
        metric_lines.append("- No decisive metric was present; improve measurement first.")
    if not counter_lines:
        counter_lines.append("- No counter highlights were present.")
    return f"""# Codex Performance Task: {category}

## Problem evidence

- Scenario: `{scenario}`
- Category reason: {reason}
{chr(10).join(metric_lines)}

## Relevant counters

{chr(10).join(counter_lines)}

## Artifacts

{chr(10).join(artifact_lines) if artifact_lines else "- No artifact inputs were available."}

## Hypothesis

The next highest-value task is `{category}` because the ratchet evidence points there. Keep the hypothesis narrow and update it only from measured artifacts.

## Required implementation constraints

- Preserve stdout, stderr, exit status, diagnostics, PHP-visible behavior, and request semantics.
- Do not globally disable fast paths to make one scenario faster.
- Do not claim a speedup without before/after artifacts under `target/performance/`.
- Keep raw measurements under `target/performance/` and do not commit them.

## Steps

1. Reproduce the baseline.
2. Add or improve one focused measurement if needed.
3. Implement the smallest fix.
4. Run correctness gates.
5. Run ratchet current and compare.
6. Keep the change only if metrics improve.

## Validation commands

```bash
nix develop -c just perf-ratchet-baseline
nix develop -c just perf-ratchet-current
nix develop -c just perf-ratchet-compare
nix develop -c just perf-ratchet-next-prompt
```

## Acceptance criteria

- The targeted metric improves without a correctness regression.
- The comparator reports no hard regressions.
- The regenerated next prompt no longer selects the same shallow bottleneck unless a deeper issue remains.
"""


def run_self_test() -> int:
    for category in CATEGORIES:
        text = prompt(category, {"id": "self", "metrics": {"external_wall_ms.p50": 1.0}}, "self-test", [], None)
        assert f"Codex Performance Task: {category}" in text
    category, _, _ = classify([], None)
    assert category == "measurement-gap"
    print("[pass] ratchet_next_prompt self-test")
    return 0


def main() -> int:
    args = parse_args()
    if args.self_test:
        return run_self_test()
    reports = [load_json(path if path.is_absolute() else ROOT / path) for path in args.ratchet if (path if path.is_absolute() else ROOT / path).is_file()]
    compare = load_optional(args.compare)
    category, evidence, reason = classify(reports, compare)
    if category not in CATEGORIES:
        raise SystemExit(f"internal error: invalid category {category}")
    out = args.out if args.out.is_absolute() else ROOT / args.out
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(prompt(category, evidence, reason, args.ratchet, args.compare), encoding="utf-8")
    print(f"[pass] wrote next performance prompt {rel(out)} ({category})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
