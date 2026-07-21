#!/usr/bin/env python3
"""Source-level architecture ratchet for the streaming Cranelift baseline."""

from __future__ import annotations

import argparse
from pathlib import Path
import subprocess
import sys


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--report")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    read = lambda path: (root / path).read_text(encoding="utf-8")
    lowering = read("crates/php_jit/src/cranelift_lowering/executable_region.rs")
    modes = read("crates/php_jit/src/cranelift_lowering/baseline_streaming.rs")
    linkage = read("crates/php_jit/src/cranelift_lowering/native_linkage.rs")
    manager = read("crates/php_jit/src/code_manager.rs")
    cache = read("crates/php_vm/src/vm/mod.rs")
    layout = read("crates/php_jit/src/cranelift_lowering/module_layout.rs")
    native_cache = read("crates/php_jit/src/native_cache.rs")
    resolver = read("crates/php_vm/src/vm/jit_abi/dynamic_units.rs")
    errors = []
    requirements = [
        ("StreamingBaselineCompiler", modes, "separate streaming baseline compiler"),
        ("SsaOptimizingCompiler", modes, "separate optimizing compiler interface"),
        ("fn compile_fragment", modes, "real fragment compiler entry point"),
        (
            "streaming-baseline-v7-borrowed-locals",
            modes + linkage,
            "versioned compiler mode",
        ),
        ("NativeFragmentFrameLayout", lowering, "compact fragment frame layout"),
        ("register_slots", lowering, "sparse register slots"),
        ("NativeRegisterStorage::Transient", lowering, "non-materialized baseline temporaries"),
        ("maximum_temporary_cache_entries", lowering, "bounded temporary-cache telemetry"),
        ("loads_per_source_instruction_milli", lowering, "baseline frame-load telemetry"),
        ("stores_per_source_instruction_milli", lowering, "baseline frame-store telemetry"),
        ("active_cost_tokens", manager, "cost-token scheduler"),
        ("queued_by_priority", manager, "priority compile queues"),
        ("compile_once_with_scratch_admission", manager, "cost-aware admission boundary"),
        ("stable_function_ir_fingerprint", cache, "function-oriented persistent identity"),
        ("cost_aware_fragment_blocks", layout, "cost-minimizing fragment partition"),
        ("fragment_boundary_cost", layout, "fragment frame-traffic objective"),
        ("refine_fragment", layout, "deterministic exact-metric fragment refinement"),
        ("preflight_only", lowering, "pre-regalloc CLIF preflight"),
        ("MAX_PRE_REGALLOC_REPLAN_ATTEMPTS", lowering, "bounded replan attempts"),
        ("pre_regalloc_replans", lowering, "replan telemetry"),
        ("planning_register_live_in", layout, "planner register liveness"),
        ("NATIVE_FRAGMENT_PLAN_SCHEMA_VERSION", layout + cache, "versioned fragment-plan identity"),
        ("resolve_native_function", resolver, "persistent function-on-demand resolver"),
        ("get_or_load_native_unit", cache, "restart-persistent function publication"),
        ("reserve_directory_bytes", native_cache, "constant-time cache size admission"),
    ]
    for needle, haystack, description in requirements:
        if needle not in haystack:
            errors.append(f"missing {description}: {needle}")
    for needle, description in [
        ("whole_unit_function_order", "whole-unit compile breadth"),
        ("instruction_blocks", "instruction-per-block lowering"),
    ]:
        if needle in lowering:
            errors.append(f"found forbidden {description}: {needle}")
    if "manager.state.lock()" in lowering:
        errors.append("lowering acquires the global code-manager state lock")
    write_atomic = native_cache.split("fn write_atomic", 1)[-1].split(
        "pub struct NativeLoadedArtifact", 1
    )[0]
    if "sync_all" in write_atomic or "read_dir" in write_atomic:
        errors.append("cache write hot path performs a durable flush or directory scan")

    cfg_command = [sys.executable, str(root / "scripts/verify/native_baseline_cfg_ratchet.py")]
    if args.report:
        cfg_command.extend(["--report", args.report])
    if subprocess.run(cfg_command, cwd=root, check=False).returncode != 0:
        errors.append("compiled CFG ratchet failed")
    if errors:
        for error in errors:
            print(f"native fast baseline ratchet: {error}", file=sys.stderr)
        return 1
    print("native fast baseline ratchet: pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
