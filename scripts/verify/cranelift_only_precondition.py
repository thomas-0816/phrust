#!/usr/bin/env python3
"""Write the source-grounded Prompt 0 Cranelift-only precondition report."""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
OUTPUT_DIR = ROOT / "target/cranelift-only"
IDENTITY_PATH = OUTPUT_DIR / "identity.txt"
CONFIG_PATH = ROOT / "scripts/verify/cranelift_only_allowlist.json"


def run(*command: str, cwd: Path = ROOT) -> str:
    return subprocess.run(
        command, cwd=cwd, text=True, capture_output=True, check=True
    ).stdout.strip()


def read(relative: str) -> str:
    return (ROOT / relative).read_text(encoding="utf-8")


def identity() -> dict[str, str]:
    values: dict[str, str] = {}
    for line in IDENTITY_PATH.read_text(encoding="utf-8").splitlines():
        key, separator, value = line.partition("=")
        if separator:
            values[key] = value
    return values


def cranelift_version() -> str:
    lock = read("Cargo.lock")
    match = re.search(
        r'name = "cranelift-codegen"\nversion = "([^"]+)"', lock
    )
    if match is None:
        raise RuntimeError("Cargo.lock has no cranelift-codegen package")
    return match.group(1)


def source_check(
    check_id: str,
    summary: str,
    prompt: int,
    evidence: list[tuple[str, str]],
    extra: bool = True,
) -> dict[str, object]:
    missing = [f"{path}: {needle}" for path, needle in evidence if needle not in read(path)]
    passed = extra and not missing
    return {
        "id": check_id,
        "summary": summary,
        "passed": passed,
        "evidence": [path for path, _ in evidence],
        "missing": missing,
        "remediation_prompt": prompt,
    }


def write_markdown(report: dict[str, object]) -> None:
    lines = [
        "# Cranelift-only precondition",
        "",
        f"Overall: **{'PASS' if report['passed'] else 'FAIL'}**",
        "",
        "## Frozen identity",
        "",
    ]
    frozen = report["identity"]
    assert isinstance(frozen, dict)
    for key, value in frozen.items():
        lines.append(f"- `{key}`: `{value}`")
    lines.extend(["", "## Prerequisites", ""])
    checks = report["checks"]
    assert isinstance(checks, list)
    for check in checks:
        assert isinstance(check, dict)
        marker = "PASS" if check["passed"] else "FAIL"
        lines.append(f"- **{marker} — {check['id']}**: {check['summary']}")
        lines.append(
            f"  Evidence: {', '.join(f'`{path}`' for path in check['evidence'])}. "
            f"Repair owner: Prompt {check['remediation_prompt']}."
        )
        if check["missing"]:
            lines.append(f"  Missing: {'; '.join(check['missing'])}")
    lines.extend(
        [
            "",
            "## Validation contract",
            "",
            "This report is emitted only after the stage ratchet, pinned external",
            "oracle smoke, native code-manager/lowering tests, and worker prewarm",
            "test have completed successfully in `just cranelift-only-precondition`.",
            "",
        ]
    )
    (OUTPUT_DIR / "precondition.md").write_text("\n".join(lines), encoding="utf-8")


