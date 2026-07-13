#!/usr/bin/env python3
"""Gate default execution on fast-path coverage, not parity alone."""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Callable

from normalize_perf_output import normalize


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_ENGINE = ROOT / "target/debug/php-vm"
DEFAULT_OUT_DIR = ROOT / "target/performance/managed-fast"
DEFAULT_JIT_MODE = "off"


@dataclass(frozen=True)
class Case:
    label: str
    source: str | None = None
    path: str | None = None
    floors: dict[str, int] = field(default_factory=dict)
    nested_floors: dict[str, dict[str, int]] = field(default_factory=dict)
    predicates: tuple[str, ...] = ()
    fallback: bool = False


@dataclass(frozen=True)
class RunResult:
    returncode: int
    stdout: str
    stderr: str
    counters: dict[str, Any]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--engine", type=Path, default=DEFAULT_ENGINE)
    parser.add_argument("--out-dir", type=Path, default=DEFAULT_OUT_DIR)
    parser.add_argument("--timeout", type=float, default=10.0)
    parser.add_argument("--self-test", action="store_true")
    return parser.parse_args()


def rel(path: Path) -> str:
    try:
        return path.resolve().relative_to(ROOT).as_posix()
    except ValueError:
        return path.as_posix()


def safe_name(value: str) -> str:
    return re.sub(r"[^A-Za-z0-9_.-]+", "__", value)


def env_for(label: str, out_dir: Path) -> dict[str, str]:
    tmp = out_dir / "tmp" / safe_name(label)
    tmp.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ)
    env.update(
        {
            "TZ": "UTC",
            "LC_ALL": "C",
            "LANG": "C",
            "TMPDIR": str(tmp),
            "TMP": str(tmp),
            "TEMP": str(tmp),
            "PHRUST_RANDOM_SEED": "managed-fast-coverage",
            "RUST_TEST_SEED": "managed-fast-coverage",
        }
    )
    return env


def materialize(case: Case, out_dir: Path) -> Path:
    if case.path is not None:
        path = ROOT / case.path
        if not path.is_file():
            raise SystemExit(f"missing managed-fast fixture: {case.path}")
        return path
    fixture_dir = out_dir / "fixtures"
    fixture_dir.mkdir(parents=True, exist_ok=True)
    path = fixture_dir / f"{safe_name(case.label)}.php"
    path.write_text(case.source or "", encoding="utf-8")
    return path


def run_case(
    engine: Path,
    case: Case,
    profile: str | None,
    out_dir: Path,
    timeout: float,
) -> RunResult:
    path = materialize(case, out_dir)
    run_dir = out_dir / "runs" / safe_name(case.label) / (profile or "default")
    run_dir.mkdir(parents=True, exist_ok=True)
    counters_path = run_dir / "counters.json"
    command = [str(engine), "run"]
    if profile is not None:
        command.append(f"--engine-preset={profile}")
    command.extend(["--counters-json", str(counters_path), rel(path)])
    completed = subprocess.run(
        command,
        cwd=ROOT,
        env=env_for(case.label, out_dir),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
        check=False,
    )
    stdout = completed.stdout.replace("\r\n", "\n").replace("\r", "\n")
    stderr = normalize(completed.stderr)
    (run_dir / "stdout").write_text(stdout, encoding="utf-8")
    (run_dir / "stderr").write_text(stderr, encoding="utf-8")
    (run_dir / "status").write_text(f"{completed.returncode}\n", encoding="utf-8")
    counters: dict[str, Any] = {}
    if counters_path.is_file():
        counters = json.loads(counters_path.read_text(encoding="utf-8"))
    if not isinstance(counters, dict):
        raise SystemExit(f"{rel(counters_path)}: counters root is not an object")
    return RunResult(completed.returncode, stdout, stderr, counters)


def int_counter(counters: dict[str, Any], key: str) -> int:
    value = counters.get(key, 0)
    return value if isinstance(value, int) else 0


def nested_counter(counters: dict[str, Any], key: str, nested: str) -> int:
    value = counters.get(key, {})
    if not isinstance(value, dict):
        return 0
    nested_value = value.get(nested, 0)
    return nested_value if isinstance(nested_value, int) else 0


