#!/usr/bin/env python3
"""Reject unlisted or newly-added legacy executor references during cutover."""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
CONFIG_PATH = ROOT / "scripts/verify/cranelift_only_allowlist.json"
SCANNED_ROOTS = ("crates", "scripts", "docs", "Cargo.toml", "justfile")
IGNORED = {
    "scripts/verify/cranelift_only_allowlist.json",
    "scripts/verify/cranelift_only_stage_ratchet.py",
    "scripts/verify/no_copy_patch.py",
}
COPY_PATTERN = re.compile(
    "copy" + r".?" + "patch" + "|jit" + "-copy-patch|PHRUST_JIT_" + "COPY_PATCH",
    re.IGNORECASE,
)
INTERPRETER_PATTERN = re.compile(
    "execute_" + "bytecode_function|execute_dense_activation|"
    "execute_function_with_dense_plan|rich_" + "dispatch|execute_ir_function"
)


def git(*args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", *args], cwd=ROOT, text=True, capture_output=True, check=check
    )


def tracked_and_untracked_files() -> list[str]:
    tracked = git("ls-files", *SCANNED_ROOTS).stdout.splitlines()
    untracked = git(
        "ls-files", "--others", "--exclude-standard", *SCANNED_ROOTS
    ).stdout.splitlines()
    return sorted(set(tracked + untracked) - IGNORED)


def matching_paths(pattern: re.Pattern[str]) -> set[str]:
    matches: set[str] = set()
    for relative in tracked_and_untracked_files():
        path = ROOT / relative
        if not path.is_file():
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except (UnicodeDecodeError, IsADirectoryError):
            continue
        if pattern.search(text):
            matches.add(relative)
    return matches


def added_legacy_lines(pre_cutover_sha: str) -> list[str]:
    diff = git("diff", "--unified=0", pre_cutover_sha, "--", *SCANNED_ROOTS).stdout
    path = ""
    violations: list[str] = []
    for line in diff.splitlines():
        if line.startswith("+++ b/"):
            path = line[6:]
            continue
        if not line.startswith("+") or line.startswith("+++") or path in IGNORED:
            continue
        payload = line[1:]
        if COPY_PATTERN.search(payload) or INTERPRETER_PATTERN.search(payload):
            violations.append(f"{path}: {payload.strip()}")
    return violations


def main() -> int:
    config = json.loads(CONFIG_PATH.read_text(encoding="utf-8"))
    failures: list[str] = []
    for key, pattern in (
        ("alternate_emitter_paths", COPY_PATTERN),
        ("interpreter_call_paths", INTERPRETER_PATTERN),
    ):
        actual = matching_paths(pattern)
        if key == "interpreter_call_paths":
            actual = {path for path in actual if path.startswith("crates/")}
        allowed = set(config[key])
        unlisted = sorted(actual - allowed)
        stale = sorted(allowed - actual)
        if unlisted:
            failures.append(f"{key} has unlisted paths: {', '.join(unlisted)}")
        if stale:
            failures.append(
                f"{key} must shrink after removals; stale paths: {', '.join(stale)}"
            )
    added = added_legacy_lines(config["pre_cutover_sha"])
    if added:
        failures.append("new legacy references are forbidden:\n  " + "\n  ".join(added))
    if failures:
        print("cranelift-only stage ratchet failed:", file=sys.stderr)
        print("\n".join(f"- {failure}" for failure in failures), file=sys.stderr)
        return 1
    print(
        "cranelift-only stage ratchet passed "
        f"(stage={config['stage']}, alternate_emitter_paths={len(config['alternate_emitter_paths'])}, "
        f"interpreter_paths={len(config['interpreter_call_paths'])})"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
