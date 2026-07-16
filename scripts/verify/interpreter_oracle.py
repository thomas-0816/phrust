#!/usr/bin/env python3
"""Verify and execute the detached pre-cutover migration oracle."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_WORKTREE = ROOT.parent / "phrust-interpreter-oracle"
PRE_CUTOVER_SHA = "c300e22a5f389c1e6b022f40184e79c9980e8cd7"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--worktree", type=Path, default=DEFAULT_WORKTREE)
    parser.add_argument("--binary", type=Path)
    parser.add_argument(
        "--fixture", type=Path, default=ROOT / "fixtures/runtime/valid/hello.php"
    )
    parser.add_argument(
        "--reference-php",
        type=Path,
        default=ROOT / "third_party/php-src/sapi/cli/php",
    )
    parser.add_argument(
        "--out",
        type=Path,
        default=ROOT / "target/cranelift-only/interpreter-oracle.json",
    )
    return parser.parse_args()


def run(
    arguments: list[str], *, cwd: Path = ROOT, timeout: float = 30.0
) -> subprocess.CompletedProcess[bytes]:
    return subprocess.run(
        arguments,
        cwd=cwd,
        env={**os.environ, "LC_ALL": "C", "TZ": "UTC"},
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
        check=False,
    )


def text_command(arguments: list[str], cwd: Path) -> tuple[int, str, str]:
    completed = run(arguments, cwd=cwd, timeout=10.0)
    return (
        completed.returncode,
        completed.stdout.decode("utf-8", "replace").strip(),
        completed.stderr.decode("utf-8", "replace").strip(),
    )


def digest(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def execution_record(completed: subprocess.CompletedProcess[bytes]) -> dict[str, Any]:
    return {
        "exit_code": completed.returncode,
        "stdout_sha256": digest(completed.stdout),
        "stderr_sha256": digest(completed.stderr),
    }


def write_report(path: Path, report: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def main() -> int:
    args = parse_args()
    worktree = args.worktree.resolve()
    binary = (args.binary or worktree / "target/oracle/debug/php-vm").resolve()
    fixture = args.fixture.resolve()
    reference = args.reference_php.resolve()
    failures: list[str] = []

    if not worktree.is_dir():
        failures.append(f"oracle worktree is missing: {worktree}")
        head = ""
        detached = False
    else:
        code, head, error = text_command(["git", "rev-parse", "HEAD"], worktree)
        if code != 0:
            failures.append(f"cannot read oracle HEAD: {error}")
        elif head != PRE_CUTOVER_SHA:
            failures.append(
                f"oracle HEAD is {head}, expected pinned pre-cutover SHA {PRE_CUTOVER_SHA}"
            )
        branch_code, _, _ = text_command(
            ["git", "symbolic-ref", "-q", "HEAD"], worktree
        )
        detached = branch_code != 0
        if not detached:
            failures.append("oracle worktree must remain detached")

    for label, path in (
        ("oracle binary", binary),
        ("PHP 8.5.7 reference", reference),
    ):
        if not path.is_file() or not os.access(path, os.X_OK):
            failures.append(f"{label} is unavailable or not executable: {path}")
    if not fixture.is_file():
        failures.append(f"oracle fixture is missing: {fixture}")

    oracle_run: subprocess.CompletedProcess[bytes] | None = None
    reference_run: subprocess.CompletedProcess[bytes] | None = None
    if not failures:
        version = run([str(reference), "-r", "echo PHP_VERSION;"], timeout=10.0)
        if version.returncode != 0 or version.stdout != b"8.5.7":
            failures.append("reference binary must report exactly PHP 8.5.7")
        else:
            oracle_run = run(
                [
                    str(binary),
                    "run",
                    "--engine-preset",
                    "baseline",
                    str(fixture),
                ]
            )
            reference_run = run([str(reference), str(fixture)])
            if (
                oracle_run.returncode != reference_run.returncode
                or oracle_run.stdout != reference_run.stdout
                or oracle_run.stderr != reference_run.stderr
            ):
                failures.append("pinned oracle output differs from PHP 8.5.7")

    report: dict[str, Any] = {
        "schema_version": 1,
        "status": "pass" if not failures else "fail",
        "pre_cutover_sha": PRE_CUTOVER_SHA,
        "oracle_head": head,
        "detached_worktree": detached,
        "worktree": str(worktree),
        "binary": str(binary),
        "fixture": str(fixture.relative_to(ROOT)) if fixture.is_relative_to(ROOT) else str(fixture),
        "reference_php": str(reference),
        "failures": failures,
    }
    if oracle_run is not None and reference_run is not None:
        report["oracle"] = execution_record(oracle_run)
        report["reference"] = execution_record(reference_run)
        report["byte_equal"] = True
    write_report(args.out.resolve(), report)

    if failures:
        for failure in failures:
            print(f"[fail] {failure}")
        return 1
    print(
        "[ok] detached migration oracle: "
        f"sha={PRE_CUTOVER_SHA} fixture={report['fixture']}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
