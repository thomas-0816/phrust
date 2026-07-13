#!/usr/bin/env python3
"""Inspect the release server for its mandatory compiler and retired engines."""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path


NATIVE_MARKERS = ("CraneliftNativeCompiler", "cranelift_jit", "JITModule")
RETIRED_MARKERS = (
    "execute_ir_function",
    "execute_dense_activation",
    "execute_bytecode_function",
    "execute_function_with_dense_plan",
    "rich_dispatch",
    "copy" + "_and_patch",
    "copy" + "-patch",
)


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: native_server_symbols.py <release-server>", file=sys.stderr)
        return 2
    binary = Path(sys.argv[1])
    if not binary.is_file():
        print(f"release server does not exist: {binary}", file=sys.stderr)
        return 2
    result = subprocess.run(
        ["nm", "-C", str(binary)], text=True, capture_output=True, check=False
    )
    if result.returncode != 0:
        print(f"nm failed for {binary}: {result.stderr.strip()}", file=sys.stderr)
        return 2
    symbols = result.stdout
    if not any(marker in symbols for marker in NATIVE_MARKERS):
        print(
            "release server has no recognizable Cranelift compiler symbol "
            f"({', '.join(NATIVE_MARKERS)})",
            file=sys.stderr,
        )
        return 1
    found = [marker for marker in RETIRED_MARKERS if marker in symbols]
    if found:
        print("release server links retired execution symbols: " + ", ".join(found), file=sys.stderr)
        return 1
    print("release server native-only symbol gate passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
