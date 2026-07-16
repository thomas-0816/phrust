#!/usr/bin/env python3
"""Verify generated generator/fiber suspension and resume state machines."""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

from rust_module import read_rust_module


ROOT = Path(__file__).resolve().parents[2]
ABI = ROOT / "crates/php_jit/src/abi.rs"
REGION = ROOT / "crates/php_jit/src/region_ir/executable.rs"
LOWERING = ROOT / "crates/php_jit/src/cranelift_lowering.rs"
METADATA = ROOT / "crates/php_jit/src/lib.rs"
REPORT = ROOT / "target/cranelift-only/native-suspensions.json"
KINDS = {"generator_yield": 1, "generator_delegate": 2, "fiber_suspend": 3}
INPUTS = {"start": 1, "value": 2, "throw": 3}
PERSISTED = {
    "native_identity",
    "continuation",
    "locals",
    "temporaries",
    "yielded_key",
    "yielded_value",
    "delegation",
    "exception",
    "roots",
}


def main() -> int:
    failures: list[str] = []
    abi = ABI.read_text(encoding="utf-8")
    region = REGION.read_text(encoding="utf-8")
    lowering = read_rust_module(LOWERING)
    metadata = METADATA.read_text(encoding="utf-8")

    for required in (
        "JitNativeGeneratorState",
        "JitNativeFiberState",
        "JitNativeSuspendKind",
        "JitNativeResumeInputKind",
        "JitNativeSuspensionGenerationPolicy",
        "transition_generation_at_suspension",
    ):
        if required not in abi:
            failures.append(f"native suspension ABI lacks {required}")
    for field in (
        "owning_generation",
        "continuation_id",
        "resume_id",
        "local_slots",
        "temporary_slots",
        "yielded_key",
        "yielded_value",
        "delegation_state",
        "exception_state",
        "root_entries",
    ):
        if not re.search(rf"pub {field}:", abi):
            failures.append(f"generator/fiber state lacks {field}")

    for operation, variant in (
        ("Yield", "GeneratorYield"),
        ("YieldFrom", "GeneratorDelegate"),
    ):
        pattern = rf"InstructionKind::{operation}\b[\s\S]{{0,500}}RegionNativeSuspend::{variant}"
        if not re.search(pattern, region):
            failures.append(f"{operation} does not enter native suspension IR")
    if not re.search(
        r'class_name\.eq_ignore_ascii_case\("fiber"\)[\s\S]{0,800}RegionNativeSuspend::FiberSuspend',
        region,
    ):
        failures.append("Fiber::suspend does not enter native suspension IR")

    for required in (
        "lower_native_suspension",
        "SUSPEND_GENERATOR",
        "SUSPEND_FIBER",
        "initialized_register_mask",
        "delegation_handle",
    ):
        if required not in lowering:
            failures.append(f"Cranelift suspension lowering lacks {required}")
    for required in (
        "JitNativeSuspensionMetadata",
        "native_suspension_resume_id",
        "invoke_i64_suspension_resume",
        "suspensions",
        "owning_generation_required",
    ):
        if required not in metadata:
            failures.append(f"native resume metadata lacks {required}")

    combined = region + lowering + metadata
    for forbidden in ("GeneratorContinuation", "FiberContinuation"):
        if forbidden in combined:
            failures.append(f"native compiler depends on interpreter type {forbidden}")
    interpreter_pattern = re.compile(
        "execute_" + "dense|rich_" + "dispatch|execute_" + "ir|resume_" + "continuation"
    )
    if interpreter_pattern.search(lowering + metadata):
        failures.append("native suspension path references retired resume dispatch")
    resume_method = re.search(
        r"pub fn invoke_i64_suspension_resume\([\s\S]+?\n    }\n", metadata
    )
    if resume_method is None:
        failures.append("native suspension resume method is missing")
    elif "loop {" in resume_method.group(0):
        failures.append("suspension helper hides a generic Rust instruction loop")

    for required in (
        "generator_yield_send_and_throw_use_native_resume_entry",
        "yield_from_publishes_native_delegation_state",
        "fiber_suspend_and_resume_use_native_continuation",
        "generator_resume_runs_compiled_finally",
    ):
        if required not in lowering:
            failures.append(f"native suspension test is missing: {required}")

    if not REPORT.is_file():
        failures.append("native suspension report was not generated")
    else:
        report = json.loads(REPORT.read_text(encoding="utf-8"))
        kinds = {entry["name"]: entry["tag"] for entry in report.get("suspension_kinds", [])}
        inputs = {entry["name"]: entry["tag"] for entry in report.get("resume_inputs", [])}
        if kinds != KINDS:
            failures.append("generated report has incomplete suspension kinds")
        if inputs != INPUTS:
            failures.append("generated report has incomplete resume inputs")
        if set(report.get("persisted_state", [])) != PERSISTED:
            failures.append("generated report has incomplete suspended heap state")
        for flag, expected in (
            ("generated_resume_entries", True),
            ("interpreter_continuation_dependency", False),
            ("interpreter_resume_dispatch", False),
            ("generic_rust_instruction_loop", False),
        ):
            if report.get(flag) is not expected:
                failures.append(f"generated report violates {flag}={expected}")

    if failures:
        print("Cranelift native-suspension gate failed:", file=sys.stderr)
        print("\n".join(f"- {failure}" for failure in failures), file=sys.stderr)
        return 1
    print(
        "Cranelift native-suspension gate passed "
        f"({len(KINDS)} kinds, {len(INPUTS)} resume inputs)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
