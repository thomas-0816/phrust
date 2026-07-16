#!/usr/bin/env python3
"""Verify the single native call model and reject executor-loop call fallbacks."""

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
TRAMPOLINE = ROOT / "crates/php_vm/src/vm/jit_abi.rs"
REPORT = ROOT / "target/cranelift-only/native-call-model.json"
CALL_FORMS = {
    "CallFunction",
    "CallMethod",
    "CallStaticMethod",
    "CallClosure",
    "CallCallable",
    "Pipe",
    "BindReferenceFromCall",
    "BindReferenceFromMethodCall",
    "NewObject",
    "DynamicNewObject",
}
CALLBACKS = {
    "builtin_runtime_callback",
    "magic_method",
    "property_hook",
    "autoload_callback",
    "error_handler",
    "shutdown_function",
    "destructor",
}


def main() -> int:
    failures: list[str] = []
    abi = ABI.read_text(encoding="utf-8")
    region = REGION.read_text(encoding="utf-8")
    lowering = read_rust_module(LOWERING)
    trampoline = read_rust_module(TRAMPOLINE)

    for required in (
        "JitNativeCallFrame",
        "JitNativeCallArgument",
        "JitNativeCallTarget",
        "JitNativeIndirectionEntry",
        "JitNativeDispatchTrampoline",
    ):
        if required not in abi:
            failures.append(f"native ABI lacks {required}")
    for field in (
        "function_id",
        "region_id",
        "continuation_id",
        "result_slot",
        "local_slots",
        "temporary_slots",
        "receiver_handle",
        "class_context",
        "exception_metadata",
        "trace_metadata",
        "generator_handle",
        "fiber_handle",
    ):
        if not re.search(rf"pub {field}:", abi):
            failures.append(f"native frame lacks {field}")
    for call in CALL_FORMS:
        # Call match arms also build argument metadata and direct-call guards.
        # Keep the bound finite so this remains an arm-local source contract.
        pattern = rf"InstructionKind::{call}\b[\s\S]{{0,6000}}RegionInstructionKind::NativeCall"
        if not re.search(pattern, region):
            failures.append(f"{call} does not enter RegionNativeCall")
    if "lower_native_call_trampoline" not in lowering:
        failures.append("Cranelift lowering lacks typed native trampoline calls")
    if "direct_compiled_target" not in lowering:
        failures.append("Cranelift lowering lacks compiled-to-compiled direct calls")
    forbidden_runtime = re.compile(
        r"execute_(?:bytecode|dense|ir)|rich_"
        + "dispatch|resume_"
        + "call|tail"
        + "call",
        re.IGNORECASE,
    )
    if forbidden_runtime.search(trampoline):
        failures.append("native call trampoline references an interpreter/resume loop")
    combined = abi + region + lowering + trampoline
    for forbidden in ("JIT_HELPER_STATUS_TAILCALL", "JIT_HELPER_STATUS_RESUME_CALL_BASE"):
        if forbidden in combined:
            failures.append(f"retired executor-transition status remains: {forbidden}")
    for forbidden in (r"\bFunctionCall\b", r"\bPreparedArg\b", r"Vec\s*<\s*Value\s*>"):
        if re.search(forbidden, region + lowering):
            failures.append(f"native call lowering constructs forbidden {forbidden}")

    if not REPORT.is_file():
        failures.append("native call-model report was not generated")
    else:
        report = json.loads(REPORT.read_text(encoding="utf-8"))
        forms = set(report.get("ir_call_forms", []))
        callbacks = {entry.get("name") for entry in report.get("callback_kinds", [])}
        if forms != CALL_FORMS:
            failures.append("generated report does not cover every IR call form")
        if callbacks != CALLBACKS:
            failures.append("generated report does not cover every runtime callback family")
        if report.get("interpreter_reentry") is not False:
            failures.append("native call report permits interpreter re-entry")

    retired_emitter = ROOT / "crates/php_jit/src" / ("copy" + "_" + "patch") / "value_leaf.rs"
    if retired_emitter.exists():
        failures.append("retired tailcall emitter source still exists")
    if failures:
        print("Cranelift native-call gate failed:", file=sys.stderr)
        print("\n".join(f"- {failure}" for failure in failures), file=sys.stderr)
        return 1
    print(
        "Cranelift native-call gate passed "
        f"({len(CALL_FORMS)} IR forms, {len(CALLBACKS)} callback kinds)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
