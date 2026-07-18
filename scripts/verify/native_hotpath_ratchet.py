#!/usr/bin/env python3
"""Generated-code and optional WordPress counter ratchets for hot native execution."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--profile",
        type=Path,
        help="optional diagnostic request profile or canonical counters JSON",
    )
    parser.add_argument(
        "--baseline-profile",
        type=Path,
        help="optional baseline counters JSON used for the 25%% code-growth ratchet",
    )
    return parser.parse_args()


def load_counters(path: Path) -> dict[str, Any]:
    document = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(document, dict):
        raise ValueError(f"{path}: top-level JSON value must be an object")
    native = document.get("native", document)
    if not isinstance(native, dict):
        raise ValueError(f"{path}: native counters must be an object")
    return native


def number(counters: dict[str, Any], *names: str) -> int:
    for name in names:
        value = counters.get(name)
        if isinstance(value, (int, float)) and not isinstance(value, bool):
            return int(value)
    return 0


def mapping(counters: dict[str, Any], *names: str) -> dict[str, int]:
    for name in names:
        value = counters.get(name)
        if isinstance(value, dict):
            return {
                str(key): int(count)
                for key, count in value.items()
                if isinstance(count, (int, float)) and not isinstance(count, bool)
            }
    return {}


def require(failures: list[str], condition: bool, message: str) -> None:
    if not condition:
        failures.append(message)


def run_generated_code_fixtures(failures: list[str]) -> None:
    commands = (
        ["cargo", "test", "-q", "-p", "php_jit", "--lib", "optimizing_"],
        [
            "cargo",
            "test",
            "-q",
            "-p",
            "php_jit",
            "--lib",
            "oversized_php_cfg_compiles_as_bounded_direct_native_fragments",
        ],
        [
            "cargo",
            "test",
            "-q",
            "-p",
            "php_jit",
            "--lib",
            "optimized_exit_after_effect_does_not_repeat_effect_in_baseline",
        ],
        [
            "cargo",
            "test",
            "-q",
            "-p",
            "php_vm",
            "--lib",
            "same_unit_call_resolves_on_demand_then_calls_native",
        ],
        [
            "cargo",
            "test",
            "-q",
            "-p",
            "php_vm",
            "--lib",
            "warmed_method_pic_reclassifies_stable_call_as_direct",
        ],
        [
            "cargo",
            "test",
            "-q",
            "-p",
            "php_vm",
            "--lib",
            "server_worker_publishes_optimized_entry_after_hot_baseline_threshold",
        ],
        [
            "cargo",
            "test",
            "-q",
            "-p",
            "php_vm",
            "--lib",
            "foreground_compile_overtakes_queued_background_work",
        ],
    )
    for command in commands:
        completed = subprocess.run(command, cwd=ROOT, check=False)
        if completed.returncode != 0:
            failures.append(f"generated hot-path fixture failed: {' '.join(command)}")


def ratchet_profile(
    failures: list[str], counters: dict[str, Any], baseline: dict[str, Any] | None
) -> None:
    helper_calls = number(counters, "runtime_helper_calls")
    reads = sum(mapping(counters, "runtime_helper_local_read_by_reason").values())
    stores = sum(mapping(counters, "runtime_helper_local_store_by_reason").values())
    truthy = sum(mapping(counters, "runtime_helper_truthy_by_value_class").values())
    retain_release = sum(mapping(counters, "runtime_helper_retain_by_reason").values()) + sum(
        mapping(counters, "runtime_helper_release_by_reason").values()
    )
    root_scans = number(counters, "runtime_helper_object_release_root_scans")
    dynamic_calls = number(counters, "call_dynamic", "native_call_dynamic")
    eligible = sum(
        number(counters, name)
        for name in (
            "same_unit_direct_eligible",
            "cross_unit_direct_eligible",
            "method_monomorphic_eligible",
            "builtin_direct_eligible",
        )
    )
    executed = sum(
        number(counters, name)
        for name in (
            "same_unit_direct_executed",
            "cross_unit_direct_executed",
            "method_monomorphic_executed",
            "builtin_direct_executed",
        )
    )
    compile_attempts = number(counters, "compile_attempts", "native_compile_attempts")
    helper_nanos = number(counters, "runtime_helper_time_nanos")
    execution_nanos = number(counters, "execution_time_nanos", "native_execution_time_nanos")

    require(failures, helper_calls <= 750_000, f"runtime helper calls {helper_calls} exceed 750000")
    require(failures, reads <= 50_000, f"local read helpers {reads} exceed 50000")
    require(failures, stores <= 25_000, f"local store helpers {stores} exceed 25000")
    require(failures, truthy <= 25_000, f"truthiness helpers {truthy} exceed 25000")
    require(
        failures,
        retain_release <= 150_000,
        f"retain/release helpers {retain_release} exceed 150000",
    )
    require(failures, root_scans <= 250, f"root scans {root_scans} exceed 250")
    require(failures, dynamic_calls <= 50_000, f"dynamic calls {dynamic_calls} exceed 50000")
    require(
        failures,
        eligible > 0 and executed * 100 >= eligible * 90,
        f"stable direct-call ratio {executed}/{eligible} is below 90%",
    )
    require(failures, compile_attempts == 0, f"warm profile compiled {compile_attempts} functions")
    require(
        failures,
        execution_nanos > 0 and helper_nanos * 100 <= execution_nanos * 30,
        "helper-exclusive time exceeds 30% of native execution",
    )

    if baseline is not None:
        current_bytes = number(counters, "mapped_executable_bytes", "native_mapped_executable_bytes")
        baseline_bytes = number(baseline, "mapped_executable_bytes", "native_mapped_executable_bytes")
        require(failures, baseline_bytes > 0, "baseline mapped executable bytes are missing")
        require(
            failures,
            baseline_bytes > 0 and current_bytes * 100 <= baseline_bytes * 125,
            f"native code grew from {baseline_bytes} to {current_bytes} bytes (>25%)",
        )


def main() -> int:
    args = parse_args()
    failures: list[str] = []
    run_generated_code_fixtures(failures)

    if args.profile is not None:
        try:
            counters = load_counters(args.profile)
            baseline = (
                load_counters(args.baseline_profile)
                if args.baseline_profile is not None
                else None
            )
            ratchet_profile(failures, counters, baseline)
        except (OSError, ValueError, json.JSONDecodeError) as error:
            failures.append(str(error))

    if failures:
        for failure in failures:
            print(f"[fail] {failure}", file=sys.stderr)
        return 1
    suffix = " with runtime counters" if args.profile is not None else ""
    print(f"[pass] native hot-path generated-code ratchet{suffix}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
