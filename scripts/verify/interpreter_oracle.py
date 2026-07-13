#!/usr/bin/env python3
"""Exercise the pinned pre-cutover interpreter as an external process."""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_BINARY = ROOT.parent / "phrust-interpreter-oracle/target/oracle/debug/php-vm"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--binary",
        type=Path,
        default=Path(os.environ.get("PHRUST_INTERPRETER_ORACLE", DEFAULT_BINARY)),
    )
    parser.add_argument("--print-path", action="store_true")
    args = parser.parse_args()
    binary = args.binary.resolve()
    if args.print_path:
        print(binary)
        return 0
    if not binary.is_file():
        print(
            f"interpreter oracle binary is missing: {binary}; build the pinned worktree first",
            file=sys.stderr,
        )
        return 1

    fixture_dir = ROOT / "target/cranelift-only"
    fixture_dir.mkdir(parents=True, exist_ok=True)
    fixture = fixture_dir / "interpreter-oracle-smoke.php"
    fixture.write_text(
        "<?php function oracle_add(int $a, int $b): int { return $a + $b; } "
        'echo oracle_add(19, 23), "\\n";\n',
        encoding="utf-8",
    )
    command = [
        str(binary),
        "run",
        "--engine-preset=baseline",
        "--bytecode-cache=off",
        str(fixture),
    ]
    result = subprocess.run(command, cwd=ROOT, capture_output=True)
    if result.returncode != 0 or result.stdout != b"42\n" or result.stderr:
        print("external interpreter oracle smoke failed", file=sys.stderr)
        print(f"command: {' '.join(command)}", file=sys.stderr)
        print(f"exit: {result.returncode}", file=sys.stderr)
        print(f"stdout: {result.stdout!r}", file=sys.stderr)
        print(f"stderr: {result.stderr!r}", file=sys.stderr)
        return 1
    print(f"external interpreter oracle passed: {binary}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
