#!/usr/bin/env python3
"""Deterministic native-only differential smoke for wordpress/php-ai-client."""

from __future__ import annotations

import argparse
import difflib
import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
EXPECTED_TAG = "1.3.1"
EXPECTED_REVISION = "631704201d15ffeff7091ad3bc7156db74054956"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--project", type=Path)
    parser.add_argument("--reference-php", type=Path)
    parser.add_argument("--vm", type=Path)
    parser.add_argument(
        "--fixture",
        type=Path,
        default=ROOT / "tests/fixtures/integration/php-ai-client-smoke.php",
    )
    parser.add_argument(
        "--out", type=Path, default=ROOT / "target/integration/php-ai-client-smoke"
    )
    return parser.parse_args()


def write_report(out: Path, report: dict[str, object]) -> None:
    out.mkdir(parents=True, exist_ok=True)
    (out / "report.json").write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )


def finish(out: Path, status: str, reason: str, code: int) -> int:
    write_report(out, {"status": status, "reason": reason})
    print(f"[{status}] {reason}")
    return code


def resolve_inputs(args: argparse.Namespace) -> tuple[Path, bool, Path, bool, Path]:
    configured_project = os.getenv("PHRUST_PHP_AI_CLIENT_DIR")
    configured_reference = os.getenv("REFERENCE_PHP")
    configured_vm = os.getenv("PHP_VM_CLI")
    project = args.project or Path(
        configured_project or ROOT / "third_party/php-ai-client"
    )
    reference = args.reference_php or Path(
        configured_reference or ROOT / "third_party/php-src/sapi/cli/php"
    )
    vm = args.vm or Path(configured_vm or ROOT / "target/cutover/php-vm")
    return (
        project.resolve(),
        args.project is not None or configured_project is not None,
        reference.resolve(),
        args.reference_php is not None or configured_reference is not None,
        vm.resolve(),
    )


def git_value(project: Path, *arguments: str) -> str:
    completed = subprocess.run(
        ["git", "-C", str(project), *arguments],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=10,
        check=False,
    )
    if completed.returncode != 0:
        raise RuntimeError(completed.stderr.strip() or "git metadata lookup failed")
    return completed.stdout.strip()


def run(command: list[str], cwd: Path) -> subprocess.CompletedProcess[bytes]:
    return subprocess.run(
        command,
        cwd=cwd,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=30,
        check=False,
    )


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def display_path(path: Path) -> str:
    try:
        return str(path.resolve().relative_to(ROOT))
    except ValueError:
        return str(path.resolve())


def main() -> int:
    args = parse_args()
    project, project_is_strict, reference, reference_is_strict, vm = resolve_inputs(args)
    out = args.out.resolve()

    if not project.is_dir():
        reason = f"php-ai-client checkout unavailable: {project}"
        return finish(out, "fail" if project_is_strict else "skip", reason, 2 if project_is_strict else 0)
    if not (project / "vendor/autoload.php").is_file():
        reason = f"php-ai-client vendor/autoload.php unavailable: {project}"
        return finish(out, "fail" if project_is_strict else "skip", reason, 2 if project_is_strict else 0)
    if not args.fixture.is_file():
        return finish(out, "fail", f"integration fixture unavailable: {args.fixture}", 2)
    if not reference.is_file() or not os.access(reference, os.X_OK):
        reason = f"PHP 8.5.7 reference binary unavailable: {reference}"
        return finish(
            out,
            "fail" if reference_is_strict else "skip",
            reason,
            2 if reference_is_strict else 0,
        )
    if not vm.is_file() or not os.access(vm, os.X_OK):
        return finish(out, "fail", f"php-vm binary unavailable: {vm}", 2)

    try:
        revision = git_value(project, "rev-parse", "HEAD")
        tag = git_value(project, "describe", "--tags", "--exact-match")
        dirty = git_value(project, "status", "--porcelain")
    except (OSError, RuntimeError, subprocess.TimeoutExpired) as error:
        return finish(out, "fail", f"php-ai-client Git verification failed: {error}", 2)
    if revision != EXPECTED_REVISION or tag != EXPECTED_TAG or dirty:
        return finish(
            out,
            "fail",
            "php-ai-client checkout must be clean at "
            f"tag {EXPECTED_TAG} revision {EXPECTED_REVISION}",
            2,
        )

    counters_path = out / "native-counters.json"
    out.mkdir(parents=True, exist_ok=True)
    reference_run = run([str(reference), str(args.fixture), str(project)], ROOT)
    candidate_run = run(
        [
            str(vm),
            "run",
            "--native-cache",
            "off",
            "--counters-json",
            str(counters_path),
            str(args.fixture),
            "--",
            str(project),
        ],
        ROOT,
    )
    (out / "reference.stdout").write_bytes(reference_run.stdout)
    (out / "reference.stderr").write_bytes(reference_run.stderr)
    (out / "candidate.stdout").write_bytes(candidate_run.stdout)
    (out / "candidate.stderr").write_bytes(candidate_run.stderr)
    diff = "".join(
        difflib.unified_diff(
            reference_run.stdout.decode("utf-8", "replace").splitlines(keepends=True),
            candidate_run.stdout.decode("utf-8", "replace").splitlines(keepends=True),
            fromfile="reference.stdout",
            tofile="candidate.stdout",
        )
    )
    (out / "stdout.diff").write_text(diff, encoding="utf-8")

    try:
        counters = json.loads(counters_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        return finish(out, "fail", f"native counters unavailable: {error}", 1)
    counter_errors = []
    for key in ("native_compile_successes", "native_execution_entries", "native_region_entries"):
        if int(counters.get(key, 0)) < 1:
            counter_errors.append(f"{key} must be positive")
    if int(counters.get("native_compile_failures", 0)) != 0:
        counter_errors.append("native_compile_failures must be zero")
    for key in ("native_cache_hits", "native_cache_misses", "native_cache_writes"):
        if int(counters.get(key, 0)) != 0:
            counter_errors.append(f"{key} must be zero with cache disabled")

    matches = (
        reference_run.returncode == 0
        and candidate_run.returncode == 0
        and reference_run.stdout == candidate_run.stdout
        and reference_run.stderr == candidate_run.stderr
        and not counter_errors
    )
    report: dict[str, object] = {
        "status": "ok" if matches else "fail",
        "package": "wordpress/php-ai-client",
        "tag": tag,
        "revision": revision,
        "fixture": display_path(args.fixture),
        "native_cache": {"mode": "off", "started_empty": True},
        "reference": {
            "exit_code": reference_run.returncode,
            "stdout_sha256": digest(reference_run.stdout),
            "stderr_sha256": digest(reference_run.stderr),
        },
        "candidate": {
            "exit_code": candidate_run.returncode,
            "stdout_sha256": digest(candidate_run.stdout),
            "stderr_sha256": digest(candidate_run.stderr),
        },
        "native_only": {
            "native_functions_compiled_or_loaded": counters.get("native_compile_successes", 0),
            "native_entries_executed": counters.get("native_execution_entries", 0),
            "native_region_entries": counters.get("native_region_entries", 0),
            "runtime_helper_calls": counters.get("runtime_helper_calls", 0),
            "native_transitions": counters.get("native_transition_count", 0),
            "interpreter_entries": "structurally_impossible",
            "retired_backend_entries": "structurally_impossible",
        },
        "counter_errors": counter_errors,
    }
    write_report(out, report)
    if not matches:
        print(f"[fail] php-ai-client native differential failed; report={out / 'report.json'}")
        return 1
    print(
        "[ok] php-ai-client native differential: "
        f"tag={tag} revision={revision} report={out / 'report.json'}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
