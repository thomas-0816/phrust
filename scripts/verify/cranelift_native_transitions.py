#!/usr/bin/env python3
"""Verify optimized exits target exact baseline-native continuations."""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
ABI = ROOT / "crates/php_jit/src/abi.rs"
API = ROOT / "crates/php_jit/src/lib.rs"
LOWERING = ROOT / "crates/php_jit/src/cranelift_lowering.rs"
EXECUTABLE = ROOT / "crates/php_jit/src/cranelift_lowering/executable_region.rs"
REPORT = ROOT / "target/cranelift-only/native-version-transitions.json"


def main() -> int:
    failures: list[str] = []
    abi = ABI.read_text(encoding="utf-8")
    api = API.read_text(encoding="utf-8")
    lowering = LOWERING.read_text(encoding="utf-8")
    executable = EXECUTABLE.read_text(encoding="utf-8")

    for required in (
        "JitNativeTransitionState",
        "native_version",
        "initialized_register_mask",
        "control_status",
    ):
        if required not in abi:
            failures.append(f"native transition state lacks {required}")
    for required in (
        "JitNativeTransitionMetadata",
        "JitNativeFunctionEntryMetadata",
        "invoke_i64_native_transition",
        "invoke_i64_with_native_transition",
        "JIT_NATIVE_TRANSITION_RESUME_TAG",
        "MissingNativeTransition",
    ):
        if required not in api:
            failures.append(f"native transition API lacks {required}")
    for required in (
        "instruction_blocks",
        "native_transition_resume_id",
        "region_instruction_result_register",
        "live_registers",
        "function_entries",
    ):
        if required not in executable:
            failures.append(f"baseline continuation lowering lacks {required}")
    for required in (
        "initialized_register_mask",
        "JitCallStatus::RECOMPILE_REQUESTED",
        "baseline_native_continuation_resumes_exact_instruction",
        "optimized_exit_after_effect_does_not_repeat_effect_in_baseline",
        "nested_callee_transition_uses_published_native_function_entry",
    ):
        if required not in lowering:
            failures.append(f"native guard-exit path lacks {required}")

    method = re.search(
        r"pub fn invoke_i64_with_native_transition\([\s\S]+?\n    }\n", api
    )
    forbidden = re.compile(
        "execute_" + "dense|rich_" + "dispatch|execute_" + "ir|execute_instruction"
    )
    if method is None:
        failures.append("native transition method is missing")
    elif forbidden.search(method.group(0)):
        failures.append("native transition method references an instruction executor")

    if not REPORT.is_file():
        failures.append("native version-transition report was not generated")
    else:
        report = json.loads(REPORT.read_text(encoding="utf-8"))
        for flag, expected in (
            ("baseline_for_every_function", True),
            ("exact_instruction_entries", True),
            ("live_locals_and_registers", True),
            ("pending_control_state", True),
            ("nested_function_entries", True),
            ("native_osr_both_directions", True),
            ("observable_effect_replay", False),
            ("interpreter_resume_target", False),
        ):
            if report.get(flag) is not expected:
                failures.append(f"generated report violates {flag}={expected}")
        if report.get("transition_resume_tag") != "20000000":
            failures.append("transition resume namespace is not stable")

    if failures:
        print("Cranelift native-transition gate failed:", file=sys.stderr)
        print("\n".join(f"- {failure}" for failure in failures), file=sys.stderr)
        return 1
    print("Cranelift native-transition gate passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
