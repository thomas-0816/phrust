#!/usr/bin/env python3
"""Compare baseline and fast-tier VM semantics for runtime fixtures."""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path

import runtime_semantics_diff as fixture_diff

TIER_PROFILES = [
    {
        "name": "default",
        "args": ["--engine-preset=default"],
        "description": "managed fast preset",
    },
    {
        "name": "bytecode-auto",
        "args": [
            "--exec-format=auto",
            "--superinstructions=off",
            "--quickening=off",
            "--inline-caches=off",
            "--jit=off",
        ],
        "description": "dense bytecode with exact rich fallback",
    },
    {
        "name": "superinstructions-auto",
        "args": [
            "--exec-format=auto",
            "--superinstructions=on",
            "--quickening=off",
            "--inline-caches=off",
            "--jit=off",
        ],
        "description": "dense bytecode plus superinstructions with exact fallback",
    },
    {
        "name": "quickening-ir",
        "args": [
            "--exec-format=ir",
            "--superinstructions=off",
            "--quickening=on",
            "--inline-caches=off",
            "--jit=off",
        ],
        "description": "rich IR with quickening metadata and guarded fast paths",
    },
    {
        "name": "inline-caches-ir",
        "args": [
            "--exec-format=ir",
            "--superinstructions=off",
            "--quickening=off",
            "--inline-caches=on",
            "--jit=off",
        ],
        "description": "rich IR with inline caches",
    },
    {
        "name": "jit-noop-ir",
        "args": [
            "--exec-format=ir",
            "--superinstructions=off",
            "--quickening=off",
            "--inline-caches=off",
            "--jit=noop",
        ],
        "description": "JIT dispatch plumbing without native code",
    },
]


def main() -> int:
    try:
        report = run(parse_args())
    except fixture_diff.HarnessError as error:
        print(f"[error] {error}", file=sys.stderr)
        return 2

    if report["summary"]["fail"]:
        print(
            f"[fail] vm-semantics oracle failures={report['summary']['fail']} "
            f"report={report['report_path']}",
            file=sys.stderr,
        )
        return 1

    print(
        "[ok] vm-semantics oracle: "
        f"total={report['summary']['total']} "
        f"pass={report['summary']['pass']} "
        f"fail={report['summary']['fail']} "
        f"skip={report['summary']['skip']} "
        f"known_gap={report['summary']['known_gap']} "
        f"path={report['report_path']}"
    )
    return 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Compare php-vm baseline profile output with every enabled fast-tier "
            "profile for runtime-semantics fixtures."
        )
    )
    parser.add_argument("--fixtures", default="fixtures/runtime_semantics")
    parser.add_argument("--out", default="target/runtime-semantics/vm-oracle")
    parser.add_argument("--rust-vm", default=os.environ.get("PHP_VM_CLI", "target/debug/php-vm"))
    parser.add_argument("--file", action="append", default=[])
    parser.add_argument("--dir", action="append", default=[])
    parser.add_argument("--category", action="append", choices=fixture_diff.CATEGORIES, default=[])
    parser.add_argument("--stop-on-fail", action="store_true")
    parser.add_argument("paths", nargs="*")
    return parser.parse_args()


def run(args: argparse.Namespace) -> dict:
    fixtures_root = Path(args.fixtures)
    out_dir = Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)
    fixtures = fixture_diff.discover_fixtures(
        fixtures_root, args.file, args.dir, args.category, args.paths
    )

    results = []
    stopped_early = False
    for fixture in fixtures:
        item = compare_fixture(fixture, Path(args.rust_vm))
        results.append(item)
        if args.stop_on_fail and item["status"] == "fail":
            stopped_early = True
            break

    report = {
        "fixtures_root": str(fixtures_root),
        "selected": len(fixtures),
        "stopped_early": stopped_early,
        "summary": summarize(results),
        "results": results,
    }
    report_path = out_dir / "vm-semantics-oracle-report.json"
    report["report_path"] = str(report_path)
    report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return report


def compare_fixture(fixture: fixture_diff.Fixture, rust_vm: Path) -> dict:
    if fixture.expect == "skip":
        return result(fixture, "skip", None, None, "fixture metadata requested skip")
    if fixture.expect == "known_gap":
        if not fixture.known_gap_id:
            return result(
                fixture,
                "fail",
                None,
                None,
                "known-gap fixture must declare runtime-semantics known_gap=<ID>",
            )
        return result(fixture, "known_gap", None, None, None)

    baseline = run_vm_profile(rust_vm, fixture, "baseline", ["--engine-preset=baseline"], None)
    if baseline["status"] == "error":
        return result(fixture, "fail", baseline, {}, baseline["message"])

    tiers = {}
    failures = []
    for profile in TIER_PROFILES:
        candidate = run_vm_profile(
            rust_vm,
            fixture,
            profile["name"],
            profile["args"],
            profile["description"],
        )
        tiers[profile["name"]] = candidate
        if candidate["status"] == "error":
            failures.append(f"{profile['name']}: {candidate['message']}")
            continue
        differences = fixture_diff.normalized_differences(baseline, candidate)
        if differences:
            failures.append(f"{profile['name']}: {'; '.join(differences)}")

    status = "pass" if not failures else "fail"
    return result(fixture, status, baseline, tiers, "; ".join(failures) or None)


def run_vm_profile(
    rust_vm: Path,
    fixture: fixture_diff.Fixture,
    profile: str,
    profile_args: list[str],
    description: str | None,
) -> dict:
    command = [str(rust_vm), "run", *profile_args, str(fixture.path)]
    if fixture.args:
        command.extend(["--", *fixture.args])
    process = run_process(command, fixture.path)
    process["profile"] = profile
    process["profile_args"] = profile_args
    process["description"] = description
    return process


def run_process(command: list[str], fixture_path: Path) -> dict:
    env = {
        "LC_ALL": "C",
        "LANG": "C",
        "NO_COLOR": "1",
        "PHP_INI_SCAN_DIR": "",
        "PATH": os.environ.get("PATH", ""),
    }
    try:
        completed = subprocess.run(command, check=False, capture_output=True, env=env, text=True)
    except OSError as error:
        return {"status": "error", "message": f"failed to execute {command[0]}: {error}"}
    return {
        "status": "completed",
        "exit_code": completed.returncode,
        "stdout": completed.stdout,
        "stderr": completed.stderr,
        "stderr_normalized": fixture_diff.normalize_stderr(completed.stderr, fixture_path, None),
    }


def result(
    fixture: fixture_diff.Fixture,
    status: str,
    baseline: dict | None,
    tiers: dict,
    message: str | None,
) -> dict:
    default = tiers.get("default") if isinstance(tiers, dict) else None
    return {
        "file": str(fixture.path),
        "category": fixture.category,
        "expect": fixture.expect,
        "known_gap_id": fixture.known_gap_id,
        "status": status,
        "message": message,
        "baseline": baseline,
        "default": default,
        "tiers": tiers,
        "metadata": fixture.metadata,
    }


def summarize(results: list[dict]) -> dict:
    summary = {"total": len(results), "pass": 0, "fail": 0, "skip": 0, "known_gap": 0}
    for item in results:
        summary[item["status"]] = summary.get(item["status"], 0) + 1
    return summary


if __name__ == "__main__":
    raise SystemExit(main())
