#!/usr/bin/env python3
"""Generate the performance Cranelift Big-Win benchmark matrix."""

from __future__ import annotations

import argparse
import json
import os
import platform
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[3]
DEFAULT_ENGINE = ROOT / "target/debug/php-vm"
DEFAULT_OUT = ROOT / "target/performance/cranelift/big_wins_report.json"

SCENARIOS = (
    {
        "scenario": "repeated_int_function_calls",
        "target": "integer_arithmetic_leaf",
        "fixture": ROOT / "tests/fixtures/performance/cranelift/int_arithmetic/repeated-function-call-add.php",
        "expected_cranelift_status": "executed",
        "known_gaps": [],
    },
    {
        "scenario": "arithmetic_expression_chain",
        "target": "integer_arithmetic_leaf",
        "fixture": ROOT / "tests/fixtures/performance/cranelift/int_arithmetic/arithmetic-expression-chain.php",
        "expected_cranelift_status": "executed",
        "known_gaps": [],
    },
    {
        "scenario": "negative_ints",
        "target": "integer_arithmetic_leaf",
        "fixture": ROOT / "tests/fixtures/performance/cranelift/int_arithmetic/negative-ints.php",
        "expected_cranelift_status": "executed",
        "known_gaps": [],
    },
    {
        "scenario": "boundary_ints",
        "target": "integer_arithmetic_leaf",
        "fixture": ROOT / "tests/fixtures/performance/cranelift/int_arithmetic/boundary-ints.php",
        "expected_cranelift_status": "executed",
        "known_gaps": [],
    },
    {
        "scenario": "counted_loop_accumulator",
        "target": "counted_loop",
        "fixture": ROOT / "tests/fixtures/performance/cranelift/int_arithmetic/counted-loop-accumulator.php",
        "expected_cranelift_status": "executed",
        "known_gaps": [],
    },
    {
        "scenario": "factorial_like_loop",
        "target": "counted_loop",
        "fixture": ROOT / "tests/fixtures/performance/cranelift/loops/factorial-like-loop.php",
        "expected_cranelift_status": "executed",
        "known_gaps": [],
    },
    {
        "scenario": "sum_to_n",
        "target": "counted_loop",
        "fixture": ROOT / "tests/fixtures/performance/cranelift/loops/sum-1-to-n.php",
        "expected_cranelift_status": "executed",
        "known_gaps": [],
    },
    {
        "scenario": "fib_iterative",
        "target": "counted_loop",
        "fixture": ROOT / "tests/fixtures/performance/cranelift/loops/fib-iterative.php",
        "expected_cranelift_status": "executed",
        "known_gaps": [],
    },
    {
        "scenario": "branchy_int_loop",
        "target": "counted_loop",
        "fixture": ROOT / "tests/fixtures/performance/cranelift/loops/branchy-int-loop.php",
        "expected_cranelift_status": "executed",
        "known_gaps": [],
    },
    {
        "scenario": "branchy_max_min",
        "target": "branchy_int",
        "fixture": ROOT / "tests/fixtures/performance/cranelift/loops/branchy-max-min.php",
        "expected_cranelift_status": "executed",
        "known_gaps": [],
    },
    {
        "scenario": "non_eligible_loop_call",
        "target": "counted_loop",
        "fixture": ROOT / "tests/fixtures/performance/cranelift/loops/non-eligible-loop-call.php",
        "expected_cranelift_status": "fallback",
        "known_gaps": ["CL-GAP-LOOP-BODY-CALL"],
    },
    {
        "scenario": "packed_array_int_fetch",
        "target": "packed_array_fetch",
        "fixture": ROOT / "tests/fixtures/performance/cranelift/arrays/packed-fetch-valid.php",
        "expected_cranelift_status": "executed",
        "expected_helper_calls": 1,
        "known_gaps": [],
    },
    {
        "scenario": "packed_foreach_int_sum",
        "target": "packed_foreach_sum",
        "fixture": ROOT / "tests/fixtures/performance/cranelift/arrays/packed-foreach-sum-large.php",
        "expected_cranelift_status": "executed",
        "known_gaps": [],
    },
    {
        "scenario": "packed_foreach_mixed_element",
        "target": "packed_foreach_sum",
        "fixture": ROOT / "tests/fixtures/performance/cranelift/arrays/packed-foreach-sum-mixed-element.php",
        "expected_cranelift_status": "side_exit",
        "expected_side_exit_reason": "helper_status",
        "expected_counter": "packed_foreach_sum_layout_exits",
        "known_gaps": [],
    },
    {
        "scenario": "packed_foreach_overflow",
        "target": "packed_foreach_sum",
        "fixture": ROOT / "tests/fixtures/performance/cranelift/arrays/packed-foreach-sum-overflow.php",
        "expected_cranelift_status": "side_exit",
        "expected_side_exit_reason": "overflow",
        "expected_counter": "packed_foreach_sum_overflow_exits",
        "known_gaps": [],
    },
    {
        "scenario": "known_strlen_valid",
        "target": "known_call",
        "fixture": ROOT / "tests/fixtures/performance/cranelift/known-calls/strlen-valid.php",
        "expected_cranelift_status": "executed",
        "expected_helper_calls": 1,
        "known_gaps": [],
    },
    {
        "scenario": "known_strlen_non_string",
        "target": "known_call",
        "fixture": ROOT / "tests/fixtures/performance/cranelift/known-calls/strlen-non-string.php",
        "expected_cranelift_status": "side_exit",
        "expected_side_exit_reason": "helper_status",
        "expected_counter": "known_call_guard_exits",
        "known_gaps": [],
    },
    {
        "scenario": "known_count_packed",
        "target": "known_call",
        "fixture": ROOT / "tests/fixtures/performance/cranelift/known-calls/count-packed.php",
        "expected_cranelift_status": "executed",
        "expected_helper_calls": 1,
        "known_gaps": [],
    },
    {
        "scenario": "known_count_mixed",
        "target": "known_call",
        "fixture": ROOT / "tests/fixtures/performance/cranelift/known-calls/count-mixed.php",
        "expected_cranelift_status": "executed",
        "expected_helper_calls": 1,
        "known_gaps": [],
    },
    {
        "scenario": "known_strlen_wrong_arity",
        "target": "known_call",
        "fixture": ROOT / "tests/fixtures/performance/cranelift/known-calls/strlen-wrong-arity.php",
        "expected_cranelift_status": "fallback",
        "known_gaps": [],
    },
    {
        "scenario": "string_concat_two_strings",
        "target": "string_concat",
        "fixture": ROOT / "tests/fixtures/performance/cranelift/string-concat/two-strings.php",
        "expected_cranelift_status": "executed",
        "expected_helper_calls": 1,
        "known_gaps": [],
    },
    {
        "scenario": "string_concat_empty_strings",
        "target": "string_concat",
        "fixture": ROOT / "tests/fixtures/performance/cranelift/string-concat/empty-strings.php",
        "expected_cranelift_status": "executed",
        "expected_helper_calls": 1,
        "known_gaps": [],
    },
    {
        "scenario": "string_concat_large_strings",
        "target": "string_concat",
        "fixture": ROOT / "tests/fixtures/performance/cranelift/string-concat/large-strings.php",
        "expected_cranelift_status": "executed",
        "expected_helper_calls": 1,
        "known_gaps": [],
    },
    {
        "scenario": "string_concat_template_loop",
        "target": "string_concat",
        "fixture": ROOT / "tests/fixtures/performance/cranelift/string-concat/template-loop.php",
        "expected_cranelift_status": "executed",
        "expected_helper_calls": 64,
        "known_gaps": [],
    },
    {
        "scenario": "string_concat_string_int",
        "target": "string_concat",
        "fixture": ROOT / "tests/fixtures/performance/cranelift/string-concat/string-int-slow.php",
        "expected_cranelift_status": "fallback",
        "known_gaps": [],
    },
    {
        "scenario": "string_concat_object_to_string",
        "target": "string_concat",
        "fixture": ROOT / "tests/fixtures/performance/cranelift/string-concat/object-to-string-slow.php",
        "expected_cranelift_status": "fallback",
        "known_gaps": [],
    },
    {
        "scenario": "property_load_simple_dto",
        "target": "property_load",
        "fixture": ROOT / "tests/fixtures/performance/cranelift/property-load/simple-dto-property-read.php",
        "expected_cranelift_status": "executed",
        "expected_helper_calls": 1,
        "known_gaps": [],
    },
    {
        "scenario": "property_load_dto_loop",
        "target": "property_load",
        "fixture": ROOT / "tests/fixtures/performance/cranelift/property-load/repeated-property-read-loop.php",
        "expected_cranelift_status": "executed",
        "expected_helper_calls": 64,
        "known_gaps": [],
    },
    {
        "scenario": "property_load_wrong_class_side_exit",
        "target": "property_load",
        "fixture": ROOT / "tests/fixtures/performance/cranelift/property-load/wrong-class-side-exit.php",
        "expected_cranelift_status": "side_exit",
        "expected_side_exit_reason": "guard_failed",
        "expected_counter": "property_load_guard_exits",
        "known_gaps": [],
    },
    {
        "scenario": "property_load_hook_magic_fallback",
        "target": "property_load",
        "fixture": ROOT / "tests/fixtures/performance/cranelift/property-load/hook-magic-fallback.php",
        "expected_cranelift_status": "fallback",
        "known_gaps": [],
    },
    {
        "scenario": "property_load_uninitialized_error_path",
        "target": "property_load",
        "fixture": ROOT / "tests/fixtures/performance/cranelift/property-load/uninitialized-error-path.php",
        "expected_cranelift_status": "side_exit",
        "expected_side_exit_reason": "guard_failed",
        "expected_counter": "property_load_uninitialized_exits",
        "known_gaps": [],
    },
    {
        "scenario": "method_call_service_dto_loop",
        "target": "method_call",
        "fixture": ROOT / "tests/fixtures/performance/cranelift/method-call/service-dto-loop.php",
        "expected_cranelift_status": "executed",
        "known_gaps": [],
    },
    {
        "scenario": "tiering_cold_threshold",
        "target": "tiering_policy",
        "fixture": ROOT / "tests/fixtures/performance/cranelift/native/return-42.php",
        "expected_cranelift_status": "fallback",
        "tiering_mode": "threshold",
        "jit_threshold": 8,
        "known_gaps": [],
    },
    {
        "scenario": "tiering_hot_threshold",
        "target": "tiering_policy",
        "fixture": ROOT / "tests/fixtures/performance/cranelift/int_arithmetic/repeated-function-call-add.php",
        "expected_cranelift_status": "executed",
        "tiering_mode": "threshold",
        "jit_threshold": 2,
        "known_gaps": [],
    },
    {
        "scenario": "overflow_correctness",
        "target": "integer_arithmetic_leaf",
        "fixture": ROOT / "tests/fixtures/performance/cranelift/int_arithmetic/overflow-correctness.php",
        "expected_cranelift_status": "side_exit",
        "known_gaps": [],
    },
)

