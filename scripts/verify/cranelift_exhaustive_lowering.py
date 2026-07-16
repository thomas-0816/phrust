#!/usr/bin/env python3
"""Verify exhaustive, non-dispatching baseline lowering coverage."""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
COVERAGE = ROOT / "crates/php_jit/src/region_ir/coverage.rs"
REGION = ROOT / "crates/php_jit/src/region_ir/executable.rs"
REPORT = ROOT / "target/cranelift-only/instruction-coverage.json"


def main() -> int:
    failures: list[str] = []
    source = COVERAGE.read_text(encoding="utf-8")
    region = REGION.read_text(encoding="utf-8")
    if re.search(r"\b_\s*=>", source):
        failures.append("authoritative coverage contains a wildcard match arm")
    for function in (
        "baseline_instruction_lowering",
        "baseline_terminator_lowering",
        "baseline_unary_class",
        "baseline_binary_class",
        "baseline_compare_class",
        "baseline_cast_class",
        "baseline_include_class",
        "baseline_callable_class",
        "baseline_call_arg_class",
    ):
        if function not in source:
            failures.append(f"missing exhaustive classifier: {function}")
    if "InstructionKind::RuntimeError" not in region or "RuntimeFatal" not in region:
        failures.append("RuntimeError is not represented as an explicit native fatal")
    if "MissingLowering" in region:
        failures.append("production Region IR still contains MissingLowering")
    for hidden_failure in ("map_or(RegionInstructionKind", "unwrap_or(RegionInstructionKind"):
        if hidden_failure in region:
            failures.append(f"conditional lowering fallback remains: {hidden_failure}")
    direct_missing = re.compile(
        r"(?P<arms>(?:InstructionKind::\w+\s*\{[^{}]*\}\s*(?:\|\s*)?)+)"
        r"=>\s*RegionInstructionKind::MissingLowering",
        re.DOTALL,
    )
    for match in direct_missing.finditer(region):
        variants = re.findall(r"InstructionKind::(\w+)", match.group("arms"))
        failures.append(
            "authoritative Region IR maps source instruction(s) directly to "
            f"MissingLowering: {', '.join(variants)}"
        )
    if not REPORT.is_file():
        failures.append("instruction coverage JSON was not generated")
    else:
        report = json.loads(REPORT.read_text(encoding="utf-8"))
        entries = report.get("entries", [])
        instructions = [entry for entry in entries if entry.get("kind") == "instruction"]
        terminators = [entry for entry in entries if entry.get("kind") == "terminator"]
        if len(instructions) != 101:
            failures.append(f"expected 101 instruction variants, found {len(instructions)}")
        if len(terminators) != 6:
            failures.append(f"expected 6 terminator variants, found {len(terminators)}")
        names = [entry.get("variant") for entry in instructions]
        if len(names) != len(set(names)):
            failures.append("instruction manifest contains duplicate variants")
        forbidden_helpers = re.compile(
            r"execute_(?:instruction|opcode)|generic_dispatch", re.IGNORECASE
        )
        for entry in entries:
            if forbidden_helpers.search(str(entry.get("helper_id", ""))):
                failures.append(f"generic dispatch helper in manifest: {entry}")

    if failures:
        print("Cranelift exhaustive lowering gate failed:", file=sys.stderr)
        print("\n".join(f"- {failure}" for failure in failures), file=sys.stderr)
        return 1
    print("Cranelift exhaustive lowering gate passed (101 instructions, 6 terminators)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
