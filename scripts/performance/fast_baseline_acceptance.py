#!/usr/bin/env python3
"""Aggregate the fast-baseline large-function and WordPress recovery gates."""

from __future__ import annotations

import argparse
import json
import tempfile
from pathlib import Path
from typing import Any

MIB = 1024 * 1024
GIB = 1024 * MIB


def load_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError as error:
        raise ValueError(f"missing required report: {path}") from error
    except json.JSONDecodeError as error:
        raise ValueError(f"invalid JSON report {path}: {error}") from error
    if not isinstance(value, dict):
        raise ValueError(f"report root must be an object: {path}")
    return value


def cold_probe(report: dict[str, Any], label: str, concurrency: int) -> tuple[float, int]:
    probe = report.get("cold_probe")
    if not isinstance(probe, dict):
        raise ValueError(f"{label} has no cold_probe object")
    if probe.get("concurrency") != concurrency:
        raise ValueError(
            f"{label} cold probe concurrency is {probe.get('concurrency')}, expected {concurrency}"
        )
    if probe.get("completed_samples") != concurrency:
        raise ValueError(
            f"{label} completed {probe.get('completed_samples')} of {concurrency} cold requests"
        )
    if probe.get("failures"):
        raise ValueError(f"{label} cold probe contains request failures")
    latency = probe.get("latency_ms", {}).get("p50")
    rss = probe.get("process", {}).get("peak_rss_bytes")
    if not isinstance(latency, (int, float)) or not isinstance(rss, int):
        raise ValueError(f"{label} cold probe lacks latency or peak RSS")
    correctness = report.get("correctness", {})
    if correctness.get("failures"):
        raise ValueError(f"{label} contains PHP-visible correctness failures")
    return float(latency), rss


def gate(name: str, actual: float, limit: float, unit: str) -> tuple[bool, str]:
    passed = actual < limit
    return passed, f"- {name}: {actual:.3f} {unit} (`{'pass' if passed else 'fail'}`, limit < {limit:g} {unit})"


def aggregate(
    large_path: Path,
    empty_c1_path: Path,
    empty_c4_path: Path,
    populated_path: Path,
    restart_path: Path,
) -> tuple[bool, str]:
    large = load_json(large_path)
    empty_c1 = load_json(empty_c1_path)
    empty_c4 = load_json(empty_c4_path)
    populated = load_json(populated_path)
    restart = load_json(restart_path)

    metrics = large.get("metrics")
    if not isinstance(metrics, dict):
        raise ValueError(f"large-function report lacks metrics: {large_path}")
    compile_ms = float(metrics["compile_time_ms"])
    rss_mib = float(metrics["rss_delta_kib"]) / 1024
    code_bytes = float(metrics["code_bytes"])
    empty_ms, empty_rss = cold_probe(empty_c1, "empty-cache c1", 1)
    c4_ms, c4_rss = cold_probe(empty_c4, "empty-cache c4", 4)
    populated_ms, populated_rss = cold_probe(populated, "populated-cache c1", 1)
    compile_attempts = restart.get("compile_attempts")
    restart_pass = restart.get("status") == "pass" and compile_attempts == 0

    checks: list[tuple[bool, str]] = [
        gate("large-function compile", compile_ms, 500, "ms"),
        gate("large-function RSS delta", rss_mib, 200, "MiB"),
        gate("large-function native code", code_bytes, MIB, "bytes"),
        gate("empty-cache c1 first page", empty_ms, 2_000, "ms"),
        gate("empty-cache c1 peak RSS", float(empty_rss), 1.5 * GIB, "bytes"),
        gate("empty-cache c4 p50", c4_ms, 2_000, "ms"),
        gate("empty-cache c4 peak RSS", float(c4_rss), 1.5 * GIB, "bytes"),
        gate("populated-cache fresh first page", populated_ms, 300, "ms"),
        gate("populated-cache fresh peak RSS", float(populated_rss), 1.5 * GIB, "bytes"),
        (
            restart_pass,
            f"- restart native compile attempts: {compile_attempts} (`{'pass' if restart_pass else 'fail'}`, required 0)",
        ),
    ]
    passed = all(result for result, _ in checks)
    lines = [
        "# Fast baseline breakthrough summary",
        "",
        f"Status: `{'pass' if passed else 'fail'}`",
        "",
        "This report applies the Prompt A8 limits without converting failed latency",
        "targets into skips or warm-only claims.",
        "",
        "## Acceptance gates",
        "",
        *(line for _, line in checks),
        "",
        "## Evidence",
        "",
        f"- large function: `{large_path}`",
        f"- empty-cache c1: `{empty_c1_path}`",
        f"- empty-cache c4: `{empty_c4_path}`",
        f"- populated-cache fresh process: `{populated_path}`",
        f"- restart compile audit: `{restart_path}`",
        "",
    ]
    return passed, "\n".join(lines)


def self_test() -> None:
    with tempfile.TemporaryDirectory(prefix="phrust-fast-baseline-") as directory:
        root = Path(directory)
        large = {"metrics": {"compile_time_ms": 100, "rss_delta_kib": 1024, "code_bytes": 1024}}

        def wordpress(concurrency: int, latency: float) -> dict[str, Any]:
            return {
                "cold_probe": {
                    "concurrency": concurrency,
                    "completed_samples": concurrency,
                    "failures": [],
                    "latency_ms": {"p50": latency},
                    "process": {"peak_rss_bytes": MIB},
                },
                "correctness": {"failures": []},
            }

        values = {
            "large.json": large,
            "empty-c1.json": wordpress(1, 100),
            "empty-c4.json": wordpress(4, 100),
            "populated.json": wordpress(1, 100),
            "restart.json": {"status": "pass", "compile_attempts": 0},
        }
        for name, value in values.items():
            (root / name).write_text(json.dumps(value), encoding="utf-8")
        passed, report = aggregate(*(root / name for name in values))
        assert passed and "Status: `pass`" in report
        values["populated.json"] = wordpress(1, 301)
        (root / "populated.json").write_text(json.dumps(values["populated.json"]), encoding="utf-8")
        passed, report = aggregate(*(root / name for name in values))
        assert not passed and "Status: `fail`" in report


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--large-function")
    parser.add_argument("--empty-c1")
    parser.add_argument("--empty-c4")
    parser.add_argument("--populated")
    parser.add_argument("--cache-restart")
    parser.add_argument("--out", default="target/breakthrough/fast-baseline/summary.md")
    parser.add_argument("--allow-fail", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("fast baseline acceptance self-test: pass")
        return 0
    required = {
        "--large-function": args.large_function,
        "--empty-c1": args.empty_c1,
        "--empty-c4": args.empty_c4,
        "--populated": args.populated,
        "--cache-restart": args.cache_restart,
    }
    missing = [name for name, value in required.items() if not value]
    if missing:
        parser.error(f"required arguments are missing: {', '.join(missing)}")
    try:
        passed, report = aggregate(
            Path(args.large_function),
            Path(args.empty_c1),
            Path(args.empty_c4),
            Path(args.populated),
            Path(args.cache_restart),
        )
    except (KeyError, TypeError, ValueError) as error:
        parser.error(str(error))
    output = Path(args.out)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(report, encoding="utf-8")
    print(output)
    return 0 if passed or args.allow_fail else 1


if __name__ == "__main__":
    raise SystemExit(main())
