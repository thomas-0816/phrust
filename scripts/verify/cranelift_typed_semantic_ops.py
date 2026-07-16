#!/usr/bin/env python3
"""Ratchet typed Region semantic operations and their stable ABI IDs."""

from __future__ import annotations

import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
SEMANTIC_OPS = ROOT / "crates/php_jit/src/region_ir/semantic_ops.rs"
EXECUTABLE = ROOT / "crates/php_jit/src/region_ir/executable.rs"
CALL_METADATA = ROOT / "crates/php_jit/src/cranelift_lowering/call_metadata.rs"
SEMANTIC_DISPATCH = ROOT / "crates/php_vm/src/vm/jit_abi/semantic_dispatch/mod.rs"
CALL_DISPATCH = ROOT / "crates/php_vm/src/vm/jit_abi/call_dispatch.rs"


def main() -> int:
    failures: list[str] = []
    sources = {
        path: path.read_text(encoding="utf-8")
        for path in (
            SEMANTIC_OPS,
            EXECUTABLE,
            CALL_METADATA,
            SEMANTIC_DISPATCH,
            CALL_DISPATCH,
        )
    }

    for path in (EXECUTABLE, CALL_METADATA, SEMANTIC_DISPATCH, CALL_DISPATCH):
        if re.search(r'"__phrust_[^"]*"', sources[path]):
            failures.append(f"{path.relative_to(ROOT)} contains a synthetic semantic name")

    semantic_source = sources[SEMANTIC_OPS]
    enum_match = re.search(
        r"pub enum RegionSemanticOperationId\s*\{(?P<body>.*?)\n\}",
        semantic_source,
        re.DOTALL,
    )
    if enum_match is None:
        failures.append("RegionSemanticOperationId enum is missing")
        ids: list[int] = []
    else:
        ids = [
            int(value)
            for value in re.findall(r"^\s*[A-Za-z][A-Za-z0-9_]*\s*=\s*(\d+),", enum_match["body"], re.MULTILINE)
        ]
    if not ids or ids != list(range(1, len(ids) + 1)):
        failures.append("semantic operation IDs must be unique, ordered, and append-only from 1")

    required_fragments = {
        EXECUTABLE: ("RegionCallTarget::Semantic", "RegionSemanticOp::"),
        CALL_METADATA: (
            "JitNativeCallKind::SEMANTIC_OPERATION",
            "operation.operation_id().raw()",
        ),
        SEMANTIC_DISPATCH: (
            "semantic_operation_from_frame",
            "execute_native_semantic_operation",
        ),
        CALL_DISPATCH: ("semantic_operation_from_frame(frame)?",),
    }
    for path, fragments in required_fragments.items():
        for fragment in fragments:
            if fragment not in sources[path]:
                failures.append(f"{path.relative_to(ROOT)} lacks {fragment}")

    if failures:
        print("Cranelift typed semantic-operation gate failed:", file=sys.stderr)
        print("\n".join(f"- {failure}" for failure in failures), file=sys.stderr)
        return 1
    print(f"Cranelift typed semantic-operation gate passed ({len(ids)} stable operations)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