REQUIRED_BIG_WIN_FAMILIES = {
    "int_leaf_calls": "int leaf calls",
    "int_counted_loop": "int counted loop",
    "packed_int_fetch": "packed int fetch",
    "packed_foreach_sum": "packed foreach sum",
    "known_strlen_count_call": "strlen/count known call",
    "string_concat": "string concat",
    "property_read_loop": "property read loop",
    "method_call_loop": "method call loop",
}


@dataclass(frozen=True)
class Run:
    command: list[str]
    returncode: int
    stdout: str
    stderr: str
    jit_stats: dict[str, Any] | None
    wall_time_seconds: float


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--engine", type=Path, default=Path(os.getenv("PHRUST_PHP_VM", DEFAULT_ENGINE)))
    parser.add_argument("--out", type=Path, default=DEFAULT_OUT)
    parser.add_argument(
        "--reference-php",
        type=Path,
        default=Path(os.environ["REFERENCE_PHP"]) if os.getenv("REFERENCE_PHP") else None,
        help="Optional PHP reference binary. Advisory only; mismatches do not fail the gate.",
    )
    parser.add_argument("--repeats", type=int, default=int(os.getenv("PHRUST_CRANELIFT_BENCH_REPEATS", "3")))
    parser.add_argument("--warmups", type=int, default=int(os.getenv("PHRUST_CRANELIFT_BENCH_WARMUPS", "1")))
    parser.add_argument("--timeout", type=float, default=float(os.getenv("PHRUST_CRANELIFT_BENCH_TIMEOUT", "10.0")))
    parser.add_argument("--smoke", action="store_true")
    return parser.parse_args()


