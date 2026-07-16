#!/usr/bin/env python3
"""Verify Prompt 3's authoritative php_ir -> Region IR -> Cranelift path."""

from __future__ import annotations

import sys
from pathlib import Path

from rust_module import read_rust_module


ROOT = Path(__file__).resolve().parents[2]
REGION = ROOT / "crates/php_jit/src/region_ir/executable.rs"
LOWERING = ROOT / "crates/php_jit/src/cranelift_lowering.rs"
GRAPH_LOWERING = ROOT / "crates/php_jit/src/cranelift_lowering/executable_region.rs"
VM = ROOT / "crates/php_vm/src/vm/mod.rs"
CODE_KEY = ROOT / "crates/php_jit/src/code_manager.rs"


def require(text: str, needle: str, label: str, failures: list[str]) -> None:
    if needle not in text:
        failures.append(f"missing {label}: {needle}")


def main() -> int:
    region = REGION.read_text(encoding="utf-8")
    lowering = read_rust_module(LOWERING)
    graph_lowering = GRAPH_LOWERING.read_text(encoding="utf-8")
    vm = VM.read_text(encoding="utf-8")
    code_key = CODE_KEY.read_text(encoding="utf-8")
    failures: list[str] = []

    for needle, label in (
        ("pub struct BaselineRegionBuilder", "baseline builder"),
        ("runtime_metadata: &CompileMetadata", "runtime metadata input"),
        ("pub struct RegionGraph", "production Region graph"),
        ("source_kind: InstructionKind", "authoritative instruction retention"),
        ("source_terminator: TerminatorKind", "authoritative terminator retention"),
        ("MissingLowering", "concrete missing-lowering representation"),
        ("strict_types: unit.strict_types_for_function(function)", "strict-types metadata"),
        ("captures: ir_function.captures.clone()", "closure capture metadata"),
        ("declarations: declaration_metadata(unit, function)", "declaration metadata"),
        ("let exception_regions = collect_exception_regions(ir_function)", "exception regions"),
    ):
        require(region, needle, label, failures)

    authoritative_start = lowering.find("fn compile_authoritative_region")
    authoritative_end = lowering.find("fn runtime_helper_abi_hash", authoritative_start)
    authoritative = lowering[authoritative_start:authoritative_end]
    require(authoritative, "BaselineRegionBuilder::build", "direct baseline build", failures)
    require(authoritative, "compile_region_graph_native", "direct graph lowering", failures)
    for forbidden in (
        "constant_return_candidate",
        "packed_array_fetch_candidate",
        "known_call_candidate",
        "property_load_candidate",
        "analyze_jit_eligibility",
    ):
        if forbidden in authoritative:
            failures.append(f"production compiler still invokes candidate chain: {forbidden}")

    for path, text in ((REGION, region), (GRAPH_LOWERING, graph_lowering)):
        if "Dense" + "Bytecode" in text or "Dense" + "Opcode" in text:
            failures.append(f"{path.relative_to(ROOT)} consumes Dense bytecode")

    require(
        vm,
        "compile_unit_with_runtime_helpers(",
        "whole-unit baseline compilation",
        failures,
    )
    require(code_key, "compiler_tier", "compiler-tier cache identity", failures)
    require(code_key, "helper_abi_hash", "helper cache identity", failures)
    require(code_key, "target_cpu", "target/CPU cache identity", failures)
    require(code_key, "semantic_config_hash", "semantic cache identity", failures)
    require(code_key, "dependency_identity", "dependency cache identity", failures)

    if failures:
        print("Cranelift baseline IR coverage gate failed:", file=sys.stderr)
        print("\n".join(f"- {failure}" for failure in failures), file=sys.stderr)
        return 1
    print("Cranelift baseline IR coverage gate passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