def nested_total(counters: dict[str, Any], key: str) -> int:
    value = counters.get(key, {})
    if not isinstance(value, dict):
        return 0
    return sum(item for item in value.values() if isinstance(item, int))


def predicate_checks(counters: dict[str, Any]) -> dict[str, Callable[[], bool]]:
    return {
        "superinstructions_executed": lambda: nested_total(counters, "superinstructions_executed")
        > 0,
        "array_fast_paths_specific": lambda: nested_counter(
            counters, "array_fast_path_hits_by_family", "packed_int_fetch"
        )
        > 0
        and nested_counter(counters, "array_fast_path_hits_by_family", "packed_foreach_by_value")
        > 0,
        "builtin_intrinsics_specific": lambda: nested_total(counters, "intrinsic_hits") > 0
        and nested_total(counters, "builtin_fast_stub_hits") > 0,
        "native_policy": lambda: int_counter(counters, "native_compiled_regions") == 0
        and int_counter(counters, "native_executions") == 0
        and isinstance(counters.get("native_platform_unavailable", 0), int),
        "magic_get_reason": lambda: any(
            "magic_get_present" in profile.get("non_eligible_reasons", [])
            for profile in counters.get("property_fetch_profiles", [])
            if isinstance(profile, dict)
        ),
    }


def assert_case(case: Case, baseline: RunResult, default: RunResult) -> list[str]:
    failures: list[str] = []
    if default.returncode != baseline.returncode:
        failures.append(
            f"exit status baseline={baseline.returncode} default={default.returncode}"
        )
    if default.stdout != baseline.stdout:
        failures.append("stdout differs from baseline")
    if default.stderr != baseline.stderr:
        failures.append("stderr/runtime diagnostics differ from baseline")
    counters = default.counters
    if counters.get("jit_mode") != DEFAULT_JIT_MODE:
        failures.append(
            f"jit_mode={counters.get('jit_mode')!r}; expected {DEFAULT_JIT_MODE}"
        )
    for key, minimum in case.floors.items():
        actual = int_counter(counters, key)
        if actual < minimum:
            failures.append(f"{key}={actual}; expected >= {minimum}")
    for key, nested in case.nested_floors.items():
        for nested_key, minimum in nested.items():
            actual = nested_counter(counters, key, nested_key)
            if actual < minimum:
                failures.append(f"{key}.{nested_key}={actual}; expected >= {minimum}")
    checks = predicate_checks(counters)
    for predicate in case.predicates:
        if predicate not in checks:
            failures.append(f"unknown predicate {predicate}")
        elif not checks[predicate]():
            failures.append(f"predicate {predicate} failed")
    return [f"{case.label}: " + "; ".join(failures)] if failures else []


def run_cache_reuse(engine: Path, out_dir: Path, timeout: float) -> list[str]:
    fixture = out_dir / "fixtures" / "cache-reuse.php"
    fixture.parent.mkdir(parents=True, exist_ok=True)
    fixture.write_text("<?php echo 'cache';\n", encoding="utf-8")
    cache_dir = out_dir / "bytecode-cache"
    if cache_dir.exists():
        shutil.rmtree(cache_dir)
    failures: list[str] = []
    stats: list[dict[str, Any]] = []
    for index in range(2):
        completed = subprocess.run(
            [
                str(engine),
                "run",
                "--bytecode-cache=read-write",
                "--bytecode-cache-dir",
                str(cache_dir),
                "--bytecode-cache-stats",
                rel(fixture),
            ],
            cwd=ROOT,
            env=env_for(f"cache-reuse-{index}", out_dir),
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout,
            check=False,
        )
        if completed.returncode != 0:
            failures.append(f"cache reuse run {index} exited {completed.returncode}")
            continue
        if completed.stdout != "cache":
            failures.append(f"cache reuse run {index} stdout mismatch")
        stats.append(json.loads(completed.stderr)["bytecode_cache"])
    if len(stats) == 2:
        if not stats[0].get("miss") or not stats[0].get("wrote"):
            failures.append("first bytecode cache run did not miss and write")
        if not stats[1].get("hit"):
            failures.append("second bytecode cache run did not hit")
    return failures


