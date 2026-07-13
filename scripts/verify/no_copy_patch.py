#!/usr/bin/env python3
"""Prompt 1 ratchet: reject every retired alternate-emitter reference."""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
PATTERN = re.compile(
    "copy" + r".?" + "patch" + "|jit" + "-copy-patch|PHRUST_JIT_" + "COPY_PATCH",
    re.IGNORECASE,
)
IGNORED = {
    "scripts/verify/no_copy_patch.py",
    "scripts/verify/cranelift_only_stage_ratchet.py",
}


def main() -> int:
    command = [
        "git",
        "ls-files",
        "--cached",
        "--others",
        "--exclude-standard",
        "--",
        "crates",
        "scripts",
        "docs",
        "Cargo.toml",
        "justfile",
    ]
    paths = subprocess.run(
        command, cwd=ROOT, text=True, capture_output=True, check=True
    ).stdout.splitlines()
    violations: list[str] = []
    for relative in sorted(set(paths) - IGNORED):
        path = ROOT / relative
        if not path.is_file():
            continue
        try:
            lines = path.read_text(encoding="utf-8").splitlines()
        except (UnicodeDecodeError, IsADirectoryError):
            continue
        for line_number, line in enumerate(lines, 1):
            if PATTERN.search(line):
                violations.append(f"{relative}:{line_number}: {line.strip()}")
    if violations:
        print("retired alternate-emitter references remain:", file=sys.stderr)
        print("\n".join(violations), file=sys.stderr)
        return 1
    print("alternate-emitter source ratchet passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