def main() -> int:
    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    config = json.loads(CONFIG_PATH.read_text(encoding="utf-8"))
    native_identity = identity()
    pre_sha = config["pre_cutover_sha"]
    oracle_root = ROOT.parent / "phrust-interpreter-oracle"
    oracle_sha = run("git", "rev-parse", "HEAD", cwd=oracle_root)
    current_sha = run("git", "rev-parse", "HEAD")
    branch = run("git", "branch", "--show-current")
    target_is_amd64 = native_identity.get("isa_name") == "x64" and "x86_64" in native_identity.get(
        "target_triple", ""
    )

    checks = [
        source_check(
            "amd64-native-execution",
            "The host is the supported AMD64 Cranelift target and executes native lowering tests.",
            0,
            [("crates/php_jit/src/cranelift_lowering.rs", "cranelift_backend_executes_multiblock_region_ir")],
            target_is_amd64,
        ),
        source_check(
            "persistent-cranelift-ownership",
            "One process-level code manager owns Cranelift modules and published handles.",
            0,
            [("crates/php_jit/src/code_manager.rs", "static GLOBAL_CODE_MANAGER")],
        ),
        source_check(
            "bounded-module-lifecycle",
            "Many functions share bounded code generations instead of leaking one module per function.",
            0,
            [("crates/php_jit/src/code_manager.rs", "many_functions_share_bounded_generations_without_module_leaks")],
        ),
        source_check(
            "executable-region-ir",
            "Executable Region IR is the structured input consumed by Cranelift.",
            0,
            [("crates/php_jit/src/region_ir/executable.rs", "pub struct ExecutableRegion")],
        ),
        source_check(
            "multiblock-native-regions",
            "Cranelift executes verified multi-block Region IR.",
            0,
            [("crates/php_jit/src/cranelift_lowering.rs", "cranelift_backend_executes_multiblock_region_ir")],
        ),
        source_check(
            "compiled-direct-calls",
            "Same-unit compiled callees are linked directly.",
            0,
            [("crates/php_jit/src/cranelift_lowering.rs", "cranelift_region_calls_same_unit_compiled_callee_directly")],
        ),
        source_check(
            "versioned-native-helper-abi",
            "Runtime and helper ABI identities are versioned and non-zero.",
            0,
            [("crates/php_jit/src/abi.rs", "JIT_RUNTIME_ABI_VERSION"), ("crates/php_jit/src/helpers.rs", "JIT_HELPER_REGISTRY_ABI_HASH")],
            native_identity.get("runtime_abi_hash", "0") != "0" and native_identity.get("helper_abi_hash", "0") != "0",
        ),
        source_check(
            "precise-native-state",
            "Native exits retain precise continuation state.",
            0,
            [("crates/php_jit/src/cranelift_lowering.rs", "cranelift_overflow_materializes_precise_region_continuation")],
        ),
        source_check(
            "native-osr-continuation",
            "Loop OSR enters through a native state mapping.",
            0,
            [("crates/php_jit/src/cranelift_lowering.rs", "cranelift_loop_enters_through_native_osr_state")],
        ),
        source_check(
            "worker-cache-prewarm",
            "Workers can prewarm bounded native entries without executing application code.",
            0,
            [("crates/php_executor/src/executor.rs", "bounded_cranelift_prewarm_populates_cache_without_executing_script")],
        ),
        source_check(
            "warm-request-no-compile",
            "A prewarmed request hits the native cache and performs zero compile attempts.",
            0,
            [("crates/php_executor/src/executor.rs", "assert_eq!(counters.jit_compile_attempts, 0")],
        ),
        source_check(
            "external-pinned-oracle",
            "The pre-cutover interpreter oracle is detached, external, and pinned exactly.",
            0,
            [("scripts/verify/interpreter_oracle.py", "subprocess.run(command")],
            oracle_sha == pre_sha,
        ),
        source_check(
            "staged-source-ratchet",
            "Legacy source is frozen behind an explicit stage allowlist.",
            0,
            [("scripts/verify/cranelift_only_stage_ratchet.py", "new legacy references are forbidden")],
        ),
    ]
    report: dict[str, object] = {
        "schema_version": 1,
        "passed": all(check["passed"] for check in checks),
        "identity": {
            "pre_cutover_sha": pre_sha,
            "oracle_sha": oracle_sha,
            "branch": branch,
            "branch_sha": current_sha,
            "cranelift_version": cranelift_version(),
            "runtime_abi_version": native_identity.get("runtime_abi_version", ""),
            "runtime_abi_hash": native_identity.get("runtime_abi_hash", ""),
            "helper_abi_hash": native_identity.get("helper_abi_hash", ""),
            "region_ir_schema_version": native_identity.get("region_ir_schema_version", ""),
            "target_triple": native_identity.get("target_triple", ""),
            "isa_name": native_identity.get("isa_name", ""),
            "cpu_feature_fingerprint": native_identity.get("cpu_feature_fingerprint", ""),
            "cpu_identity": native_identity.get("cpu_identity", ""),
        },
        "checks": checks,
    }
    (OUTPUT_DIR / "precondition.json").write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    write_markdown(report)
    print(
        f"wrote {OUTPUT_DIR / 'precondition.json'} and {OUTPUT_DIR / 'precondition.md'}"
    )
    if not report["passed"]:
        print("Cranelift-only precondition failed; inspect the generated report", file=sys.stderr)
        return 1
    print("Cranelift-only precondition passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