CASES = (
    Case(
        "dense-superinstructions-output",
        source='<?php $name = "world"; echo "hello "; echo $name; echo "a" . "b";',
        floors={
            "bytecode_lower_successes": 1,
            "dense_functions_executed": 1,
            "superinstructions_emitted": 1,
            "output_fast_appends": 3,
        },
        predicates=("superinstructions_executed", "native_policy"),
    ),
    Case(
        "quickening-inline-cache",
        source="<?php function f($x) { return $x + 1; } for ($i = 0; $i < 8; $i = $i + 1) { echo f($i); }",
        floors={
            "quickening_attempts": 1,
            "inline_cache_hits": 1,
            "function_call_ic_hits": 1,
        },
    ),
    Case(
        "array-shape-fast-paths",
        path="tests/fixtures/performance/perf_smoke/array_fast_paths_v2.php",
        floors={
            "packed_dim_fast_path_hits": 1,
            "array_packed_read_fast_path_hits": 1,
            "array_sequential_foreach_fast_path_hits": 1,
        },
        predicates=("array_fast_paths_specific",),
    ),
    Case(
        "record-shape-fast-path",
        source='<?php $row = ["id" => 1, "name" => "a"]; echo isset($row["id"]) ? $row["name"] : "missing";',
        floors={"record_shape_hits": 1},
        nested_floors={"array_fast_path_hits_by_family": {"record_shape_fetch": 1}},
    ),
    Case(
        "builtin-intrinsics",
        path="tests/fixtures/performance/inline_cache/builtin-fast-stubs.php",
        floors={
            "internal_function_dispatch_cache_hits": 1,
            "builtin_call_ic_hits": 1,
            "builtin_intrinsic_candidates": 1,
        },
        predicates=("builtin_intrinsics_specific",),
    ),
    Case(
        "string-output-fast-paths",
        source='<?php $s = ""; for ($i = 0; $i < 10; $i = $i + 1) { $s = $s . "x"; } echo $s; echo "-"; echo strlen($s);',
        floors={
            "string_concat_fast_path_hits": 1,
            "concat_prealloc_hits": 1,
            "output_fast_appends": 2,
        },
        nested_floors={"intrinsic_hits": {"strlen": 1}},
    ),
    Case(
        "include-resolution-cache",
        source='<?php for ($i = 0; $i < 4; $i = $i + 1) { echo include (__DIR__ . "/include-value.php"); }',
        floors={
            "includes": 4,
            "include_resolution_hits": 1,
            "include_path_ic_hits": 1,
            "include_compile_misses": 1,
        },
    ),
    Case(
        "reference-cow-fallback",
        path="tests/fixtures/performance/regressions/byref-array-aliasing.php",
        floors={
            "cow_or_reference_fallbacks": 1,
            "fast_path_disabled_by_reference": 1,
        },
        nested_floors={"array_fast_path_fallback_by_reason": {"cow_or_reference": 1}},
        fallback=True,
    ),
    Case(
        "by-reference-call-fallback",
        source="<?php function bump(&$x) { $x = $x + 1; } $v = 1; bump($v); echo $v;",
        floors={"inline_cache_fallback_calls": 1, "function_call_ic_misses": 1},
        fallback=True,
    ),
    Case(
        "magic-property-fallback",
        source="<?php class M { public function __get($name) { return 7; } } $m = new M(); echo $m->value;",
        floors={"property_ic_misses": 1},
        predicates=("magic_get_reason",),
        fallback=True,
    ),
    Case(
        "numeric-string-fallback",
        source='<?php echo "2e2" + "70";',
        floors={"numeric_string_classify_calls": 1, "numeric_string_cache_misses": 1},
        fallback=True,
    ),
    Case(
        "dynamic-include-fallback",
        source='<?php $name = __DIR__ . "/missing.php"; @include $name; echo "done";',
        floors={"includes": 1, "include_resolution_misses": 1},
        nested_floors={"include_fallback_by_reason": {"missing_path": 1}},
        fallback=True,
    ),
    Case(
        "exception-boundary-fallback",
        source='<?php try { throw new Exception("x"); } catch (Exception $e) { echo $e->getMessage(); }',
        floors={"frame_allocations": 1},
        fallback=True,
    ),
)


