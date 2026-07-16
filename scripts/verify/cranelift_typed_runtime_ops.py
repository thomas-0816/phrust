#!/usr/bin/env python3
"""Verify the typed, backend-neutral runtime-operation ABI and its IR mapping."""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "crates/php_runtime/src/native_ops.rs"
AUDIT = ROOT / "target/cranelift-only/runtime-helper-audit.json"
COVERAGE = ROOT / "target/cranelift-only/instruction-coverage.json"
REQUIRED_FAMILIES = {
    "ConstantsDeclarations",
    "GlobalsStatics",
    "ReferencesLvalues",
    "ScalarOperators",
    "OutputDiagnostics",
    "CallsArguments",
    "ObjectsClone",
    "Properties",
    "StaticProperties",
    "ClassConstantsInstanceOf",
    "Arrays",
    "Foreach",
    "IncludeRequire",
    "Eval",
    "ExceptionsFinally",
    "GeneratorsFibers",
    "Autoload",
    "Destructors",
    "GcSafepoints",
    "Resources",
    "RuntimeErrors",
}


def main() -> int:
    failures: list[str] = []
    source = SOURCE.read_text(encoding="utf-8")
    for label, pattern in {
        "interpreter dependency": r"\b(?:use|extern\s+crate)\s+php_vm\b|\bphp_vm::",
        "IR instruction operand": r"\bInstructionKind\b",
        "generic opcode dispatch": r"execute_runtime_instruction|generic_dispatch|opcode\s*:\s*u32",
        "boxed result transfer": r"Box\s*<\s*Value\s*>",
        "unsafe runtime helper": r"\bunsafe\b",
    }.items():
        if re.search(pattern, source, re.IGNORECASE):
            failures.append(f"typed runtime source contains forbidden {label}")

    if not AUDIT.is_file():
        failures.append("runtime-helper audit JSON was not generated")
        operations: list[dict[str, object]] = []
    else:
        report = json.loads(AUDIT.read_text(encoding="utf-8"))
        if report.get("abi_version") != 1:
            failures.append("typed runtime ABI version is not 1")
        operations = report.get("operations", [])

    ids = [operation.get("id") for operation in operations]
    if ids != sorted(ids) or len(ids) != len(set(ids)):
        failures.append("helper IDs must be unique and strictly ordered")
    families = {operation.get("family") for operation in operations}
    missing_families = sorted(REQUIRED_FAMILIES - families)
    if missing_families:
        failures.append("missing required helper families: " + ", ".join(missing_families))

    by_id = {operation.get("id"): operation for operation in operations}
    for operation in operations:
        name = operation.get("name", "<unknown>")
        for key in (
            "signature_version",
            "args",
            "result",
            "ownership",
            "implementation",
            "direct_callers",
            "native_callers",
            "may_call_user_code",
            "may_allocate",
            "may_throw",
            "may_diagnose",
            "may_suspend",
            "gc_safepoint",
        ):
            if key not in operation:
                failures.append(f"{name} lacks audit field {key}")
        if operation.get("native_callable"):
            if not str(operation.get("implementation", "")).startswith(
                "php_runtime::api::native_"
            ):
                failures.append(f"{name} has no concrete typed native implementation")
            callers = set(operation.get("native_callers", []))
            if callers != {"baseline", "optimizing"}:
                failures.append(f"{name} is not callable from both native tiers")

    if not COVERAGE.is_file():
        failures.append("instruction coverage JSON was not generated")
    else:
        coverage = json.loads(COVERAGE.read_text(encoding="utf-8"))
        mapped = [
            entry
            for entry in coverage.get("entries", [])
            if entry.get("class", "").startswith("typed_runtime_helper:")
        ]
        if len(mapped) != 5:
            failures.append(f"expected 5 helper-mapped IR operations, found {len(mapped)}")
        for entry in mapped:
            helper = by_id.get(entry.get("helper_id"))
            if helper is None or not helper.get("native_callable"):
                failures.append(
                    f"{entry.get('variant')} maps to missing/non-callable helper {entry.get('helper_id')}"
                )

    if failures:
        print("Cranelift typed runtime-operation gate failed:", file=sys.stderr)
        print("\n".join(f"- {failure}" for failure in failures), file=sys.stderr)
        return 1
    print(
        "Cranelift typed runtime-operation gate passed "
        f"({len(operations)} operations, {len(REQUIRED_FAMILIES)} families, 5 native helpers)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
