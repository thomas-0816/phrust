#!/usr/bin/env python3
"""Differential harness for performance optimizer levels."""

from __future__ import annotations

import argparse
import difflib
import json
import os
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from normalize_perf_output import normalize


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_ENGINE = ROOT / "target/debug/php-vm"
DEFAULT_OUT_DIR = ROOT / "target/performance/optimizer-diff"
LEVELS = ("0", "1", "2")
DIAGNOSTIC_ID_RE = re.compile(r"E_PHP_[A-Z0-9_]+")

SELECTED_RUNTIME_6_FIXTURES = (
    "fixtures/runtime/valid/hello.php",
    "fixtures/runtime/valid/scalars/expressions.php",
    "fixtures/runtime/valid/control_flow/while-counter.php",
    "fixtures/runtime/valid/functions/factorial.php",
    "fixtures/runtime/valid/includes/include-return.php",
    "fixtures/runtime/valid/arrays/indexed.php",
    "fixtures/runtime/invalid/errors/undefined-function.php",
    "tests/fixtures/stdlib/_harness/json-pcre-date/json_basics.php",
    "tests/fixtures/stdlib/_harness/stdlib/string_transform.php",
    "tests/fixtures/stdlib/corpus/array_manipulation.php",
)


@dataclass(frozen=True)
class Sample:
    fixture: str
    level: str
    returncode: int
    stdout: str
    stderr: str
    normalized_stderr: str
    diagnostics: list[dict[str, Any]]
    counters: dict[str, Any] | None


@dataclass(frozen=True)
class Difference:
    fixture: str
    other_level: str
    sections: list[str]
    diff_path: Path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--engine", type=Path, default=Path(os.getenv("PHRUST_PHP_VM", DEFAULT_ENGINE)))
    parser.add_argument("--out-dir", type=Path, default=DEFAULT_OUT_DIR)
    parser.add_argument("--self-test", action="store_true")
    return parser.parse_args()


def rel(path: Path) -> str:
    try:
        return path.resolve().relative_to(ROOT).as_posix()
    except ValueError:
        return path.as_posix()


def safe_name(value: str) -> str:
    return re.sub(r"[^A-Za-z0-9_.-]+", "__", value)


def fixture_paths() -> list[str]:
    fixtures: set[str] = set()
    optimizer_dir = ROOT / "tests/fixtures/performance/optimizer"
    if optimizer_dir.is_dir():
        fixtures.update(rel(path) for path in optimizer_dir.rglob("*.php") if path.is_file())

    perf_dir = ROOT / "tests/fixtures/performance"
    fixtures.update(
        rel(path)
        for path in perf_dir.rglob("*.php")
        if path.is_file() and path.with_name(path.name + ".out").is_file()
    )

    for fixture in SELECTED_RUNTIME_6_FIXTURES:
        path = ROOT / fixture
        if not path.is_file():
            raise SystemExit(f"selected optimizer fixture is missing: {fixture}")
        fixtures.add(fixture)

    if not fixtures:
        raise SystemExit("optimizer-diff found no fixtures")
    return sorted(fixtures)


def extract_diagnostics(stderr: str) -> list[dict[str, Any]]:
    diagnostics: list[dict[str, Any]] = []
    for line in normalize(stderr).splitlines():
        if "runtime-diagnostic:" in line:
            payload = line.split("runtime-diagnostic:", 1)[1].strip()
            try:
                diagnostics.append({"kind": "runtime", "payload": json.loads(payload)})
            except json.JSONDecodeError:
                diagnostics.append({"kind": "runtime-unparsed", "line": payload})
            continue
        ids = DIAGNOSTIC_ID_RE.findall(line)
        for diagnostic_id in ids:
            diagnostics.append({"kind": "diagnostic-id", "id": diagnostic_id})
    return diagnostics