def rel(path: Path) -> str:
    try:
        return path.resolve().relative_to(ROOT).as_posix()
    except ValueError:
        return path.as_posix()


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
            "PHRUST_RANDOM_SEED": "performance-cranelift-bench-matrix",
            "RUST_TEST_SEED": "performance-cranelift-bench-matrix",
        }
    )
    return env


def normalize_text(value: str) -> str:
    return value.replace("\r\n", "\n").replace("\r", "\n")


def extract_jit_stats(stderr: str) -> dict[str, Any] | None:
    for line in stderr.splitlines():
        stripped = line.strip()
        if not stripped.startswith("{"):
            continue
        try:
            decoded = json.loads(stripped)
        except json.JSONDecodeError:
            continue
        jit = decoded.get("jit") if isinstance(decoded, dict) else None
        if isinstance(jit, dict):
            return jit
    return None


def stderr_without_jit_stats(stderr: str) -> str:
    lines = []
    for line in stderr.splitlines():
        stripped = line.strip()
        if stripped.startswith("{"):
            try:
                decoded = json.loads(stripped)
            except json.JSONDecodeError:
                pass
            else:
                if isinstance(decoded, dict) and isinstance(decoded.get("jit"), dict):
                    continue
        lines.append(line)
    return "\n".join(lines) + ("\n" if lines else "")


def tiering_mode_for(scenario: dict[str, Any]) -> str:
    return str(scenario.get("tiering_mode", "eager"))


def command_for(engine: Path, fixture: Path, mode: str, scenario: dict[str, Any]) -> list[str]:
    command = [
        str(engine),
        "run",
        f"--jit={mode}",
        "--jit-stats=json",
    ]
    if mode == "cranelift":
        tiering_mode = tiering_mode_for(scenario)
        if tiering_mode == "eager":
            command.append("--jit-eager")
        elif tiering_mode == "threshold":
            command.append(f"--jit-threshold={int(scenario.get('jit_threshold', 8))}")
        else:
            raise ValueError(f"unsupported tiering mode: {tiering_mode}")
    command.append(rel(fixture))
    return command


def reference_command_for(reference_php: Path, fixture: Path) -> list[str]:
    return [str(reference_php), rel(fixture)]


def run_engine(
    engine: Path,
    fixture: Path,
    mode: str,
    tmp_dir: Path,
    timeout: float,
    scenario: dict[str, Any],
) -> Run:
    tmp_dir.mkdir(parents=True, exist_ok=True)
    command = command_for(engine, fixture, mode, scenario)
    environment = normalized_env(tmp_dir)
    # This matrix isolates Cranelift from the earlier copy-and-patch tier.
    # Copy-and-patch is default-on and otherwise consumes the same leaf calls
    # before either the interpreter baseline or Cranelift can observe them.
    environment["PHRUST_JIT_COPY_PATCH"] = "0"
    started = time.perf_counter()
    completed = subprocess.run(
        command,
        cwd=ROOT,
        env=environment,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
        check=False,
    )
    wall_time_seconds = time.perf_counter() - started
    stderr = normalize_text(completed.stderr)
    return Run(
        command=[rel(Path(command[0])), *command[1:]],
        returncode=completed.returncode,
        stdout=normalize_text(completed.stdout),
        stderr=stderr,
        jit_stats=extract_jit_stats(stderr),
        wall_time_seconds=wall_time_seconds,
    )


def run_reference(
    reference_php: Path,
    fixture: Path,
    tmp_dir: Path,
    timeout: float,
) -> Run:
    tmp_dir.mkdir(parents=True, exist_ok=True)
    command = reference_command_for(reference_php, fixture)
    started = time.perf_counter()
    completed = subprocess.run(
        command,
        cwd=ROOT,
        env=normalized_env(tmp_dir),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
        check=False,
    )
    wall_time_seconds = time.perf_counter() - started
    return Run(
        command=[rel(Path(command[0])), *command[1:]],
        returncode=completed.returncode,
        stdout=normalize_text(completed.stdout),
        stderr=normalize_text(completed.stderr),
        jit_stats=None,
        wall_time_seconds=wall_time_seconds,
    )


