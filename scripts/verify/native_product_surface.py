#!/usr/bin/env python3
"""Verify the single mandatory-native product surface and telemetry contract."""

from __future__ import annotations

import os
import re
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
TARGET = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
if not TARGET.is_absolute():
    TARGET = ROOT / TARGET

PRODUCT_DOCS = (
    "AGENTS.md",
    "README.md",
    "docs/adr/0017-native-execution-architecture.md",
    "docs/foundation/engine-defaults.md",
    "docs/performance/README.md",
    "docs/performance/methodology.md",
    "docs/performance/counter-families.md",
    "docs/performance/ci-policy.md",
    "docs/contributor/wordpress-smoke.md",
)
PRODUCT_SOURCES = (
    "crates/php_vm_cli/src/commands.rs",
    "crates/php_server/src/config.rs",
    "crates/php_server/src/server.rs",
    "crates/php_server/src/metrics.rs",
    "crates/php_executor/src/profile.rs",
)
RETIRED = (
    "--" + "jit",
    "--exec" + "-format",
    "--quick" + "ening",
    "--super" + "instructions",
    "--den" + "se-",
    "experimental" + "-jit",
    "engine-preset=" + "fast",
)
TELEMETRY_FAMILIES = (
    "native_compile",
    "native_cache",
    "native_execution",
    "native_region",
    "native_call",
    "native_version",
    "native_transition",
    "native_ssa",
    "native_ownership",
    "native_value_table",
    "native_slow_path",
    "runtime_helper",
    "gc_safepoint",
)
LINKAGE_FOOTPRINT_COUNTERS = frozenset(
    {
        "native_artifact_header_padding_bytes",
        "native_builtin_direct_eligible",
        "native_builtin_direct_executed",
        "native_code_bytes_by_function",
        "native_code_bytes_by_unit",
        "native_cross_unit_direct_eligible",
        "native_cross_unit_direct_executed",
        "native_duplicate_function_body_count",
        "native_frame_arena_capacity_bytes",
        "native_frame_arena_high_water_bytes",
        "native_function_body_compile_count",
        "native_inline_bytes_added",
        "native_inline_calls_removed",
        "native_inline_rejected_by_reason",
        "native_inlined_calls",
        "native_loaded_artifact_maps",
        "native_loaded_artifact_registry_hits",
        "native_loaded_entry_table_constructions",
        "native_mapped_executable_bytes",
        "native_metadata_bytes",
        "native_method_monomorphic_eligible",
        "native_method_monomorphic_executed",
        "native_relocation_bytes",
        "native_rodata_bytes",
        "native_same_unit_direct_eligible",
        "native_same_unit_direct_executed",
        "native_stack_bytes_by_function",
        "native_tail_calls",
        "native_worker_stack_committed_bytes",
        "native_worker_stack_virtual_bytes",
    }
)
HOTPATH_DIAGNOSTIC_COUNTERS = frozenset(
    {
        "native_builtin_calls_by_name",
        "native_builtin_time_nanos_by_name",
        "native_value_decodes",
        "native_value_encodes",
    }
)


def run_help(binary: Path) -> tuple[int, str]:
    completed = subprocess.run(
        [str(binary), "--help"], cwd=ROOT, text=True, capture_output=True, check=False
    )
    return completed.returncode, completed.stdout + completed.stderr


def scan(paths: tuple[str, ...], failures: list[str]) -> None:
    for relative in paths:
        path = ROOT / relative
        if not path.is_file():
            failures.append(f"missing product contract file: {relative}")
            continue
        text = path.read_text(encoding="utf-8")
        for retired in RETIRED:
            if retired in text:
                failures.append(f"{relative} exposes retired product surface {retired!r}")


def verify_counters(failures: list[str]) -> None:
    path = ROOT / "crates/php_vm/src/counters.rs"
    text = path.read_text(encoding="utf-8")
    body_match = re.search(r"pub struct VmCounters \{(?P<body>.*?)\n\}", text, re.S)
    if body_match is None:
        failures.append("VmCounters declaration is missing")
        return
    fields = re.findall(r"pub ([A-Za-z0-9_]+):", body_match.group("body"))
    unexpected = sorted(
        field
        for field in fields
        if not field.startswith(TELEMETRY_FAMILIES)
        and field not in LINKAGE_FOOTPRINT_COUNTERS
        and field not in HOTPATH_DIAGNOSTIC_COUNTERS
    )
    if unexpected:
        failures.append("non-canonical VmCounters fields: " + ", ".join(unexpected))
    for family in TELEMETRY_FAMILIES:
        if not any(field.startswith(family) for field in fields):
            failures.append(f"VmCounters is missing telemetry family {family}")


def main() -> int:
    failures: list[str] = []
    scan(PRODUCT_DOCS, failures)
    scan(PRODUCT_SOURCES, failures)

    for name in ("php-vm", "phrust-server"):
        binary = TARGET / "debug" / name
        if not binary.is_file():
            failures.append(f"product binary is missing: {binary}")
            continue
        status, help_text = run_help(binary)
        if status != 0:
            failures.append(f"{name} --help exited with {status}")
        for retired in RETIRED:
            if retired in help_text:
                failures.append(f"{name} --help exposes {retired!r}")
        if "baseline" not in help_text or "default" not in help_text:
            failures.append(f"{name} --help does not expose baseline/default presets")
        if "native-cache" not in help_text:
            failures.append(f"{name} --help does not expose native cache policy")

    verify_counters(failures)
    startup = (ROOT / "crates/php_server/src/server.rs").read_text(encoding="utf-8")
    for field in (
        "compiler_version",
        "runtime_abi",
        "helper_abi",
        "target",
        "cpu_features",
        "cache_mode",
        "cache_path",
        "preset",
        "artifacts_loaded",
        "artifacts_compiled",
    ):
        if field not in startup:
            failures.append(f"startup identity is missing {field}")

    if failures:
        print("native product surface gate failed:", file=sys.stderr)
        print("\n".join(f"- {failure}" for failure in failures), file=sys.stderr)
        return 1
    print("native product surface gate passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
