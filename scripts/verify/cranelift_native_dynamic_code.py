#!/usr/bin/env python3
"""Verify native-only dynamic source compilation and publication."""

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
COORDINATOR = ROOT / "crates/php_jit/src/dynamic_code.rs"
VM_ABI = ROOT / "crates/php_vm/src/vm/jit_abi.rs"
REPORT = ROOT / "target/cranelift-only/native-dynamic-code.json"
OPERATIONS = {
    "include": 1,
    "include_once": 2,
    "require": 3,
    "require_once": 4,
    "eval": 5,
    "declare_function": 6,
    "declare_class": 7,
    "make_closure": 8,
}


def main() -> int:
    failures: list[str] = []
    abi = ABI.read_text(encoding="utf-8")
    region = REGION.read_text(encoding="utf-8")
    lowering = read_rust_module(LOWERING)
    coordinator = COORDINATOR.read_text(encoding="utf-8")
    vm_abi = read_rust_module(VM_ABI)

    for required in (
        "JitNativeDynamicCodeKind",
        "JitNativeDynamicCodeRequest",
        "JitNativeDynamicCodeTrampoline",
    ):
        if required not in abi:
            failures.append(f"dynamic-code ABI lacks {required}")
    for operation in ("Include", "Eval", "DeclareFunction", "DeclareClass", "MakeClosure"):
        pattern = rf"InstructionKind::{operation}\b[\s\S]{{0,800}}RegionNativeDynamicCode"
        if not re.search(pattern, region):
            failures.append(f"{operation} does not enter native dynamic-code IR")
    for required in (
        "lower_native_dynamic_code",
        "NATIVE_DYNAMIC_CODE_SYMBOL",
        "JitNativeDynamicCodeRequest",
        "JitCallStatus::RETURN",
    ):
        if required not in lowering:
            failures.append(f"Cranelift dynamic-code lowering lacks {required}")
    for required in (
        "DynamicCodeCacheKey",
        "get_or_compile",
        "install_restart_artifact",
        "reinitialize_after_fork",
        "Condvar",
        "RecursiveSameKey",
    ):
        if required not in coordinator:
            failures.append(f"compile-once coordinator lacks {required}")
    if "jit_native_dynamic_code_abi" not in vm_abi:
        failures.append("VM does not publish native dynamic-code ABI")
    forbidden = re.compile(
        "execute_" + "dense|rich_" + "dispatch|execute_" + "ir|execute_" + "instruction"
    )
    if forbidden.search(lowering + coordinator):
        failures.append("native dynamic-code compiler references an instruction executor")

    for test in (
        "concurrent_dynamic_compile_waits_and_publishes_once",
        "nested_dynamic_compile_uses_independent_key_without_deadlock",
        "exact_source_key_controls_process_and_restart_cache_reuse",
        "dynamic_compile_errors_are_explicit_and_cached",
        "after_fork_reinitialization_discards_inherited_synchronization",
        "include_executes_only_after_native_dynamic_compiler_returns_entry_result",
    ):
        if test not in coordinator + lowering:
            failures.append(f"native dynamic-code test is missing: {test}")

    if not REPORT.is_file():
        failures.append("native dynamic-code report was not generated")
    else:
        report = json.loads(REPORT.read_text(encoding="utf-8"))
        operations = {entry["name"]: entry["tag"] for entry in report.get("operations", [])}
        if operations != OPERATIONS:
            failures.append("generated report has incomplete dynamic operations")
        for flag, expected in (
            ("compile_once_exact_key", True),
            ("concurrent_miss_waits", True),
            ("nested_compile_lock_free", True),
            ("after_fork_reinitialization", True),
            ("process_cache", True),
            ("restart_cache_participation", True),
            ("publish_before_execute", True),
            ("interpreter_first_execution", False),
        ):
            if report.get(flag) is not expected:
                failures.append(f"generated report violates {flag}={expected}")

    if failures:
        print("Cranelift native dynamic-code gate failed:", file=sys.stderr)
        print("\n".join(f"- {failure}" for failure in failures), file=sys.stderr)
        return 1
    print(f"Cranelift native dynamic-code gate passed ({len(OPERATIONS)} operations)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