def require(condition: bool, failures: list[str], message: str) -> None:
    if not condition:
        failures.append(message)


def matrix_family_for(scenario: dict[str, Any]) -> str:
    target = scenario["target"]
    name = scenario["scenario"]
    if target == "integer_arithmetic_leaf":
        return "int_leaf_calls"
    if target in {"counted_loop", "branchy_int"}:
        return "int_counted_loop"
    if target == "packed_array_fetch":
        return "packed_int_fetch"
    if target == "packed_foreach_sum":
        return "packed_foreach_sum"
    if target == "known_call":
        return "known_strlen_count_call"
    if target == "string_concat":
        return "string_concat"
    if target == "property_load":
        return "property_read_loop"
    if target == "method_call":
        return "method_call_loop"
    if target == "tiering_policy":
        return "tiering_policy"
    raise ValueError(f"{name}: unmapped Big-Win matrix target {target!r}")


def jit_status_for(mode: str, stats: dict[str, Any] | None) -> str:
    if mode == "off":
        return "skipped"
    if mode == "reference_php":
        return "reference"
    if not stats:
        return "skipped"
    if stats.get("side_exits", 0) > 0:
        return "side_exit"
    if stats.get("direct_call_hits", 0) > 0:
        return "executed"
    if stats.get("executed", 0) > 0:
        return "executed"
    if stats.get("bailouts", 0) > 0 or stats.get("compiled", 0) == 0:
        return "fallback"
    return "skipped"


def path_kinds_for(mode: str, stats: dict[str, Any] | None) -> list[str]:
    if mode == "off":
        return ["interpreter_baseline"]
    if mode == "reference_php":
        return ["reference_orientation"]
    stats = stats or {}
    kinds: list[str] = []
    if (
        int(stats.get("helper_calls", 0)) > 0
        or int(stats.get("known_call_fast_hits", 0)) > 0
        or int(stats.get("property_load_fast_hits", 0)) > 0
        or int(stats.get("string_concat_fast_path_hits", 0)) > 0
    ):
        kinds.append("jit_helper_call_path")
    if (
        int(stats.get("fast_path_hits", 0)) > 0
        or int(stats.get("packed_fetch_fast_hits", 0)) > 0
        or int(stats.get("packed_foreach_sum_fast_hits", 0)) > 0
        or int(stats.get("direct_call_hits", 0)) > 0
    ):
        kinds.append("jit_fast_path")
    if int(stats.get("compiled", 0)) == 0:
        kinds.append("jit_fallback_or_skip")
    if int(stats.get("side_exits", 0)) > 0:
        kinds.append("jit_side_exit_resume")
    return kinds or ["jit_observed"]


def counters_from_stats(stats: dict[str, Any] | None) -> dict[str, int]:
    stats = stats or {}
    return {
        "compile_attempts": int(stats.get("compile_attempts", 0)),
        "compiled_regions": int(stats.get("compiled", 0)),
        "executed_regions": int(stats.get("executed", 0)),
        "bailouts": int(stats.get("bailouts", 0)),
        "code_bytes": int(stats.get("code_bytes", 0)),
        "compile_time_nanos": int(stats.get("compile_time_nanos", 0)),
        "side_exits": int(stats.get("side_exits", 0)),
        "guard_failures": int(stats.get("guard_failures", 0)),
        "blacklisted_regions": int(stats.get("blacklisted_regions", 0)),
        "tiering_cold_functions": int(stats.get("tiering_cold_functions", 0)),
        "tiering_hot_functions": int(stats.get("tiering_hot_functions", 0)),
        "tiering_eager_functions": int(stats.get("tiering_eager_functions", 0)),
        "tiering_blacklist_rejections": int(stats.get("tiering_blacklist_rejections", 0)),
        "tiering_budget_rejections": int(stats.get("tiering_budget_rejections", 0)),
        "helper_calls": int(stats.get("helper_calls", 0)),
        "fast_path_hits": int(stats.get("fast_path_hits", 0)),
        "packed_fetch_fast_hits": int(stats.get("packed_fetch_fast_hits", 0)),
        "packed_fetch_bounds_exits": int(stats.get("packed_fetch_bounds_exits", 0)),
        "packed_fetch_layout_exits": int(stats.get("packed_fetch_layout_exits", 0)),
        "packed_foreach_sum_fast_hits": int(stats.get("packed_foreach_sum_fast_hits", 0)),
        "packed_foreach_sum_layout_exits": int(stats.get("packed_foreach_sum_layout_exits", 0)),
        "packed_foreach_sum_overflow_exits": int(stats.get("packed_foreach_sum_overflow_exits", 0)),
        "known_call_fast_hits": int(stats.get("known_call_fast_hits", 0)),
        "known_call_guard_exits": int(stats.get("known_call_guard_exits", 0)),
        "known_call_slow_calls": int(stats.get("known_call_slow_calls", 0)),
        "direct_call_hits": int(stats.get("direct_call_hits", 0)),
        "direct_call_fallbacks": int(stats.get("direct_call_fallbacks", 0)),
        "property_load_fast_hits": int(stats.get("property_load_fast_hits", 0)),
        "property_load_guard_exits": int(stats.get("property_load_guard_exits", 0)),
        "property_load_layout_exits": int(stats.get("property_load_layout_exits", 0)),
        "property_load_uninitialized_exits": int(stats.get("property_load_uninitialized_exits", 0)),
        "property_load_slow_calls": int(stats.get("property_load_slow_calls", 0)),
        "string_concat_fast_path_hits": int(stats.get("string_concat_fast_path_hits", 0)),
        "string_concat_fast_path_misses": int(stats.get("string_concat_fast_path_misses", 0)),
        "overflow_exits": int(stats.get("overflow_exits", 0)),
        "slow_path_calls": int(stats.get("slow_path_calls", 0)),
        "compile_cache_hits": int(stats.get("compile_cache_hits", 0)),
        "compile_cache_misses": int(stats.get("compile_cache_misses", 0)),
        "compile_cache_invalidations": int(stats.get("compile_cache_invalidations", 0)),
    }


