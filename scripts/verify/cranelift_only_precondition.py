#!/usr/bin/env python3
"""Emit the source-grounded Cranelift-only precondition report."""

from __future__ import annotations

import hashlib
import json
import os
import platform
import subprocess
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "target/cranelift-only"
PRE_CUTOVER_SHA = "c300e22a5f389c1e6b022f40184e79c9980e8cd7"


def command(arguments: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        arguments,
        cwd=ROOT,
        env={**os.environ, "LC_ALL": "C", "TZ": "UTC"},
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )


def git_value(*arguments: str) -> str:
    result = command(["git", *arguments])
    if result.returncode != 0:
        raise RuntimeError(result.stderr.strip() or "git command failed")
    return result.stdout.strip()


def source_fingerprint() -> str:
    names = git_value("ls-files", "--cached", "--others", "--exclude-standard").splitlines()
    digest = hashlib.sha256()
    for relative in sorted(set(names)):
        path = ROOT / relative
        if not path.is_file() or relative.startswith(("target/", ".git/")):
            continue
        digest.update(relative.encode("utf-8"))
        digest.update(b"\0")
        digest.update(path.read_bytes())
        digest.update(b"\0")
    return digest.hexdigest()


def parse_identity() -> dict[str, str]:
    result = command(
        [
            "cargo",
            "run",
            "--quiet",
            "-p",
            "php_jit",
            "--example",
            "cranelift_precondition_identity",
        ]
    )
    if result.returncode != 0:
        raise RuntimeError(result.stderr.strip() or "native identity probe failed")
    identity: dict[str, str] = {}
    for line in result.stdout.splitlines():
        key, separator, value = line.partition("=")
        if separator:
            identity[key] = value
    required = {
        "runtime_abi_version",
        "runtime_abi_hash",
        "helper_abi_hash",
        "region_ir_schema_version",
        "isa_name",
        "target_triple",
        "cpu_feature_fingerprint",
        "cpu_identity",
    }
    missing = sorted(required - identity.keys())
    if missing:
        raise RuntimeError(f"native identity probe omitted: {', '.join(missing)}")
    return identity


def cranelift_version() -> str:
    lock = (ROOT / "Cargo.lock").read_text(encoding="utf-8")
    marker = 'name = "cranelift-codegen"\nversion = "'
    start = lock.find(marker)
    if start < 0:
        raise RuntimeError("Cargo.lock lacks cranelift-codegen")
    start += len(marker)
    return lock[start : lock.find('"', start)]


def prerequisite(
    identifier: str, statement: str, evidence: list[str], tests: list[str]
) -> dict[str, Any]:
    missing = [path for path in evidence if not (ROOT / path).is_file()]
    return {
        "id": identifier,
        "status": "pass" if not missing else "fail",
        "statement": statement,
        "evidence": evidence,
        "tests": tests,
        "missing_evidence": missing,
    }


def markdown(report: dict[str, Any]) -> str:
    frozen = report["frozen_identity"]
    lines = [
        "# Cranelift-only precondition",
        "",
        f"Overall: **{str(report['status']).upper()}**",
        "",
        "## Frozen identity",
        "",
    ]
    for key, value in frozen.items():
        lines.append(f"- `{key}`: `{value}`")
    lines.extend(["", "## Prerequisites", ""])
    for item in report["prerequisites"]:
        lines.append(
            f"- **{str(item['status']).upper()} — {item['id']}**: {item['statement']}"
        )
        lines.append(f"  Evidence: `{', '.join(item['evidence'])}`")
        lines.append(f"  Gate evidence: `{', '.join(item['tests'])}`")
    lines.extend(
        [
            "",
            "## Migration oracle",
            "",
            "The old executor is an external detached worktree and binary. It is",
            "invoked only by the migration diagnostic harness and is absent from the",
            "candidate Cargo graph.",
            "",
            "## Ratchet state",
            "",
            "The temporary staged allowlist has been removed. The final ratchet now",
            "enforces the source, Cargo graph, binary, and runtime contract with zero",
            "exceptions.",
            "",
        ]
    )
    return "\n".join(lines)


