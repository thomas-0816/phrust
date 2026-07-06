#!/usr/bin/env python3
"""Smoke-test the no-exec copy-and-patch stencil report."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
ENGINE = ROOT / "target/debug/php-vm"
OUT_DIR = ROOT / "target/performance/stencils"
FIXTURES = (
    ROOT / "tests/fixtures/performance/perf_smoke/arithmetic.php",
    ROOT / "tests/fixtures/performance/perf_smoke/arrays_packed.php",
    ROOT / "tests/fixtures/performance/perf_smoke/stdlib_dispatch.php",
    ROOT / "tests/fixtures/performance/perf_smoke/properties.php",
    ROOT / "tests/fixtures/performance/framework_smoke/packed_mixed_array_traversal.php",
)


def rel(path: Path) -> str:
    try:
        return path.resolve().relative_to(ROOT).as_posix()
    except ValueError:
        return path.as_posix()


def run_report(fixture: Path) -> dict[str, Any]:
    completed = subprocess.run(
        [str(ENGINE), "dump-copy-patch-stencils", rel(fixture), "--json"],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if completed.returncode != 0:
        raise SystemExit(
            f"[fail] copy-patch stencil failed for {rel(fixture)} "
            f"with status {completed.returncode}: {completed.stderr.strip()}"
        )
    try:
        report = json.loads(completed.stdout)
    except json.JSONDecodeError as exc:
        raise SystemExit(
            f"[fail] invalid copy-patch stencil JSON for {rel(fixture)}: {exc}"
        ) from exc
    if report.get("schema_version") != 1:
        raise SystemExit(f"[fail] unexpected schema version for {rel(fixture)}")
    if report.get("backend") != "copy-patch-stencil":
        raise SystemExit(f"[fail] unexpected backend for {rel(fixture)}")
    if report.get("status") != "no-exec":
        raise SystemExit(f"[fail] copy-patch report is not no-exec for {rel(fixture)}")
    if report.get("native_execution") is not False:
        raise SystemExit(f"[fail] native execution unexpectedly enabled for {rel(fixture)}")
    if report.get("executable_memory") is not False:
        raise SystemExit(f"[fail] executable memory unexpectedly enabled for {rel(fixture)}")
    if report.get("instructions", 0) <= 0:
        raise SystemExit(f"[fail] report has no instructions for {rel(fixture)}")
    if report.get("stencil_count", 0) <= 0:
        raise SystemExit(f"[fail] report has no stencils for {rel(fixture)}")
    if report.get("patch_sites", 0) <= 0:
        raise SystemExit(f"[fail] report has no patch-site estimate for {rel(fixture)}")
    if report.get("compile_cost_units", 0) <= 0:
        raise SystemExit(f"[fail] report has no compile-cost estimate for {rel(fixture)}")
    if "work_to_compile_ratio" not in report:
        raise SystemExit(f"[fail] report has no work-to-compile ratio for {rel(fixture)}")
    return report


def main() -> int:
    if not ENGINE.is_file():
        raise SystemExit(f"[fail] Rust VM is not executable: {rel(ENGINE)}")
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    reports = []
    stencil_kinds: dict[str, int] = {}
    unsupported_reasons: dict[str, int] = {}
    for fixture in FIXTURES:
        if not fixture.is_file():
            raise SystemExit(f"[fail] missing copy-patch fixture: {rel(fixture)}")
        report = run_report(fixture)
        reports.append(report)
        output = OUT_DIR / f"{rel(fixture).replace('/', '__')}.json"
        output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        for kind, count in report.get("stencil_kinds", {}).items():
            if isinstance(count, int):
                stencil_kinds[kind] = stencil_kinds.get(kind, 0) + count
        for reason, count in report.get("unsupported_by_reason", {}).items():
            if isinstance(count, int):
                unsupported_reasons[reason] = unsupported_reasons.get(reason, 0) + count

    required_kinds = {
        "guarded_int_arithmetic",
        "packed_array_guard_fetch",
        "known_builtin_call",
        "branch_guard",
        "return",
        "guarded_property_fetch",
        "guarded_property_assignment",
    }
    missing = sorted(required_kinds.difference(stencil_kinds))
    if missing:
        raise SystemExit(f"[fail] copy-patch smoke missed stencil kind(s): {', '.join(missing)}")
    if "array_mutation_requires_reference_cow_and_allocator_state" not in unsupported_reasons:
        raise SystemExit("[fail] copy-patch smoke did not exercise array mutation rejection")

    instructions = sum(int(report["instructions"]) for report in reports)
    compile_cost = sum(int(report["compile_cost_units"]) for report in reports)
    stencil_count = sum(int(report["stencil_count"]) for report in reports)
    summary = {
        "status": "pass",
        "schema_version": 1,
        "fixture_count": len(reports),
        "native_execution": False,
        "executable_memory": False,
        "instructions": instructions,
        "stencil_count": stencil_count,
        "unsupported_instructions": sum(int(report["unsupported_instructions"]) for report in reports),
        "patch_sites": sum(int(report["patch_sites"]) for report in reports),
        "helper_calls": sum(int(report["helper_calls"]) for report in reports),
        "live_state_slots": sum(int(report["live_state_slots"]) for report in reports),
        "deopt_points": sum(int(report["deopt_points"]) for report in reports),
        "compile_cost_units": compile_cost,
        "estimated_code_size_bytes": sum(
            int(report["estimated_code_size_bytes"]) for report in reports
        ),
        "work_to_compile_ratio": f"{stencil_count / max(compile_cost, 1):.3f}",
        "stencil_kinds": stencil_kinds,
        "unsupported_by_reason": unsupported_reasons,
        "fixtures": [report["path"] for report in reports],
    }
    (OUT_DIR / "summary.json").write_text(
        json.dumps(summary, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    print(
        "[pass] copy-patch stencil smoke compared "
        f"{len(reports)} fixture(s), {instructions} instruction(s), "
        f"{stencil_count} stencil(s), and wrote {rel(OUT_DIR / 'summary.json')}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