def summarize_runs(runs: list[Run]) -> tuple[Run, float]:
    if not runs:
        raise ValueError("cannot summarize empty runs")
    return runs[-1], sum(run.wall_time_seconds for run in runs) / len(runs)


def report_environment(repeats: int, warmups: int) -> dict[str, Any]:
    git_commit = os.getenv("GIT_COMMIT") or "unknown"
    if git_commit == "unknown":
        try:
            git_commit = subprocess.check_output(
                ["git", "rev-parse", "--short=12", "HEAD"],
                cwd=ROOT,
                text=True,
                stderr=subprocess.DEVNULL,
            ).strip()
        except (OSError, subprocess.CalledProcessError):
            git_commit = "unknown"
    rust_target = platform.machine() + "-" + platform.system().lower()
    try:
        rustc_version = subprocess.check_output(
            ["rustc", "-vV"],
            cwd=ROOT,
            text=True,
            stderr=subprocess.DEVNULL,
        )
    except (OSError, subprocess.CalledProcessError):
        pass
    else:
        for line in rustc_version.splitlines():
            if line.startswith("host: "):
                rust_target = line.removeprefix("host: ").strip()
                break
    return {
        "engine_version": "phrust-0.0.0",
        "git_commit": git_commit,
        "rust_target_triple": rust_target,
        "feature_flags": {"jit-cranelift": True},
        "cli_flags": [
            "--jit=off",
            "--jit=cranelift",
            "--jit-eager",
            "--jit-threshold=N",
            "--jit-stats=json",
        ],
        "repeats": repeats,
        "warmups": warmups,
        "timing": "wall_time_seconds informational only",
    }


