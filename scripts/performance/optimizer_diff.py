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
    for fixture in fixtures:
        samples = {level: run_sample(engine, fixture, level, out_dir) for level in LEVELS}
        baseline = samples["0"]
        for level in LEVELS[1:]:
            difference = compare_samples(baseline, samples[level], out_dir)
            if difference is not None:
                differences.append(difference)

    summary = {
        "gate": "optimizer-diff",
        "status": "fail" if differences else "pass",
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
            "pass_report_invariants": "covered by php_optimizer unit tests",
            "product_compile_report": "not exposed by the native-only CLI",
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
    if differences:
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
