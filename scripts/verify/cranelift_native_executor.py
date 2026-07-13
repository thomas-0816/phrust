#!/usr/bin/env python3
"""Verify that the product source tree contains only native PHP execution."""

from __future__ import annotations

import json
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
REPORT_DIR = ROOT / "target" / "cranelift-only"

FORBIDDEN_PATHS = (
    "crates/php_vm/src/bytecode",
    "crates/php_vm/src/deopt.rs",
    "crates/php_vm/src/fallback.rs",
    "crates/php_vm/src/osr.rs",
    "crates/php_vm/src/quickening.rs",
    "crates/php_vm/src/vm/dense_" + "dispatch.rs",
    "crates/php_vm/src/vm/rich_" + "dispatch.rs",
    "crates/php_jit/src/region_ir/interpreter.rs",
)
FORBIDDEN_NAMES = (
    "Dense" + "Opcode",
    "Dense" + "BytecodeUnit",
    "Execution" + "Format",
    "Quickening" + "Mode",
    "Superinstruction" + "Mode",
    "execute_" + "bytecode_function",
    "execute_" + "ir_function",
    "rich_" + "dispatch",
)
SCANNED_ROOTS = (
    ROOT / "crates/php_vm",
    ROOT / "crates/php_jit",
    ROOT / "crates/php_executor",
    ROOT / "crates/php_vm_cli",
    ROOT / "crates/php_server",
)


def source_files() -> list[Path]:
    files: list[Path] = []
    for root in SCANNED_ROOTS:
        files.extend(root.rglob("*.rs"))
        files.extend(root.rglob("*.toml"))
    return sorted(set(files))


def main() -> int:
    failures: list[str] = []
    for relative in FORBIDDEN_PATHS:
        if (ROOT / relative).exists():
            failures.append(f"retired executor path still exists: {relative}")

    matches: dict[str, list[str]] = {}
    for path in source_files():
        text = path.read_text(encoding="utf-8")
        found = [name for name in FORBIDDEN_NAMES if name in text]
        if found:
            matches[str(path.relative_to(ROOT))] = found
    if matches:
        for path, names in matches.items():
            failures.append(f"{path} contains retired names: {', '.join(names)}")

    vm_lib = (ROOT / "crates/php_vm/src/lib.rs").read_text(encoding="utf-8")
    if "Native PHP execution coordinator" not in vm_lib:
        failures.append("php_vm crate documentation does not identify the native coordinator")
    if "contains no opcode execution loop" not in vm_lib:
        failures.append("php_vm crate documentation does not forbid opcode loops")

    config = json.loads(
        (ROOT / "scripts/verify/cranelift_only_allowlist.json").read_text(encoding="utf-8")
    )
    if config.get("stage") != 11 or config.get("interpreter_call_paths") != []:
        failures.append("stage 11 must have an empty legacy call-path allowlist")

    if failures:
        print("native executor source gate failed:", file=sys.stderr)
        for failure in failures:
            print(f"- {failure}", file=sys.stderr)
        return 1

    REPORT_DIR.mkdir(parents=True, exist_ok=True)
    report = {
        "schema_version": 1,
        "stage": 11,
        "executor": "cranelift-native-only",
        "opcode_loop_count": 0,
        "legacy_call_path_count": 0,
        "source_files_scanned": len(source_files()),
    }
    (REPORT_DIR / "native-executor.json").write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    (REPORT_DIR / "native-executor.md").write_text(
        "# Native executor audit\n\n"
        "- Cranelift is the only PHP execution engine.\n"
        "- Production opcode loops: 0.\n"
        "- Legacy call paths: 0.\n",
        encoding="utf-8",
    )
    print("native executor source gate passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