def validate_scenario(
    *,
    scenario: dict[str, Any],
    off: Run,
    cranelift: Run,
    failures: list[str],
) -> str:
    name = scenario["scenario"]
    require(off.returncode == cranelift.returncode, failures, f"{name}: exit mismatch")
    require(off.stdout == cranelift.stdout, failures, f"{name}: stdout mismatch")
    require(
        stderr_without_jit_stats(off.stderr) == stderr_without_jit_stats(cranelift.stderr),
        failures,
        f"{name}: stderr mismatch after removing jit stats",
    )
    require(cranelift.jit_stats is not None, failures, f"{name}: missing Cranelift jit stats")
    status = jit_status_for("cranelift", cranelift.jit_stats)
    require(
        status == scenario["expected_cranelift_status"],
        failures,
        f"{name}: expected jit_status={scenario['expected_cranelift_status']}, got {status}",
    )
    if scenario["expected_cranelift_status"] == "executed" and cranelift.jit_stats is not None:
        require(cranelift.jit_stats.get("fast_path_hits", 0) > 0, failures, f"{name}: missing fast-path hits")
        expected_helper_calls = int(scenario.get("expected_helper_calls", 0))
        require(
            cranelift.jit_stats.get("helper_calls", 0) == expected_helper_calls,
            failures,
            f"{name}: expected helper_calls={expected_helper_calls}, got {cranelift.jit_stats.get('helper_calls')}",
        )
        if scenario["target"] != "method_call":
            require(cranelift.jit_stats.get("compile_time_nanos", 0) > 0, failures, f"{name}: missing compile time")
        if scenario["target"] == "packed_array_fetch":
            require(
                cranelift.jit_stats.get("packed_fetch_fast_hits", 0) > 0,
                failures,
                f"{name}: missing packed-fetch fast hits",
            )
            require(
                cranelift.jit_stats.get("packed_fetch_bounds_exits", 0) == 0,
                failures,
                f"{name}: unexpected packed-fetch bounds exits",
            )
            require(
                cranelift.jit_stats.get("packed_fetch_layout_exits", 0) == 0,
                failures,
                f"{name}: unexpected packed-fetch layout exits",
            )
        if scenario["target"] == "packed_foreach_sum":
            require(
                cranelift.jit_stats.get("packed_foreach_sum_fast_hits", 0) > 0,
                failures,
                f"{name}: missing packed-foreach sum native loop hits",
            )
            require(
                cranelift.jit_stats.get("packed_foreach_sum_layout_exits", 0) == 0,
                failures,
                f"{name}: unexpected packed-foreach layout exits",
            )
            require(
                cranelift.jit_stats.get("packed_foreach_sum_overflow_exits", 0) == 0,
                failures,
                f"{name}: unexpected packed-foreach overflow exits",
            )
        if scenario["target"] == "known_call":
            require(
                cranelift.jit_stats.get("known_call_fast_hits", 0) > 0,
                failures,
                f"{name}: missing known-call fast hits",
            )
            require(
                cranelift.jit_stats.get("known_call_guard_exits", 0) == 0,
                failures,
                f"{name}: unexpected known-call guard exits",
            )
            require(
                cranelift.jit_stats.get("known_call_slow_calls", 0) == 0,
                failures,
                f"{name}: unexpected known-call slow calls",
            )
        if scenario["target"] == "method_call":
            require(
                cranelift.jit_stats.get("direct_call_hits", 0) > 0,
                failures,
                f"{name}: missing direct-call hits",
            )
            require(
                cranelift.jit_stats.get("direct_call_fallbacks", 0) > 0,
                failures,
                f"{name}: missing direct-call fallbacks",
            )
        if scenario["target"] == "string_concat":
            require(
                cranelift.jit_stats.get("string_concat_fast_path_hits", 0) > 0,
                failures,
                f"{name}: missing string-concat fast-path hits",
            )
            require(
                cranelift.jit_stats.get("string_concat_fast_path_misses", 0) == 0,
                failures,
                f"{name}: unexpected string-concat guard misses",
            )
        if scenario["target"] == "property_load":
            require(
                cranelift.jit_stats.get("property_load_fast_hits", 0) > 0,
                failures,
                f"{name}: missing property-load fast hits",
            )
            require(
                cranelift.jit_stats.get("property_load_guard_exits", 0) == 0,
                failures,
                f"{name}: unexpected property-load guard exits",
            )
            require(
                cranelift.jit_stats.get("property_load_slow_calls", 0) == 0,
                failures,
                f"{name}: unexpected property-load slow calls",
            )
        if scenario["target"] == "tiering_policy":
            require(
                cranelift.jit_stats.get("compiled", 0) > 0,
                failures,
                f"{name}: hot threshold fixture should compile",
            )
            require(
                cranelift.jit_stats.get("tiering_hot_functions", 0) > 0,
                failures,
                f"{name}: hot threshold fixture should report hot tiering decisions",
            )
    if (
        scenario["target"] == "tiering_policy"
        and scenario["expected_cranelift_status"] == "fallback"
        and cranelift.jit_stats is not None
    ):
        require(
            cranelift.jit_stats.get("compiled", 0) == 0,
            failures,
            f"{name}: cold threshold fixture should not compile",
        )
        require(
            cranelift.jit_stats.get("tiering_cold_functions", 0) > 0,
            failures,
            f"{name}: cold threshold fixture should report cold tiering decisions",
        )
    if scenario["expected_cranelift_status"] == "side_exit" and cranelift.jit_stats is not None:
        require(cranelift.jit_stats.get("side_exits", 0) > 0, failures, f"{name}: missing side exits")
        expected_reason = scenario.get("expected_side_exit_reason", "overflow")
        side_exit_reasons = cranelift.jit_stats.get("side_exit_reasons", {})
        require(
            int(side_exit_reasons.get(expected_reason, 0)) > 0,
            failures,
            f"{name}: missing side-exit reason {expected_reason}",
        )
        expected_counter = scenario.get("expected_counter")
        if expected_counter:
            require(
                cranelift.jit_stats.get(expected_counter, 0) > 0,
                failures,
                f"{name}: missing {expected_counter}",
            )
        elif expected_reason == "overflow":
            require(cranelift.jit_stats.get("overflow_exits", 0) > 0, failures, f"{name}: missing overflow exits")
        require(cranelift.jit_stats.get("slow_path_calls", 0) > 0, failures, f"{name}: missing slow-path calls")
        if scenario["target"] == "known_call":
            require(
                cranelift.jit_stats.get("known_call_slow_calls", 0) > 0,
                failures,
                f"{name}: missing known-call slow calls",
            )
        if scenario["target"] == "property_load":
            require(
                cranelift.jit_stats.get("property_load_guard_exits", 0) > 0,
                failures,
                f"{name}: missing property-load guard exits",
            )
            require(
                cranelift.jit_stats.get("property_load_slow_calls", 0) > 0,
                failures,
                f"{name}: missing property-load slow calls",
            )
    return "pass" if off.returncode == cranelift.returncode and off.stdout == cranelift.stdout else "fail"


def validate_matrix_coverage(rows: list[dict[str, Any]], failures: list[str]) -> None:
    for family in sorted(REQUIRED_BIG_WIN_FAMILIES):
        family_rows = [row for row in rows if row.get("matrix_family") == family]
        require(family_rows, failures, f"matrix: missing {REQUIRED_BIG_WIN_FAMILIES[family]} family")
        modes = {row.get("jit_mode") for row in family_rows}
        require("off" in modes, failures, f"matrix: missing interpreter baseline for {family}")
        require("cranelift" in modes, failures, f"matrix: missing Cranelift row for {family}")
        cranelift_paths = {
            path
            for row in family_rows
            if row.get("jit_mode") == "cranelift"
            for path in row.get("path_kinds", [])
        }
        require(
            bool(cranelift_paths - {"jit_fallback_or_skip"}),
            failures,
            f"matrix: Cranelift rows for {family} only fallback/skip",
        )

    all_paths = {path for row in rows for path in row.get("path_kinds", [])}
    require("interpreter_baseline" in all_paths, failures, "matrix: missing interpreter baseline path")
    require("jit_helper_call_path" in all_paths, failures, "matrix: missing JIT helper-call path")
    require("jit_fast_path" in all_paths, failures, "matrix: missing JIT fast-path")


def build_matrix_summary(rows: list[dict[str, Any]]) -> dict[str, Any]:
    families: dict[str, Any] = {}
    for row in rows:
        family = row["matrix_family"]
        entry = families.setdefault(
            family,
            {
                "label": REQUIRED_BIG_WIN_FAMILIES.get(family, family.replace("_", " ")),
                "scenarios": [],
                "jit_modes": [],
                "path_kinds": [],
                "statuses": [],
            },
        )
        scenario = row["scenario"]
        if scenario not in entry["scenarios"]:
            entry["scenarios"].append(scenario)
        mode = row["jit_mode"]
        if mode not in entry["jit_modes"]:
            entry["jit_modes"].append(mode)
        for path in row["path_kinds"]:
            if path not in entry["path_kinds"]:
                entry["path_kinds"].append(path)
        status = row["jit_status"]
        if status not in entry["statuses"]:
            entry["statuses"].append(status)
    return {
        "required_families": REQUIRED_BIG_WIN_FAMILIES,
        "families": families,
        "reference_php_policy": "advisory only; not a gate",
        "speedup_policy": "informational timings only; no hard speedup gate",
    }


