#!/usr/bin/env python3
"""Audit local prerequisites for the performance ratchet workflow."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path
from typing import Any

from ratchet_schema import ROOT, executable, rel


DEFAULT_OUT_DIR = ROOT / "target/performance/ratchet/prereq"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--engine", type=Path, default=ROOT / "target/debug/php-vm")
    parser.add_argument("--out-dir", type=Path, default=DEFAULT_OUT_DIR)
    parser.add_argument("--self-test", action="store_true")
    return parser.parse_args()


def contains(path: Path, text: str) -> bool:
    return path.is_file() and text in path.read_text(encoding="utf-8")


def run_command(command: list[str], timeout: float = 10.0) -> dict[str, Any]:
    try:
        completed = subprocess.run(
            command,
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        return {"command": command, "status": "fail", "reason": str(error)}
    return {
        "command": command,
        "status": "pass" if completed.returncode == 0 else "fail",
        "exit_code": completed.returncode,
        "stdout": completed.stdout[-1000:],
        "stderr": completed.stderr[-1000:],
    }


def write_markdown(summary: dict[str, Any], path: Path) -> None:
    lines = [
        "# Performance Ratchet Prerequisite Audit",
        "",
        "| Field | Value |",
        "| --- | --- |",
        f"| Status | `{summary['status']}` |",
        "",
        "## Features",
        "",
        "| Feature | Status |",
        "| --- | --- |",
    ]
    for name, status in summary["features"].items():
        lines.append(f"| `{name}` | `{status}` |")
    if summary["failures"]:
        lines.extend(["", "## Failures", ""])
        lines.extend(f"- {failure}" for failure in summary["failures"])
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def audit(args: argparse.Namespace) -> dict[str, Any]:
    features = {
        "timings_json": "present"
        if contains(ROOT / "crates/php_vm_cli/src/commands/args.rs", "--timings-json")
        else "missing",
        "counters_json": "present"
        if contains(ROOT / "crates/php_vm_cli/src/commands/args.rs", "--counters-json")
        else "missing",
        "benchmark_timing_ingest": "present"
        if contains(ROOT / "scripts/performance/bench_matrix.py", "--timings-json")
        and contains(ROOT / "scripts/performance/bench_matrix.py", "derived_timing_metrics")
        else "missing",
        "app_flow_timing_ingest": "present"
        if contains(ROOT / "scripts/performance/app_flow_matrix.py", "--timings-json")
        and contains(ROOT / "scripts/performance/app_flow_matrix.py", "phase_summary")
        else "missing",
        "decision_baseline": "present"
        if (ROOT / "scripts/performance/decision_baseline.py").is_file()
        else "missing",
    }
    checked_commands: list[dict[str, Any]] = []
    engine = args.engine if args.engine.is_absolute() else ROOT / args.engine
    if executable(engine):
        timing_path = args.out_dir / "timings-check.json"
        counter_path = args.out_dir / "counters-check.json"
        args.out_dir.mkdir(parents=True, exist_ok=True)
        checked_commands.append(
            run_command(
                [
                    str(engine),
                    "run",
                    "--timings-json",
                    str(timing_path),
                    "--counters-json",
                    str(counter_path),
                    "fixtures/runtime/valid/hello.php",
                ]
            )
        )
        if not timing_path.is_file():
            features["timings_json"] = "missing"
        if not counter_path.is_file():
            features["counters_json"] = "missing"
    else:
        checked_commands.append(
            {
                "command": [str(engine), "run", "--timings-json", "..."],
                "status": "skip",
                "reason": f"engine unavailable: {rel(engine)}",
            }
        )
    failures = [
        f"{name} is missing"
        for name, status in features.items()
        if status == "missing"
    ]
    failures.extend(
        f"command failed: {' '.join(item.get('command', []))}"
        for item in checked_commands
        if item.get("status") == "fail"
    )
    return {
        "schema_version": 1,
        "status": "fail" if failures else "pass",
        "checked_commands": checked_commands,
        "features": features,
        "failures": failures,
    }


def run_self_test() -> int:
    assert contains(ROOT / "scripts/performance/ratchet_prereq.py", "timings_json")
    print("[pass] ratchet_prereq self-test")
    return 0


def main() -> int:
    args = parse_args()
    if args.self_test:
        return run_self_test()
    out_dir = args.out_dir if args.out_dir.is_absolute() else ROOT / args.out_dir
    args.out_dir = out_dir
    summary = audit(args)
    json_path = out_dir / "prereq-summary.json"
    markdown_path = out_dir / "prereq-summary.md"
    out_dir.mkdir(parents=True, exist_ok=True)
    json_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    write_markdown(summary, markdown_path)
    print(f"[{summary['status']}] ratchet prereq wrote {rel(json_path)}")
    return 0 if summary["status"] == "pass" else 1


if __name__ == "__main__":
    sys.exit(main())