def run_sample(engine: Path, fixture: str, level: str, out_dir: Path) -> Sample:
    counters_path = out_dir / "counters" / f"{safe_name(fixture)}.opt{level}.json"
    counters_path.parent.mkdir(parents=True, exist_ok=True)
    counters_path.unlink(missing_ok=True)
    completed = subprocess.run(
        [
            str(engine),
            "run",
            f"--opt-level={level}",
            "--counters-json",
            rel(counters_path),
            fixture,
        ],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    counters = None
    if counters_path.is_file():
        counters = json.loads(counters_path.read_text(encoding="utf-8"))
    return Sample(
        fixture=fixture,
        level=level,
        returncode=completed.returncode,
        stdout=completed.stdout.replace("\r\n", "\n").replace("\r", "\n"),
        stderr=completed.stderr.replace("\r\n", "\n").replace("\r", "\n"),
        normalized_stderr=normalize(completed.stderr),
        diagnostics=extract_diagnostics(completed.stderr),
        counters=counters,
    )


def compile_optimizer_report(
    engine: Path,
    fixture: str,
    level: str,
    out_dir: Path,
) -> dict[str, Any] | None:
    report_path = out_dir / "optimizer-reports" / f"{safe_name(fixture)}.opt{level}.json"
    report_path.parent.mkdir(parents=True, exist_ok=True)
    completed = subprocess.run(
        [
            str(engine),
            "compile",
            "--json",
            f"--opt-level={level}",
            fixture,
        ],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    payload: dict[str, Any]
    if completed.returncode == 0:
        try:
            payload = json.loads(completed.stdout)
        except json.JSONDecodeError as error:
            payload = {
                "ok": False,
                "error": f"compile JSON parse failed: {error}",
                "stderr": normalize(completed.stderr),
            }
    else:
        payload = {
            "ok": False,
            "error": f"compile exited {completed.returncode}",
            "stderr": normalize(completed.stderr),
        }
    report_path.write_text(pretty_json(payload), encoding="utf-8")
    optimizer = payload.get("optimizer")
    return optimizer if isinstance(optimizer, dict) else None


def summarize_optimizer_reports(
    reports: dict[str, dict[str, dict[str, Any] | None]],
) -> dict[str, Any]:
    by_pass: dict[str, dict[str, int]] = {}
    changed: dict[str, int] = {}
    reports_seen = 0
    for levels in reports.values():
        for report in levels.values():
            if not report:
                continue
            reports_seen += 1
            for pass_report in report.get("passes", []):
                name = pass_report.get("name")
                if not isinstance(name, str):
                    continue
                if pass_report.get("changed") is True:
                    changed[name] = changed.get(name, 0) + 1
                stats = pass_report.get("stats", {})
                if not isinstance(stats, dict):
                    continue
                totals = by_pass.setdefault(name, {})
                for key, value in stats.items():
                    if isinstance(key, str) and isinstance(value, int):
                        totals[key] = totals.get(key, 0) + value
    return {
        "reports_seen": reports_seen,
        "changed_pass_fixture_counts": changed,
        "stats_by_pass": by_pass,
    }


def optimizer_report_invariant_errors(
    reports: dict[str, dict[str, dict[str, Any] | None]],
) -> list[str]:
    errors: list[str] = []
    for fixture, levels in reports.items():
        for level, report in levels.items():
            if not report:
                errors.append(f"{fixture} opt{level}: optimizer report missing")
                continue
            for pass_report in report.get("passes", []):
                name = pass_report.get("name", "<unnamed>")
                if not pass_report.get("enabled", False):
                    continue
                stats = pass_report.get("stats", {})
                verifier_calls = stats.get("verifier_calls")
                expected_verifier_calls = 1 if pass_report.get("changed") else 0
                if verifier_calls != expected_verifier_calls:
                    errors.append(
                        f"{fixture} opt{level} {name}: verifier_calls={verifier_calls!r}, "
                        f"expected {expected_verifier_calls}"
                    )
                if not str(name).endswith("_noop"):
                    continue
                if stats.get("scope_snapshots") != 0 or stats.get("snapshot_bytes") != 0:
                    errors.append(
                        f"{fixture} opt{level} {name}: no-op pass created a snapshot"
                    )
                scope = pass_report.get("scope")
                if scope != {
                    "blocks": [],
                    "constants": False,
                    "functions": [],
                    "metadata": [],
                    "source_mappings_may_change": False,
                }:
                    errors.append(f"{fixture} opt{level} {name}: no-op pass touched scope")
    return errors


def pretty_json(value: Any) -> str:
    return json.dumps(value, indent=2, sort_keys=True) + "\n"


def unified(name: str, left: str, right: str, left_label: str, right_label: str) -> str:
    if left == right:
        return ""
    lines = difflib.unified_diff(
        left.splitlines(keepends=True),
        right.splitlines(keepends=True),
        fromfile=f"{left_label}/{name}",
        tofile=f"{right_label}/{name}",
    )
    return "".join(lines)


def counter_invariant_errors(baseline: Sample, other: Sample) -> list[str]:
    errors: list[str] = []
    if baseline.counters is None:
        errors.append("opt0 counters missing")
    if other.counters is None:
        errors.append(f"opt{other.level} counters missing")
    if errors:
        return errors

    for key in ("runtime_diagnostics", "guard_failures"):
        baseline_value = baseline.counters.get(key)
        other_value = other.counters.get(key)
        if baseline_value != other_value:
            errors.append(
                f"{key} differs: opt0={baseline_value!r} opt{other.level}={other_value!r}"
            )
    return errors


def compare_samples(baseline: Sample, other: Sample, out_dir: Path) -> Difference | None:
    sections: list[str] = []
    diff_parts: list[str] = []
    if baseline.returncode != other.returncode:
        sections.append("exit_code")
        diff_parts.append(
            f"exit_code opt0={baseline.returncode} opt{other.level}={other.returncode}\n"
        )
    stdout_diff = unified("stdout", baseline.stdout, other.stdout, "opt0", f"opt{other.level}")
    if stdout_diff:
        sections.append("stdout")
        diff_parts.append(stdout_diff)
    stderr_diff = unified(
        "stderr.normalized",
        baseline.normalized_stderr,
        other.normalized_stderr,
        "opt0",
        f"opt{other.level}",
    )
    if stderr_diff:
        sections.append("stderr")
        diff_parts.append(stderr_diff)
    diagnostics_diff = unified(
        "diagnostics.json",
        pretty_json(baseline.diagnostics),
        pretty_json(other.diagnostics),
        "opt0",
        f"opt{other.level}",
    )
    if diagnostics_diff:
        sections.append("diagnostics")
        diff_parts.append(diagnostics_diff)
    counter_errors = counter_invariant_errors(baseline, other)
    if counter_errors:
        sections.append("counter_invariants")
        diff_parts.append("counter_invariants\n" + "\n".join(counter_errors) + "\n")
    if not sections:
        return None

    diff_path = out_dir / f"{safe_name(baseline.fixture)}.opt0_vs_opt{other.level}.diff"
    diff_path.write_text("\n".join(diff_parts), encoding="utf-8")
    return Difference(
        fixture=baseline.fixture,
        other_level=other.level,
        sections=sections,
        diff_path=diff_path,
    )


def run_self_test(out_dir: Path) -> None:
    out_dir.mkdir(parents=True, exist_ok=True)
    baseline = Sample(
        fixture="self-test.php",
        level="0",
        returncode=0,
        stdout="same\n",
        stderr="",
        normalized_stderr="",
        diagnostics=[],
        counters={"instructions_executed": 1},
    )
    same = Sample(
        fixture="self-test.php",
        level="1",
        returncode=0,
        stdout="same\n",
        stderr="",
        normalized_stderr="",
        diagnostics=[],
        counters={"instructions_executed": 1},
    )
    different = Sample(
        fixture="self-test.php",
        level="2",
        returncode=0,
        stdout="different\n",
        stderr="",
        normalized_stderr="",
        diagnostics=[],
        counters={"instructions_executed": 1},
    )
    if compare_samples(baseline, same, out_dir) is not None:
        raise SystemExit("optimizer-diff self-test failed to accept identical samples")
    difference = compare_samples(baseline, different, out_dir)
    if difference is None or "stdout" not in difference.sections:
        raise SystemExit("optimizer-diff self-test failed to detect simulated stdout difference")
    print("[pass] optimizer-diff self-test detected simulated difference")


def run_real(engine: Path, out_dir: Path) -> int:
    if not engine.is_file():
        raise SystemExit(f"optimizer engine not found: {engine}")
    out_dir.mkdir(parents=True, exist_ok=True)
    for stale_diff in out_dir.glob("*.diff"):
        stale_diff.unlink()

    fixtures = fixture_paths()
    differences: list[Difference] = []
    optimizer_reports: dict[str, dict[str, dict[str, Any] | None]] = {}
    for fixture in fixtures:
        samples = {level: run_sample(engine, fixture, level, out_dir) for level in LEVELS}
        optimizer_reports[fixture] = {
            level: compile_optimizer_report(engine, fixture, level, out_dir)
            for level in LEVELS
            if level != "0"
        }
        baseline = samples["0"]
        for level in LEVELS[1:]:
            difference = compare_samples(baseline, samples[level], out_dir)
            if difference is not None:
                differences.append(difference)

    report_errors = optimizer_report_invariant_errors(optimizer_reports)

    summary = {
        "gate": "optimizer-diff",
        "status": "fail" if differences or report_errors else "pass",
        "fixtures": fixtures,
        "levels": list(LEVELS),
        "comparisons": len(fixtures) * (len(LEVELS) - 1),
        "differences": [
            {
                "fixture": difference.fixture,
                "level": difference.other_level,
                "sections": difference.sections,
                "diff": rel(difference.diff_path),
            }
            for difference in differences
        ],
        "optimizer_evidence": {
            "reports_dir": rel(out_dir / "optimizer-reports"),
            "summary": summarize_optimizer_reports(optimizer_reports),
            "invariant_errors": report_errors,
        },
        "compared": [
            "stdout",
            "stderr.normalized",
            "exit_code",
            "structured_diagnostics",
            "counter_invariants",
        ],
    }
    (out_dir / "summary.json").write_text(pretty_json(summary), encoding="utf-8")
    if differences:
        for difference in differences:
            print(
                f"[fail] {difference.fixture} opt0 vs opt{difference.other_level}: "
                f"{','.join(difference.sections)}; see {rel(difference.diff_path)}",
                file=sys.stderr,
            )
    for error in report_errors:
        print(f"[fail] optimizer report invariant: {error}", file=sys.stderr)
    if differences or report_errors:
        return 1
    print(
        f"[pass] optimizer-diff compared {len(fixtures)} fixture(s) across "
        f"opt-levels {','.join(LEVELS)}"
    )
    return 0


def main() -> int:
    args = parse_args()
    out_dir = args.out_dir if args.out_dir.is_absolute() else ROOT / args.out_dir
    if args.self_test:
        run_self_test(out_dir)
        return 0
    engine = args.engine if args.engine.is_absolute() else ROOT / args.engine
    return run_real(engine, out_dir)


if __name__ == "__main__":
    raise SystemExit(main())