def prepare_include_support(out_dir: Path) -> None:
    include_value = out_dir / "fixtures" / "include-value.php"
    include_value.parent.mkdir(parents=True, exist_ok=True)
    include_value.write_text("<?php return 7;\n", encoding="utf-8")


def render_markdown(summary: dict[str, Any]) -> str:
    lines = [
        "# Managed Fast Coverage",
        "",
        "Generated by `nix develop -c just managed-fast-coverage`.",
        "Raw run artifacts are local-only under `target/performance/managed-fast/`.",
        "",
        f"- Status: `{summary['status']}`",
        f"- Cases: {summary['case_count']}",
        f"- Intentional fallback cases: {summary['fallback_case_count']}",
        f"- Failures: {len(summary['failures'])}",
        "",
        "## Cases",
        "",
        "| Case | Fallback | Native candidates | Native executions | Platform unavailable |",
        "| --- | --- | ---: | ---: | ---: |",
    ]
    for row in summary["cases"]:
        lines.append(
            f"| `{row['label']}` | `{row['fallback']}` | {row['native_candidates']} | "
            f"{row['native_executions']} | {row['native_platform_unavailable']} |"
        )
    if summary["failures"]:
        lines.extend(["", "## Failures", ""])
        lines.extend(f"- {failure}" for failure in summary["failures"])
    return "\n".join(lines) + "\n"


def self_test() -> int:
    counters = {
        "jit_mode": DEFAULT_JIT_MODE,
        "native_compiled_regions": 0,
        "native_executions": 0,
        "native_platform_unavailable": 0,
        "superinstructions_executed": {"load_const_echo": 1},
        "array_fast_path_hits_by_family": {
            "packed_int_fetch": 1,
            "packed_foreach_by_value": 1,
        },
        "intrinsic_hits": {"strlen": 1},
        "builtin_fast_stub_hits": {"strlen": 1},
        "property_fetch_profiles": [{"non_eligible_reasons": ["magic_get_present"]}],
    }
    checks = predicate_checks(counters)
    required = (
        "superinstructions_executed",
        "array_fast_paths_specific",
        "builtin_intrinsics_specific",
        "native_policy",
        "magic_get_reason",
    )
    missing = [name for name in required if not checks[name]()]
    if missing:
        print(f"[fail] self-test predicate failures: {', '.join(missing)}", file=sys.stderr)
        return 1
    print("[pass] managed fast coverage self-test")
    return 0


def main() -> int:
    args = parse_args()
    if args.self_test:
        return self_test()
    if not args.engine.is_file():
        print(f"[fail] engine not found: {args.engine}", file=sys.stderr)
        return 1
    args.out_dir.mkdir(parents=True, exist_ok=True)
    prepare_include_support(args.out_dir)
    failures: list[str] = []
    rows: list[dict[str, Any]] = []
    for case in CASES:
        baseline = run_case(args.engine, case, "baseline", args.out_dir, args.timeout)
        default = run_case(args.engine, case, None, args.out_dir, args.timeout)
        failures.extend(assert_case(case, baseline, default))
        rows.append(
            {
                "label": case.label,
                "fallback": case.fallback,
                "native_candidates": int_counter(default.counters, "native_candidates"),
                "native_executions": int_counter(default.counters, "native_executions"),
                "native_platform_unavailable": int_counter(
                    default.counters, "native_platform_unavailable"
                ),
            }
        )
    failures.extend(
        f"cache-reuse: {failure}"
        for failure in run_cache_reuse(args.engine, args.out_dir, args.timeout)
    )
    summary = {
        "status": "pass" if not failures else "fail",
        "case_count": len(CASES),
        "fallback_case_count": sum(1 for case in CASES if case.fallback),
        "failures": failures,
        "cases": rows,
    }
    (args.out_dir / "summary.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    (args.out_dir / "summary.md").write_text(render_markdown(summary), encoding="utf-8")
    if failures:
        print(f"[fail] managed fast coverage found {len(failures)} failure(s)", file=sys.stderr)
        for failure in failures:
            print(f"[fail] {failure}", file=sys.stderr)
        return 1
    print(
        "[pass] managed fast coverage checked "
        f"{len(CASES)} fixture(s); wrote {rel(args.out_dir / 'summary.json')} "
        f"and {rel(args.out_dir / 'summary.md')}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