def row_for(
    *,
    scenario: dict[str, Any],
    mode: str,
    correctness: str,
    run: Run,
    average_wall_time: float,
    repeats: int,
) -> dict[str, Any]:
    stats = run.jit_stats or {}
    known_gaps = list(scenario["known_gaps"]) if mode == "cranelift" else []
    compile_time_nanos = int(stats.get("compile_time_nanos", 0))
    compile_time_seconds = compile_time_nanos / 1_000_000_000
    total_time_seconds = average_wall_time
    execution_time_seconds = max(total_time_seconds - compile_time_seconds, 0.0)
    return {
        "scenario": scenario["scenario"],
        "matrix_family": matrix_family_for(scenario),
        "matrix_family_label": REQUIRED_BIG_WIN_FAMILIES.get(matrix_family_for(scenario), matrix_family_for(scenario)),
        "target": scenario["target"],
        "fixture": rel(scenario["fixture"]),
        "jit_mode": mode,
        "path_kinds": path_kinds_for(mode, run.jit_stats),
        "tiering_mode": tiering_mode_for(scenario),
        "correctness": correctness,
        "jit_status": jit_status_for(mode, run.jit_stats),
        "command": run.command,
        "iterations": repeats,
        "wall_time_seconds": average_wall_time,
        "total_time_seconds": total_time_seconds,
        "compile_time_seconds": compile_time_seconds,
        "execution_time_seconds": execution_time_seconds,
        "compile_time_nanos": compile_time_nanos,
        "side_exits": int(stats.get("side_exits", 0)),
        "side_exit_reasons": {
            str(key): int(value)
            for key, value in (stats.get("side_exit_reasons") or {}).items()
            if isinstance(value, int)
        },
        "blacklist_reasons": {
            str(key): int(value)
            for key, value in (stats.get("blacklist_reasons") or {}).items()
            if isinstance(value, int)
        },
        "counters": counters_from_stats(run.jit_stats),
        "vm_counters": {
            "jit_compile_attempts": int(stats.get("compile_attempts", 0)),
            "jit_executed": int(stats.get("executed", 0)),
            "jit_side_exits": int(stats.get("side_exits", 0)),
            "jit_blacklisted_regions": int(stats.get("blacklisted_regions", 0)),
            "jit_tiering_cold_functions": int(stats.get("tiering_cold_functions", 0)),
            "jit_tiering_hot_functions": int(stats.get("tiering_hot_functions", 0)),
            "jit_tiering_eager_functions": int(stats.get("tiering_eager_functions", 0)),
            "jit_tiering_blacklist_rejections": int(stats.get("tiering_blacklist_rejections", 0)),
            "jit_tiering_budget_rejections": int(stats.get("tiering_budget_rejections", 0)),
            "jit_helper_calls": int(stats.get("helper_calls", 0)),
            "jit_fast_path_hits": int(stats.get("fast_path_hits", 0)),
            "packed_fetch_fast_hits": int(stats.get("packed_fetch_fast_hits", 0)),
            "packed_fetch_bounds_exits": int(stats.get("packed_fetch_bounds_exits", 0)),
            "packed_fetch_layout_exits": int(stats.get("packed_fetch_layout_exits", 0)),
            "packed_foreach_sum_fast_hits": int(stats.get("packed_foreach_sum_fast_hits", 0)),
            "packed_foreach_sum_layout_exits": int(stats.get("packed_foreach_sum_layout_exits", 0)),
            "packed_foreach_sum_overflow_exits": int(stats.get("packed_foreach_sum_overflow_exits", 0)),
            "known_call_fast_hits": int(stats.get("known_call_fast_hits", 0)),
            "known_call_guard_exits": int(stats.get("known_call_guard_exits", 0)),
            "known_call_slow_calls": int(stats.get("known_call_slow_calls", 0)),
            "direct_call_hits": int(stats.get("direct_call_hits", 0)),
            "direct_call_fallbacks": int(stats.get("direct_call_fallbacks", 0)),
            "property_load_fast_hits": int(stats.get("property_load_fast_hits", 0)),
            "property_load_guard_exits": int(stats.get("property_load_guard_exits", 0)),
            "property_load_layout_exits": int(stats.get("property_load_layout_exits", 0)),
            "property_load_uninitialized_exits": int(stats.get("property_load_uninitialized_exits", 0)),
            "property_load_slow_calls": int(stats.get("property_load_slow_calls", 0)),
            "string_concat_fast_path_hits": int(stats.get("string_concat_fast_path_hits", 0)),
            "string_concat_fast_path_misses": int(stats.get("string_concat_fast_path_misses", 0)),
            "jit_overflow_exits": int(stats.get("overflow_exits", 0)),
            "jit_slow_path_calls": int(stats.get("slow_path_calls", 0)),
        },
        "known_gaps": known_gaps,
    }


