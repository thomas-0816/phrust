#!/usr/bin/env python3
"""Anti-theater guard for performance branches.

Compares a base ref against a head ref (or the working tree) and fails when
a diff claims performance work while only containing measurement theater:
docs, report scripts, counters, or metric renames without production Rust
changes or gates.

Usage:
  perf_pr_guard.py [--base <ref>] [--head <ref>] [--allow-measurement-only]
  perf_pr_guard.py --self-test

Base resolution: --base, then PHRUST_PERF_GUARD_BASE, then merge-base with
origin/main (falling back to main).
"""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]

HOT_PATH_PREFIXES = (
    "crates/php_runtime/src/value.rs",
    "crates/php_runtime/src/array.rs",
    "crates/php_runtime/src/string",
    "crates/php_runtime/src/reference.rs",
    "crates/php_runtime/src/layout_stats.rs",
    "crates/php_vm/src/vm/",
)
GATE_PREFIXES = (
    "crates/php_bench/benches/",
    "scripts/performance/",
    "scripts/verify/",
    "justfile",
    "tests/fixtures/performance/",
)
REPORT_SCRIPT_MARKERS = ("_report.py", "root_profile.py")
NATIVE_REPORT_MARKERS = ("native_region_report.py", "jit_report")
NATIVE_CODE_PREFIXES = (
    "crates/php_jit/",
    "crates/php_vm/src/vm/jit",
    "crates/php_vm/src/jit",
)


def changed_files(base: str, head: str | None) -> list[str]:
    args = ["git", "diff", "--name-only", base]
    if head:
        args.append(head)
    output = subprocess.run(
        args, cwd=REPO_ROOT, capture_output=True, text=True, check=True
    ).stdout
    return [line.strip() for line in output.splitlines() if line.strip()]


def classify(files: list[str]) -> dict[str, list[str]]:
    classes: dict[str, list[str]] = {
        "production_rust": [],
        "docs": [],
        "report_scripts": [],
        "counters_metrics": [],
        "hot_paths": [],
        "gates": [],
        "native_reports": [],
        "native_code": [],
        "other": [],
    }
    for path in files:
        matched = False
        if path.endswith(".rs") and path.startswith("crates/") and "/tests/" not in path:
            classes["production_rust"].append(path)
            matched = True
            if path.endswith(("counters.rs", "metrics.rs")):
                classes["counters_metrics"].append(path)
            if any(path.startswith(prefix) for prefix in HOT_PATH_PREFIXES):
                classes["hot_paths"].append(path)
            if any(path.startswith(prefix) for prefix in NATIVE_CODE_PREFIXES):
                classes["native_code"].append(path)
        if path.startswith("docs/"):
            classes["docs"].append(path)
            matched = True
        if path.endswith(".py") and any(marker in path for marker in REPORT_SCRIPT_MARKERS):
            classes["report_scripts"].append(path)
            matched = True
        if any(marker in path for marker in NATIVE_REPORT_MARKERS):
            classes["native_reports"].append(path)
            matched = True
        if any(path.startswith(prefix) for prefix in GATE_PREFIXES) or path == "justfile":
            classes["gates"].append(path)
            matched = True
        if not matched:
            classes["other"].append(path)
    return classes


def evaluate(classes: dict[str, list[str]]) -> list[str]:
    failures: list[str] = []
    production = classes["production_rust"]
    gates = classes["gates"]
    if classes["docs"] and not production:
        failures.append(
            "docs changed without production Rust changes; a performance "
            "branch must change the code it claims to optimize"
        )
    if classes["report_scripts"] and not production:
        failures.append(
            "report scripts changed without production Rust changes; "
            "measurement alone is not an optimization"
        )
    if classes["counters_metrics"] and not gates:
        failures.append(
            "counters/metrics changed without a benchmark or gate change; "
            "counter work needs an executable check"
        )
    if classes["hot_paths"] and not gates:
        failures.append(
            "hot-path files changed without touching a performance gate; "
            "run and update profiler-overhead/perf gates alongside hot-path work"
        )
    if classes["native_reports"] and not classes["native_code"]:
        failures.append(
            "native/JIT report files changed while no native/JIT execution "
            "code changed; do not report a tier that does not run"
        )
    return failures


def self_test() -> int:
    cases = [
        # (description, files, expect_failures)
        ("docs-only perf diff fails", ["docs/performance-notes.md"], True),
        (
            "report-script-only diff fails",
            ["scripts/performance/clone_churn_report.py"],
            True,
        ),
        (
            "production rust + gate passes",
            [
                "crates/php_vm/src/vm/mod.rs",
                "scripts/performance/profiler_overhead_gate.py",
            ],
            False,
        ),
        (
            "hot-path change with criterion benchmark passes",
            [
                "crates/php_vm/src/vm/method_dispatch.rs",
                "crates/php_bench/benches/perf_hotpaths.rs",
            ],
            False,
        ),
        (
            "hot-path change without gate fails",
            ["crates/php_runtime/src/array.rs"],
            True,
        ),
        (
            "native report without native code fails",
            ["scripts/performance/native_region_report.py"],
            True,
        ),
        (
            "counter change with gate passes",
            ["crates/php_vm/src/counters.rs", "justfile"],
            False,
        ),
    ]
    failed = 0
    for description, files, expect_failures in cases:
        failures = evaluate(classify(files))
        ok = bool(failures) == expect_failures
        print(f"[{'ok' if ok else 'FAIL'}] {description}: {failures or 'clean'}")
        if not ok:
            failed += 1
    return 1 if failed else 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base")
    parser.add_argument("--head")
    parser.add_argument("--allow-measurement-only", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        return self_test()

    base = args.base or os.environ.get("PHRUST_PERF_GUARD_BASE", "")
    if not base:
        for candidate in ("origin/main", "main"):
            probe = subprocess.run(
                ["git", "merge-base", "HEAD", candidate],
                cwd=REPO_ROOT,
                capture_output=True,
                text=True,
            )
            if probe.returncode == 0:
                base = probe.stdout.strip()
                break
    if not base:
        print("[fail] perf PR guard: could not resolve a base ref")
        return 1

    files = changed_files(base, args.head)
    if not files:
        print("[ok] perf PR guard: no changes against base")
        return 0
    classes = classify(files)
    failures = evaluate(classes)
    if failures and args.allow_measurement_only:
        print(
            "[warn] perf PR guard: measurement-only patch explicitly allowed — "
            "this is NOT an optimization and must not claim one"
        )
        for failure in failures:
            print(f"  waived: {failure}")
        return 0
    if failures:
        print("[fail] perf PR guard:")
        for failure in failures:
            print(f"  - {failure}")
        print("  fix: include the production change the branch claims, or add/run")
        print("  the matching gate; use --allow-measurement-only only for explicitly")
        print("  non-optimization measurement work.")
        return 1
    print(f"[ok] perf PR guard: {len(files)} changed files look like real work")
    return 0


if __name__ == "__main__":
    sys.exit(main())
