#!/usr/bin/env python3
"""Run deterministic performance benchmark smoke scenarios and emit JSON."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import statistics
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from normalize_perf_output import normalize


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_FIXTURES = ROOT / "tests/fixtures/performance/perf_smoke"
DEFAULT_ENGINE = ROOT / "target/debug/php-vm"
DEFAULT_OUT = ROOT / "target/performance/bench-performance-smoke.json"
DEFAULT_REFERENCE = ROOT / "third_party/php-src/sapi/cli/php"
RUN_ID = "performance-bench-matrix"


@dataclass(frozen=True)
class Engine:
    key: str
    path: Path
    args_before_fixture: tuple[str, ...]


@dataclass(frozen=True)
class ProcessSample:
    elapsed_ms: float
    returncode: int
    stdout: str
    stderr: str
    counters: dict[str, Any] | None
    timings: dict[str, Any] | None
    timing_warning: str | None = None


def positive_int(value: str) -> int:
    parsed = int(value)
    if parsed < 0:
        raise argparse.ArgumentTypeError("value must be non-negative")
    return parsed


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--fixtures-dir", type=Path, default=DEFAULT_FIXTURES)
    parser.add_argument("--engine", type=Path, default=Path(os.getenv("PHRUST_PHP_VM", DEFAULT_ENGINE)))
    parser.add_argument("--reference-php", type=Path, default=None)
    parser.add_argument("--out", type=Path, default=DEFAULT_OUT)
    parser.add_argument("--repetitions", type=positive_int, default=int(os.getenv("PHRUST_PERF_BENCH_REPETITIONS", "1")))
    parser.add_argument("--warmups", type=positive_int, default=int(os.getenv("PHRUST_PERF_BENCH_WARMUPS", "0")))
    parser.add_argument("--timeout", type=float, default=float(os.getenv("PHRUST_PERF_BENCH_TIMEOUT", "5.0")))
    parser.add_argument(
        "--no-counters",
        action="store_true",
        help="Do not collect Rust VM counters in benchmark measurements.",
    )
    parser.add_argument(
        "--opt-flag",
        action="append",
        default=[],
        help="Rust VM run flag to pass before the fixture and record in JSON.",
    )
    parser.add_argument(
        "--jit",
        choices=("off", "noop", "cranelift"),
        default=None,
        help="Rust VM JIT mode to pass through before the fixture.",
    )
    parser.add_argument(
        "--jit-threshold",
        type=positive_int,
        default=None,
        help="Rust VM JIT hot-call threshold to pass through before the fixture.",
    )
    parser.add_argument(
        "--jit-dump-clif",
        type=Path,
        default=None,
        help="Rust VM JIT CLIF dump path to pass through before the fixture.",
    )
    parser.add_argument(
        "--jit-stats",
        choices=("json",),
        default=None,
        help="Rust VM JIT stats format to pass through before the fixture.",
    )
    parser.add_argument(
        "--reference-flag",
        action="append",
        default=[],
        help="Reference PHP CLI flag to pass before the fixture.",
    )
    parser.add_argument("--self-test", action="store_true")
    return parser.parse_args()


def jit_run_flags(args: argparse.Namespace) -> tuple[str, ...]:
    flags: list[str] = []
    if args.jit is not None:
        flags.append(f"--jit={args.jit}")
    if args.jit_threshold is not None:
        flags.append(f"--jit-threshold={args.jit_threshold}")
    if args.jit_dump_clif is not None:
        flags.append(f"--jit-dump-clif={args.jit_dump_clif}")
    if args.jit_stats is not None:
        flags.append(f"--jit-stats={args.jit_stats}")
    return tuple(flags)


def rel(path: Path) -> str:
    try:
        return path.resolve().relative_to(ROOT).as_posix()
    except ValueError:
        return path.as_posix()


def fixture_paths(fixtures_dir: Path) -> list[Path]:
    if not fixtures_dir.is_dir():
        raise SystemExit(f"fixtures directory not found: {fixtures_dir}")
    fixtures = sorted(path for path in fixtures_dir.glob("*.php") if path.is_file())
    if not fixtures:
        raise SystemExit(f"no benchmark fixtures found under {fixtures_dir}")
    for fixture in fixtures:
        expected = fixture.with_name(f"{fixture.name}.out")
        if not expected.is_file():
            raise SystemExit(f"missing expected output for {fixture}: {expected}")
    return fixtures


def reference_php_path(explicit: Path | None) -> tuple[Path | None, str | None]:
    if explicit is not None:
        if not explicit.is_file() or not os.access(explicit, os.X_OK):
            raise SystemExit(f"reference PHP is not executable: {explicit}")
        return explicit, None
    env_path = os.getenv("REFERENCE_PHP")
    if env_path:
        path = Path(env_path)
        if not path.is_file() or not os.access(path, os.X_OK):
            raise SystemExit(f"REFERENCE_PHP is not executable: {path}")
        return path, None
    if DEFAULT_REFERENCE.is_file() and os.access(DEFAULT_REFERENCE, os.X_OK):
        return DEFAULT_REFERENCE, None
    return None, "REFERENCE_PHP not set and third_party/php-src/sapi/cli/php is unavailable"


def normalized_env(tmp_dir: Path) -> dict[str, str]:
    env = dict(os.environ)
    env.update(
        {
            "TZ": "UTC",
            "LC_ALL": "C",
            "LANG": "C",
            "TMPDIR": str(tmp_dir),
            "TMP": str(tmp_dir),
            "TEMP": str(tmp_dir),
            "PHRUST_RANDOM_SEED": "performance-bench-matrix",
            "RUST_TEST_SEED": "performance-bench-matrix",
        }
    )
    return env


def command_for(
    engine: Engine,
    fixture: Path,
    counters_path: Path | None = None,
    timings_path: Path | None = None,
) -> list[str]:
    if engine.key == "rust-vm":
        counters_args = []
        if counters_path is not None:
            counters_args = ["--counters-json", str(counters_path)]
        timings_args = []
        if timings_path is not None:
            timings_args = ["--timings-json", str(timings_path)]
        return [
            str(engine.path),
            "run",
            *engine.args_before_fixture,
            *counters_args,
            *timings_args,
            rel(fixture),
        ]
    return [str(engine.path), *engine.args_before_fixture, rel(fixture)]


def display_command_for(
    engine: Engine,
    fixture: Path,
    counters_path: Path | None = None,
    timings_path: Path | None = None,
) -> list[str]:
    if engine.key == "rust-vm":
        counters_args = []
        if counters_path is not None:
            counters_args = ["--counters-json", rel(counters_path)]
        timings_args = []
        if timings_path is not None:
            timings_args = ["--timings-json", rel(timings_path)]
        return [
            rel(engine.path),
            "run",
            *engine.args_before_fixture,
            *counters_args,
            *timings_args,
            rel(fixture),
        ]
    return [rel(engine.path), *engine.args_before_fixture, rel(fixture)]


def load_json_object(path: Path, label: str) -> tuple[dict[str, Any] | None, str | None]:
    if not path.is_file():
        return None, f"{label} missing: {rel(path)}"
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        return None, f"{label} malformed: {rel(path)}: {error}"
    if not isinstance(data, dict):
        return None, f"{label} malformed: {rel(path)}: root is not an object"
    return data, None


def phase_ms(timings: dict[str, Any], name: str) -> float:
    phases = timings.get("phases")
    if not isinstance(phases, dict):
        return 0.0
    value = phases.get(name)
    return float(value) if isinstance(value, (int, float)) else 0.0


def compile_total_ms(timings: dict[str, Any]) -> float:
    return sum(
        phase_ms(timings, name)
        for name in (
            "cache_prepare_ms",
            "cache_load_ms",
            "cache_store_ms",
            "source_read_ms",
            "frontend_analyze_ms",
            "ir_lower_ms",
            "ir_verify_ms",
            "optimizer_ms",
            "bytecode_lower_ms",
            "bytecode_layout_ms",
            "superinstruction_select_ms",
            "vm_construct_ms",
            "diagnostics_ms",
            "counters_write_ms",
            "timings_write_ms",
        )
    )


def derived_timing_metrics(samples: list[ProcessSample]) -> list[dict[str, Any]]:
    timing_samples = [sample for sample in samples if sample.timings is not None]
    if not timing_samples:
        return []
    external = statistics.median(sample.elapsed_ms for sample in timing_samples)
    internal = statistics.median(
        float(sample.timings.get("total_internal_ms", 0.0))
        for sample in timing_samples
        if sample.timings is not None
    )
    compile_total = statistics.median(
        compile_total_ms(sample.timings)
        for sample in timing_samples
        if sample.timings is not None
    )
    execute = statistics.median(
        phase_ms(sample.timings, "execute_ms")
        for sample in timing_samples
        if sample.timings is not None
    )
    startup_external = max(external - internal, 0.0)
    compile_share = (compile_total / internal * 100.0) if internal > 0 else 0.0
    execute_share = (execute / internal * 100.0) if internal > 0 else 0.0
    return [
        {"name": "external_wall_ms", "unit": "ms", "value": external, "lower_is_better": True},
        {"name": "internal_total_ms", "unit": "ms", "value": internal, "lower_is_better": True},
        {
            "name": "startup_external_ms",
            "unit": "ms",
            "value": startup_external,
            "lower_is_better": True,
        },
        {"name": "compile_total_ms", "unit": "ms", "value": compile_total, "lower_is_better": True},
        {"name": "execute_ms", "unit": "ms", "value": execute, "lower_is_better": True},
        {
            "name": "compile_share_percent",
            "unit": "percent",
            "value": compile_share,
            "lower_is_better": True,
        },
        {
            "name": "execute_share_percent",
            "unit": "percent",
            "value": execute_share,
            "lower_is_better": True,
        },
    ]


def run_process(
    engine: Engine,
    fixture: Path,
    tmp_dir: Path,
    timeout: float,
    counters_path: Path | None,
    timings_path: Path | None,
) -> ProcessSample:
    tmp_dir.mkdir(parents=True, exist_ok=True)
    if counters_path is not None:
        counters_path.parent.mkdir(parents=True, exist_ok=True)
        counters_path.unlink(missing_ok=True)
    if timings_path is not None:
        timings_path.parent.mkdir(parents=True, exist_ok=True)
        timings_path.unlink(missing_ok=True)
    started = time.perf_counter_ns()
    completed = subprocess.run(
        command_for(engine, fixture, counters_path, timings_path),
        cwd=ROOT,
        env=normalized_env(tmp_dir),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
        check=False,
    )
    elapsed_ms = (time.perf_counter_ns() - started) / 1_000_000.0
    counters = None
    if counters_path is not None and counters_path.is_file():
        counters = json.loads(counters_path.read_text(encoding="utf-8"))
    timings = None
    timing_warning = None
    if timings_path is not None:
        timings, timing_warning = load_json_object(timings_path, "timings")
    return ProcessSample(
        elapsed_ms=elapsed_ms,
        returncode=completed.returncode,
        stdout=completed.stdout.replace("\r\n", "\n").replace("\r", "\n"),
        stderr=normalize(completed.stderr),
        counters=counters,
        timings=timings,
        timing_warning=timing_warning,
    )


def sample_metrics(samples: list[ProcessSample]) -> list[dict[str, Any]]:
    elapsed = [sample.elapsed_ms for sample in samples]
    return [
        {
            "name": "wall_time_min",
            "unit": "ms",
            "value": min(elapsed),
            "lower_is_better": True,
        },
        {
            "name": "wall_time_median",
            "unit": "ms",
            "value": statistics.median(elapsed),
            "lower_is_better": True,
        },
        {
            "name": "wall_time_max",
            "unit": "ms",
            "value": max(elapsed),
            "lower_is_better": True,
        },
        {
            "name": "wall_time_mean",
            "unit": "ms",
            "value": statistics.fmean(elapsed),
            "lower_is_better": True,
        },
        {
            "name": "stdout_bytes",
            "unit": "bytes",
            "value": len(samples[-1].stdout.encode()),
            "lower_is_better": False,
        },
        {
            "name": "stderr_bytes",
            "unit": "bytes",
            "value": len(samples[-1].stderr.encode()),
            "lower_is_better": False,
        },
        {
            "name": "exit_code",
            "unit": "code",
            "value": samples[-1].returncode,
            "lower_is_better": True,
        },
    ]


def measure_fixture(
    engine: Engine,
    fixture: Path,
    expected_stdout: str,
    repetitions: int,
    warmups: int,
    timeout: float,
    collect_counters: bool,
) -> tuple[dict[str, Any], bool]:
    scenario_key = fixture.stem.replace(".", "_").replace("-", "_")
    tmp_base = ROOT / "target/performance/tmp" / engine.key / scenario_key
    for index in range(warmups):
        timings_path = None
        if engine.key == "rust-vm":
            timings_path = (
                ROOT / "target/performance/timings/bench" / scenario_key / engine.key / f"warmup-{index}.json"
            )
        run_process(engine, fixture, tmp_base / f"warmup-{index}", timeout, None, timings_path)

    samples = []
    for index in range(max(repetitions, 1)):
        counters_path = None
        if collect_counters and engine.key == "rust-vm":
            counters_path = (
                ROOT / "target/performance/counters" / scenario_key / f"repeat-{index}.json"
            )
        timings_path = None
        if engine.key == "rust-vm":
            timings_path = (
                ROOT / "target/performance/timings/bench" / scenario_key / engine.key / f"repeat-{index}.json"
            )
        samples.append(
            run_process(
                engine,
                fixture,
                tmp_base / f"repeat-{index}",
                timeout,
                counters_path,
                timings_path,
            )
        )
    last = samples[-1]
    output_matches = last.stdout == expected_stdout
    status = "pass" if last.returncode == 0 and output_matches else "fail"
    display_counters_path = None
    if collect_counters and engine.key == "rust-vm":
        display_counters_path = ROOT / "target/performance/counters" / scenario_key / "repeat-N.json"
    display_timings_path = None
    if engine.key == "rust-vm":
        display_timings_path = (
            ROOT / "target/performance/timings/bench" / scenario_key / engine.key / "repeat-N.json"
        )
    measurement = {
        "scenario": {
            "id": f"performance.perf_smoke.{engine.key}.{scenario_key}",
            "name": f"{fixture.stem} ({engine.key})",
            "group": f"perf_smoke.{engine.key}",
            "fixture": rel(fixture),
        },
        "engine": engine.key,
        "engine_path": rel(engine.path),
        "command": display_command_for(engine, fixture, display_counters_path, display_timings_path),
        "iterations": max(repetitions, 1),
        "warmups": warmups,
        "metrics": [*sample_metrics(samples), *derived_timing_metrics(samples)],
        "wall_time_ms": statistics.median(sample.elapsed_ms for sample in samples),
        "status": status,
        "stdout_sha256": hashlib.sha256(last.stdout.encode()).hexdigest(),
        "expected_stdout_sha256": hashlib.sha256(expected_stdout.encode()).hexdigest(),
        "stderr": last.stderr,
    }
    if last.counters is not None:
        measurement["vm_counters"] = last.counters
    if last.timings is not None:
        measurement["phase_timings"] = last.timings
    timing_warnings = [sample.timing_warning for sample in samples if sample.timing_warning]
    if timing_warnings:
        measurement["timing_warnings"] = timing_warnings
    return measurement, status == "pass"


def run_self_test() -> int:
    report = {
        "total_internal_ms": 8.0,
        "phases": {
            "source_read_ms": 1.0,
            "frontend_analyze_ms": 2.0,
            "ir_lower_ms": 1.0,
            "execute_ms": 3.0,
            "timings_write_ms": 1.0,
        },
    }
    sample = ProcessSample(10.0, 0, "ok\n", "", {}, report)
    metrics = {item["name"]: item["value"] for item in derived_timing_metrics([sample])}
    assert metrics["external_wall_ms"] == 10.0
    assert metrics["internal_total_ms"] == 8.0
    assert metrics["startup_external_ms"] == 2.0
    assert metrics["compile_total_ms"] == 5.0
    assert metrics["execute_ms"] == 3.0
    assert metrics["compile_share_percent"] == 62.5
    assert metrics["execute_share_percent"] == 37.5
    print("[pass] bench_matrix self-test")
    return 0


def git_commit() -> str | None:
    try:
        completed = subprocess.run(
            ["git", "rev-parse", "--short=12", "HEAD"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            timeout=2.0,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired):
        return None
    if completed.returncode != 0:
        return None
    return completed.stdout.strip() or None


def report_environment(args: argparse.Namespace, reference_skip: str | None) -> dict[str, Any]:
    extra = {
        "tz": "UTC",
        "lc_all": "C",
        "tmp_root": "target/performance/tmp",
        "deterministic_seed": "performance-bench-matrix",
        "python": platform.python_version(),
        "platform": platform.platform(),
    }
    if reference_skip is not None:
        extra["reference_php_skip"] = reference_skip
    return {
        "engine_version": RUN_ID,
        "git_commit": git_commit(),
        "rust_target_triple": platform.machine() + "-" + platform.system().lower(),
        "opt_flags": args.opt_flag,
        "jit_flags": list(jit_run_flags(args)),
        "feature_flags": {},
        "extra": extra,
    }


def main() -> int:
    args = parse_args()
    if args.self_test:
        return run_self_test()
    if not args.engine.is_file() or not os.access(args.engine, os.X_OK):
        raise SystemExit(f"Rust engine is not executable: {args.engine}")
    if args.timeout <= 0:
        raise SystemExit("--timeout must be positive")

    fixtures = fixture_paths(args.fixtures_dir)
    reference_php, reference_skip = reference_php_path(args.reference_php)
    rust_vm_flags = tuple(args.opt_flag) + jit_run_flags(args)
    engines = [Engine("rust-vm", args.engine, rust_vm_flags)]
    if reference_php is not None:
        engines.append(Engine("reference-php", reference_php, tuple(args.reference_flag)))

    measurements: list[dict[str, Any]] = []
    failed = False
    for fixture in fixtures:
        expected = fixture.with_name(f"{fixture.name}.out").read_text(encoding="utf-8")
        for engine in engines:
            measurement, ok = measure_fixture(
                engine,
                fixture,
                expected,
                args.repetitions,
                args.warmups,
                args.timeout,
                not args.no_counters,
            )
            measurements.append(measurement)
            failed = failed or not ok
    if not args.no_counters:
        rust_counter_measurements = [
            measurement
            for measurement in measurements
            if measurement["engine"] == "rust-vm" and measurement.get("vm_counters") is not None
        ]
        if not rust_counter_measurements:
            print("[fail] no Rust VM counter samples were recorded", file=sys.stderr)
            failed = True
        elif not any(
            measurement["vm_counters"].get("literal_intern_hits", 0) > 0
            for measurement in rust_counter_measurements
        ):
            print("[fail] Rust VM counters recorded no literal intern hits", file=sys.stderr)
            failed = True

    report = {
        "schema_version": 1,
        "run_id": RUN_ID,
        "environment": report_environment(args, reference_skip),
        "measurements": measurements,
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    if failed:
        print(f"[fail] performance benchmark smoke wrote {rel(args.out)}", file=sys.stderr)
        return 1
    print(f"[pass] performance benchmark smoke wrote {rel(args.out)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
