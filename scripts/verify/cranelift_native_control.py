#!/usr/bin/env python3
"""Verify explicit native PHP control flow, frame roots, and PC metadata."""

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
VM_ABI = ROOT / "crates/php_vm/src/vm/jit_abi.rs"
REPORT = ROOT / "target/cranelift-only/native-control-flow.json"
STATUSES = {
    "Continue": 0,
    "Return": 1,
    "ReturnReference": 2,
    "Throw": 3,
    "Exit": 4,
    "SuspendGenerator": 5,
    "SuspendFiber": 6,
    "RuntimeError": 7,
    "CompileRequired": 8,
    "RecompileRequested": 9,
}
CONTROL_OPS = {
    "EnterTry",
    "LeaveTry",
    "EndFinally",
    "Throw",
    "MakeException",
    "ReturnThroughFinally",
    "ThrowThroughFinally",
    "ExitThroughFinally",
}
DESTRUCTOR_POINTS = {
    "local_overwrite",
    "discard",
    "frame_return",
    "exception_unwind",
    "request_shutdown",
}


def main() -> int:
    failures: list[str] = []
    abi = ABI.read_text(encoding="utf-8")
    region = REGION.read_text(encoding="utf-8")
    lowering = read_rust_module(LOWERING)
    metadata = METADATA.read_text(encoding="utf-8")
    vm_abi = read_rust_module(VM_ABI)

    for name, tag in (
        ("CONTINUE", 0),
        ("RETURN", 1),
        ("RETURN_REFERENCE", 2),
        ("THROW", 3),
        ("EXIT", 4),
        ("SUSPEND_GENERATOR", 5),
        ("SUSPEND_FIBER", 6),
        ("RUNTIME_ERROR", 7),
        ("COMPILE_REQUIRED", 8),
        ("RECOMPILE_REQUESTED", 9),
    ):
        if not re.search(rf"pub const {name}: Self = Self\({tag}\);", abi):
            failures.append(f"native control status {name} does not have stable tag {tag}")

    for required in (
        "JitNativeControlRecord",
        "JitNativeExceptionHandler",
        "JitNativeFrameHeader",
        "JitNativeRootEntry",
        "JitNativeRootKind",
        "JitNativePcMetadata",
        "JitNativeDestructorPoint",
    ):
        if required not in abi:
            failures.append(f"native ABI lacks {required}")

    for operation in ("EnterTry", "LeaveTry", "EndFinally", "Throw", "MakeException"):
        pattern = rf"InstructionKind::{operation}\b[\s\S]{{0,1600}}RegionInstructionKind::NativeControl"
        if not re.search(pattern, region):
            failures.append(f"{operation} does not enter RegionNativeControl")

    for required in (
        "select_native_unwind",
        "invoke_i64_with_native_unwind",
        "resolve_native_pc",
        "exception_handlers",
        "safepoints",
        "baseline_frame_slots",
        "optimized_roots_required",
    ):
        if required not in metadata:
            failures.append(f"native frame metadata lacks {required}")

    for required in (
        "return_runs_compiled_finally_before_native_frame_return",
        "exit_runs_compiled_finally_before_native_exit_status",
        "native_unwind_resumes_compiled_catch_without_interpreter_frame",
        "throw_uses_explicit_native_status_and_publishes_unwind_metadata",
    ):
        if required not in lowering:
            failures.append(f"native control test is missing: {required}")

    if "catch_unwind" not in vm_abi:
        failures.append("native runtime ABI does not contain Rust panics at the generated boundary")
    forbidden = re.compile(
        r"execute_rich_exception_instruction|execute_dense|execute_ir|resume_call|tailcall",
        re.IGNORECASE,
    )
    if forbidden.search(lowering + metadata):
        failures.append("native control lowering references an interpreter dispatch loop")

    if not REPORT.is_file():
        failures.append("native control report was not generated")
    else:
        report = json.loads(REPORT.read_text(encoding="utf-8"))
        statuses = {entry["name"]: entry["tag"] for entry in report.get("statuses", [])}
        if statuses != STATUSES:
            failures.append("generated report has incomplete or unstable status tags")
        if set(report.get("control_operations", [])) != CONTROL_OPS:
            failures.append("generated report lacks native control operations")
        points = {entry["name"] for entry in report.get("destructor_points", [])}
        if points != DESTRUCTOR_POINTS:
            failures.append("generated report lacks destructor release points")
        if report.get("native_unwind") is not True:
            failures.append("generated report does not require native unwind")
        if report.get("rust_unwind_across_generated") is not False:
            failures.append("generated report permits Rust unwind across generated code")
        if report.get("interpreter_exception_dispatch") is not False:
            failures.append("generated report permits interpreter exception dispatch")

    if failures:
        print("Cranelift native-control gate failed:", file=sys.stderr)
        print("\n".join(f"- {failure}" for failure in failures), file=sys.stderr)
        return 1
    print(
        "Cranelift native-control gate passed "
        f"({len(STATUSES)} statuses, {len(CONTROL_OPS)} operations)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
