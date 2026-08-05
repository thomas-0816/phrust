#!/usr/bin/env python3
"""Prove that compiler semantic operations have no runtime IR dispatcher."""

from __future__ import annotations

import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
SEMANTIC_OPS = ROOT / "crates/php_jit/src/region_ir/semantic_ops.rs"
DELETED_EXACT_INTERPRETER = ROOT / "crates/php_vm/src/vm/jit_abi/native_exact_semantics.rs"
LOWERING = ROOT / "crates/php_jit/src/cranelift_lowering.rs"
EXECUTABLE = ROOT / "crates/php_jit/src/cranelift_lowering/executable_region.rs"
DELETED_DISPATCHERS = (
    ROOT / "crates/php_vm/src/vm/jit_abi/baseline_semantic_dispatch.rs",
    ROOT / "crates/php_vm/src/vm/jit_abi/baseline_call_dispatch.rs",
)


def main() -> int:
    failures: list[str] = []
    for path in DELETED_DISPATCHERS:
        if path.exists():
            failures.append(f"superseded dispatcher still exists: {path.relative_to(ROOT)}")
    if DELETED_EXACT_INTERPRETER.exists():
        failures.append(
            "fixed-symbol wrapper around the semantic IR interpreter still exists: "
            f"{DELETED_EXACT_INTERPRETER.relative_to(ROOT)}"
        )

    semantic_source = SEMANTIC_OPS.read_text(encoding="utf-8")
    lowering_source = LOWERING.read_text(encoding="utf-8") + EXECUTABLE.read_text(encoding="utf-8")

    enum_match = re.search(
        r"pub enum RegionSemanticOperationId\s*\{(?P<body>.*?)\n\}",
        semantic_source,
        re.DOTALL,
    )
    ids = [] if enum_match is None else [
        int(value)
        for value in re.findall(
            r"^\s*[A-Za-z][A-Za-z0-9_]*\s*=\s*(\d+),",
            enum_match["body"],
            re.MULTILINE,
        )
    ]
    if not ids or ids != list(range(1, len(ids) + 1)):
        failures.append("compiler-only semantic IDs must remain ordered and append-only")

    forbidden = (
        "jit_baseline_native_semantic_dispatch",
        "semantic_operation_from_frame",
        "execute_native_semantic_operation",
        "phrust_baseline_native_call_dispatch",
        "phrust_baseline_native_builtin_dispatch",
    )
    combined = lowering_source
    for fragment in forbidden:
        if fragment in combined:
            failures.append(f"generated semantic surface retains selector {fragment}")

    repository_sources = "\n".join(
        path.read_text(encoding="utf-8")
        for root in (ROOT / "crates/php_jit/src", ROOT / "crates/php_vm/src")
        for path in root.rglob("*.rs")
    )
    for fragment in (
        "native_exact_semantic",
        "exact_semantic_leaf!(",
        ".semantic_instruction()",
    ):
        if fragment in repository_sources:
            failures.append(f"runtime semantic IR interpretation remains: {fragment}")

    variants = [] if enum_match is None else re.findall(
        r"^\s*([A-Za-z][A-Za-z0-9_]*)\s*=\s*\d+,",
        enum_match["body"],
        re.MULTILINE,
    )
    missing = [
        variant
        for variant in variants
        if f"RegionSemanticOp::{variant}" not in lowering_source
        and not (
            variant == "ObjectClassName"
            and "RegionInstructionKind::FetchObjectClassName" in lowering_source
        )
    ]
    if missing:
        failures.append(f"generated lowering is missing semantic variants: {missing}")

    if failures:
        print("Cranelift generated semantic gate failed:", file=sys.stderr)
        print("\n".join(f"- {failure}" for failure in failures), file=sys.stderr)
        return 1
    print(f"Cranelift generated semantic gate passed ({len(ids)} compiler operations)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