def reference_row_for(
    *,
    scenario: dict[str, Any],
    baseline: Run,
    reference: Run,
) -> dict[str, Any]:
    total_time_seconds = reference.wall_time_seconds
    matches_baseline = (
        baseline.returncode == reference.returncode
        and baseline.stdout == reference.stdout
        and stderr_without_jit_stats(baseline.stderr) == reference.stderr
    )
    return {
        "scenario": scenario["scenario"],
        "matrix_family": matrix_family_for(scenario),
        "matrix_family_label": REQUIRED_BIG_WIN_FAMILIES.get(matrix_family_for(scenario), matrix_family_for(scenario)),
        "target": scenario["target"],
        "fixture": rel(scenario["fixture"]),
        "jit_mode": "reference_php",
        "path_kinds": path_kinds_for("reference_php", None),
        "tiering_mode": "not_applicable",
        "correctness": "orientation_match" if matches_baseline else "orientation_mismatch",
        "jit_status": "reference",
        "command": reference.command,
        "iterations": 1,
        "wall_time_seconds": total_time_seconds,
        "total_time_seconds": total_time_seconds,
        "compile_time_seconds": 0.0,
        "execution_time_seconds": total_time_seconds,
        "compile_time_nanos": 0,
        "side_exits": 0,
        "side_exit_reasons": {},
        "blacklist_reasons": {},
        "counters": counters_from_stats(None),
        "vm_counters": {},
        "reference_matches_interpreter_baseline": matches_baseline,
        "reference_exit_code": reference.returncode,
        "baseline_exit_code": baseline.returncode,
        "known_gaps": [],
    }


def main() -> int:
    args = parse_args()
    repeats = 1 if args.smoke else args.repeats
    warmups = 0 if args.smoke else args.warmups
    if repeats < 1:
        raise SystemExit("--repeats must be >= 1")
    if warmups < 0:
        raise SystemExit("--warmups must be >= 0")
    if not args.engine.is_file() or not os.access(args.engine, os.X_OK):
        raise SystemExit(f"Rust VM is not executable: {args.engine}")
    reference_status: dict[str, Any] = {
        "status": "skipped",
        "reason": "REFERENCE_PHP not set",
        "binary": None,
    }
    reference_php = args.reference_php
    if reference_php is not None:
        reference_status = {
            "status": "skipped",
            "reason": "REFERENCE_PHP is not an executable file",
            "binary": rel(reference_php),
        }
        if reference_php.is_file() and os.access(reference_php, os.X_OK):
            reference_status = {
                "status": "ran",
                "reason": None,
                "binary": rel(reference_php),
            }
        else:
            reference_php = None

    rows: list[dict[str, Any]] = []
    failures: list[str] = []
    for scenario in SCENARIOS:
        fixture = scenario["fixture"]
        name = scenario["scenario"]
        if not fixture.is_file():
            failures.append(f"{name}: missing fixture {rel(fixture)}")
            continue
        for warmup_index in range(warmups):
            run_engine(
                args.engine,
                fixture,
                "off",
                ROOT / "target/performance/cranelift/bench-matrix-tmp" / name / "warmup-off" / str(warmup_index),
                args.timeout,
                scenario,
            )
            run_engine(
                args.engine,
                fixture,
                "cranelift",
                ROOT / "target/performance/cranelift/bench-matrix-tmp" / name / "warmup-cranelift" / str(warmup_index),
                args.timeout,
                scenario,
            )
        off_runs = [
            run_engine(
                args.engine,
                fixture,
                "off",
                ROOT / "target/performance/cranelift/bench-matrix-tmp" / name / "off" / str(index),
                args.timeout,
                scenario,
            )
            for index in range(repeats)
        ]
        cranelift_runs = [
            run_engine(
                args.engine,
                fixture,
                "cranelift",
                ROOT / "target/performance/cranelift/bench-matrix-tmp" / name / "cranelift" / str(index),
                args.timeout,
                scenario,
            )
            for index in range(repeats)
        ]
        off, off_average = summarize_runs(off_runs)
        cranelift, cranelift_average = summarize_runs(cranelift_runs)
        correctness = validate_scenario(
            scenario=scenario,
            off=off,
            cranelift=cranelift,
            failures=failures,
        )
        rows.append(
            row_for(
                scenario=scenario,
                mode="off",
                correctness=correctness,
                run=off,
                average_wall_time=off_average,
                repeats=repeats,
            )
        )
        rows.append(
            row_for(
                scenario=scenario,
                mode="cranelift",
                correctness=correctness,
                run=cranelift,
                average_wall_time=cranelift_average,
                repeats=repeats,
            )
        )
        if reference_php is not None and not args.smoke:
            reference = run_reference(
                reference_php,
                fixture,
                ROOT / "target/performance/cranelift/bench-matrix-tmp" / name / "reference_php",
                args.timeout,
            )
            rows.append(
                reference_row_for(
                    scenario=scenario,
                    baseline=off,
                    reference=reference,
                )
            )

    validate_matrix_coverage(rows, failures)

    report = {
        "schema_version": 2,
        "run_id": "performance-cranelift-bench-smoke" if args.smoke else "performance-cranelift-local",
        "gate": "jit-cranelift-bench-smoke" if args.smoke else "jit-cranelift-report",
        "status": "fail" if failures else "pass",
        "environment": report_environment(repeats, warmups),
        "matrix": build_matrix_summary(rows),
        "reference_php": reference_status,
        "rows": rows,
        "failures": failures,
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")

    if failures:
        for failure in failures:
            print(f"[fail] {failure}", file=sys.stderr)
        print(f"[fail] Cranelift bench matrix wrote {rel(args.out)}", file=sys.stderr)
        return 1

    print(f"[pass] Cranelift bench matrix wrote {rel(args.out)} with {len(rows)} row(s)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