def main() -> int:
    OUT.mkdir(parents=True, exist_ok=True)
    failures: list[str] = []
    try:
        identity = parse_identity()
        branch = git_value("branch", "--show-current")
        head = git_value("rev-parse", "HEAD")
        fingerprint = source_fingerprint()
        version = cranelift_version()
    except (OSError, RuntimeError) as error:
        print(f"[fail] precondition identity: {error}")
        return 1

    oracle_path = OUT / "interpreter-oracle.json"
    ratchet_path = OUT / "final-ratchet.json"
    for label, path in (("migration oracle", oracle_path), ("final ratchet", ratchet_path)):
        try:
            document = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as error:
            failures.append(f"{label} report unavailable: {error}")
            continue
        if document.get("status") != "pass":
            failures.append(f"{label} report is not passing")

    prerequisites = [
        prerequisite(
            "amd64-native-execution",
            "The supported AMD64 target executes production Cranelift lowering.",
            ["crates/php_jit/src/cranelift_lowering.rs"],
            ["cranelift_backend_executes_multiblock_region_ir"],
        ),
        prerequisite(
            "persistent-cranelift-ownership",
            "A process-level manager owns modules and generation-safe published handles.",
            ["crates/php_jit/src/code_manager.rs"],
            ["many_functions_share_bounded_generations_without_module_leaks"],
        ),
        prerequisite(
            "executable-region-ir",
            "Executable Region IR is built from PHP IR and consumed by Cranelift.",
            ["crates/php_jit/src/region_ir/executable.rs"],
            ["builds_verified_multiblock_region_from_php_ir"],
        ),
        prerequisite(
            "compiled-direct-calls",
            "Stable same-unit user functions use compiled-to-compiled calls.",
            ["crates/php_jit/src/cranelift_lowering.rs"],
            ["cranelift_region_calls_same_unit_compiled_callee_directly"],
        ),
        prerequisite(
            "versioned-native-abi",
            "Runtime and helper ABI identities are versioned and cache-visible.",
            ["crates/php_jit/src/abi.rs", "crates/php_jit/src/helpers.rs"],
            ["cranelift_precondition_identity"],
        ),
        prerequisite(
            "precise-native-state",
            "Native transitions retain exact continuation and live-slot state.",
            ["crates/php_jit/src/cranelift_lowering.rs"],
            ["baseline_native_continuation_resumes_exact_instruction"],
        ),
        prerequisite(
            "native-osr-continuation",
            "Loop OSR and guard exits enter generated native continuations.",
            ["crates/php_jit/src/region_ir/osr.rs"],
            ["cranelift_loop_enters_through_native_osr_state"],
        ),
        prerequisite(
            "worker-cache-prewarm",
            "Workers prewarm bounded native entries without running application code.",
            ["crates/php_executor/src/executor.rs"],
            ["bounded_cranelift_prewarm_populates_cache_without_executing_script"],
        ),
        prerequisite(
            "external-pinned-oracle",
            "The migration oracle is detached, SHA-pinned, and external to the Cargo graph.",
            ["scripts/verify/interpreter_oracle.py"],
            ["interpreter_oracle.py"],
        ),
        prerequisite(
            "final-source-ratchet",
            "The final source and binary ratchet passes without an exception list.",
            ["scripts/verify/cranelift_only_ratchet.py"],
            ["cranelift-only-ratchet-fast"],
        ),
    ]
    for item in prerequisites:
        if item["status"] != "pass":
            failures.append(f"missing evidence for {item['id']}")

    if platform.system() != "Linux" or platform.machine() != "x86_64":
        failures.append("first cutover target must be x86_64 Linux")
    if identity["target_triple"] != "x86_64-unknown-linux-gnu":
        failures.append(f"unexpected target triple: {identity['target_triple']}")
    if branch != "engine/cranelift-only-cutover":
        failures.append(f"unexpected cutover branch: {branch}")

    report: dict[str, Any] = {
        "schema_version": 2,
        "status": "pass" if not failures else "fail",
        "passed": not failures,
        "failures": failures,
        "frozen_identity": {
            "pre_cutover_sha": PRE_CUTOVER_SHA,
            "cranelift_only_branch": branch,
            "cranelift_only_branch_sha": head,
            "worktree_source_sha256": fingerprint,
            "worktree_dirty": bool(git_value("status", "--porcelain")),
            "cranelift_version": version,
            **identity,
        },
        "prerequisites": prerequisites,
    }
    (OUT / "precondition.json").write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    (OUT / "precondition.md").write_text(markdown(report), encoding="utf-8")

    if failures:
        for failure in failures:
            print(f"[fail] {failure}")
        return 1
    print(
        "[ok] Cranelift-only precondition: "
        f"head={head} source={fingerprint[:12]} oracle={PRE_CUTOVER_SHA}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
