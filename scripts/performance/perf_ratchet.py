#!/usr/bin/env python3
"""Top-level performance ratchet report orchestration."""

from __future__ import annotations

import argparse
import os
import shutil
import sys
from pathlib import Path
from typing import Any

from ratchet_schema import ROOT, load_json, make_report, rel, render_report_markdown, validate_report, write_json


DEFAULT_INPUTS = [
    ROOT / "target/performance/ratchet/cli/current.json",
    ROOT / "target/performance/ratchet/app-flow/current.json",
    ROOT / "target/performance/ratchet/server/current.json",
    ROOT / "target/performance/ratchet/counters/current.json",
]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="command", required=True)
    for name in ("combine", "report"):
        command = sub.add_parser(name)
        command.add_argument("--run-id", default=f"perf-ratchet-{name}")
        command.add_argument("--input", action="append", type=Path, default=[])
        command.add_argument("--out", type=Path, required=True)
        command.add_argument("--markdown-out", type=Path, required=True)
    accept = sub.add_parser("accept-local")
    accept.add_argument("--current", type=Path, default=ROOT / "target/performance/ratchet/current.json")
    accept.add_argument("--baseline", type=Path, default=ROOT / "target/performance/ratchet/baseline.json")
    accept.add_argument("--current-markdown", type=Path, default=ROOT / "target/performance/ratchet/current.md")
    accept.add_argument("--baseline-markdown", type=Path, default=ROOT / "target/performance/ratchet/baseline.md")
    return parser.parse_args()


def combine(run_id: str, inputs: list[Path]) -> dict[str, Any]:
    scenarios: list[dict[str, Any]] = []
    failures: list[str] = []
    missing: list[str] = []
    environment: dict[str, Any] | None = None
    for input_path in inputs or DEFAULT_INPUTS:
        path = input_path if input_path.is_absolute() else ROOT / input_path
        if not path.is_file():
            missing.append(rel(path))
            continue
        data = load_json(path)
        errors = validate_report(data)
        if errors:
            failures.extend(f"{rel(path)}: {error}" for error in errors)
            continue
        scenarios.extend(data.get("scenarios", []))
        failures.extend(str(item) for item in data.get("failures", []))
        if environment is None and isinstance(data.get("environment"), dict):
            environment = data["environment"]
    report = make_report(
        run_id=run_id,
        created_by="perf_ratchet.py",
        scenarios=scenarios,
        failures=failures,
    )
    if environment is not None:
        report["environment"].update(environment)
    report["missing_inputs"] = missing
    return report


def render_top_report(report: dict[str, Any]) -> str:
    lines = [
        render_report_markdown(report, "Performance Ratchet Report").rstrip(),
        "",
        "## Commands",
        "",
        "- `nix develop -c just perf-ratchet-smoke`",
        "- `nix develop -c just perf-ratchet-current`",
        "- `nix develop -c just perf-ratchet-compare`",
        "- `nix develop -c just perf-ratchet-next-prompt`",
    ]
    if report.get("missing_inputs"):
        lines.extend(["", "## Missing Inputs", ""])
        lines.extend(f"- `{item}`" for item in report["missing_inputs"])
    lines.extend(["", "Next prompt path: `target/performance/ratchet/next-performance-prompt.md`"])
    return "\n".join(lines) + "\n"


def write_report(report: dict[str, Any], out: Path, markdown_out: Path) -> int:
    errors = validate_report(report)
    if errors:
        report["failures"].extend(errors)
    write_json(out, report)
    markdown_out.parent.mkdir(parents=True, exist_ok=True)
    markdown_out.write_text(render_top_report(report), encoding="utf-8")
    print(f"[{'fail' if report['failures'] else 'pass'}] ratchet report wrote {rel(out)}")
    return 1 if report["failures"] else 0


def accept_local(args: argparse.Namespace) -> int:
    if os.getenv("PHRUST_RATCHET_ACCEPT") != "1":
        print("[fail] refusing to accept local baseline without PHRUST_RATCHET_ACCEPT=1", file=sys.stderr)
        return 1
    current = args.current if args.current.is_absolute() else ROOT / args.current
    current_md = args.current_markdown if args.current_markdown.is_absolute() else ROOT / args.current_markdown
    baseline = args.baseline if args.baseline.is_absolute() else ROOT / args.baseline
    baseline_md = args.baseline_markdown if args.baseline_markdown.is_absolute() else ROOT / args.baseline_markdown
    data = load_json(current)
    errors = validate_report(data)
    if errors or data.get("failures"):
        print("[fail] refusing to accept invalid or failing current report", file=sys.stderr)
        for error in [*errors, *data.get("failures", [])]:
            print(f"- {error}", file=sys.stderr)
        return 1
    baseline.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(current, baseline)
    if current_md.is_file():
        shutil.copyfile(current_md, baseline_md)
    print(f"[pass] accepted local ratchet baseline at {rel(baseline)}")
    return 0


def main() -> int:
    args = parse_args()
    if args.command in {"combine", "report"}:
        out = args.out if args.out.is_absolute() else ROOT / args.out
        markdown = args.markdown_out if args.markdown_out.is_absolute() else ROOT / args.markdown_out
        return write_report(combine(args.run_id, args.input), out, markdown)
    if args.command == "accept-local":
        return accept_local(args)
    return 1


if __name__ == "__main__":
    sys.exit(main())
