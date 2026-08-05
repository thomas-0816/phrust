#!/usr/bin/env python3
"""Generated-code and optional WordPress counter ratchets for hot native execution."""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from collections import Counter
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--profile",
        type=Path,
        help="optional diagnostic request profile or canonical counters JSON",
    )
    parser.add_argument(
        "--baseline-profile",
        type=Path,
        help="optional baseline counters JSON used for the 25%% code-growth ratchet",
    )
    parser.add_argument(
        "--clean-result",
        type=Path,
        help="clean WordPress result required by the breakthrough gate",
    )
    parser.add_argument(
        "--clean-baseline",
        type=Path,
        help="optional source-identical clean WordPress baseline",
    )
    parser.add_argument(
        "--breakthrough",
        action="store_true",
        help="require clean, diagnostic, lowering, correctness, latency, and RSS evidence",
    )
    return parser.parse_args()


def load_counters(path: Path) -> dict[str, Any]:
    document = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(document, dict):
        raise ValueError(f"{path}: top-level JSON value must be an object")
    native = document.get("native", document)
    if not isinstance(native, dict):
        raise ValueError(f"{path}: native counters must be an object")
    return native


def load_document(path: Path) -> dict[str, Any]:
    document = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(document, dict):
        raise ValueError(f"{path}: top-level JSON value must be an object")
    return document


def number(counters: dict[str, Any], *names: str) -> int:
    for name in names:
        value = counters.get(name)
        if isinstance(value, (int, float)) and not isinstance(value, bool):
            return int(value)
    return 0


def mapping(counters: dict[str, Any], *names: str) -> dict[str, int]:
    for name in names:
        value = counters.get(name)
        if isinstance(value, dict):
            return {
                str(key): int(count)
                for key, count in value.items()
                if isinstance(count, (int, float)) and not isinstance(count, bool)
            }
    return {}


def compile_descriptors(counters: dict[str, Any]) -> list[dict[str, Any]]:
    value = counters.get("native_compile_descriptors", [])
    if not isinstance(value, list):
        return []
    return [row for row in value if isinstance(row, dict)]


def compile_cause_attribution(descriptors: list[dict[str, Any]]) -> dict[str, Any]:
    """Build one disjoint attribution from the bounded diagnostic records."""
    causes: Counter[str] = Counter()
    seen_sources: set[str] = set()
    seen_functions: set[tuple[Any, ...]] = set()
    seen_tiers: set[tuple[Any, ...]] = set()
    seen_external: set[tuple[Any, ...]] = set()
    seen_layouts: set[tuple[Any, ...]] = set()
    seen_artifacts: set[tuple[Any, ...]] = set()
    tiers: Counter[str] = Counter()

    for descriptor in descriptors:
        source = str(descriptor.get("source_identity", ""))
        function = (
            source,
            descriptor.get("function_id"),
            str(descriptor.get("ir_fingerprint", "")),
        )
        tier = str(descriptor.get("tier", "unknown"))
        tier_key = (*function, tier)
        external = int(descriptor.get("external_signatures_hash", 0) or 0)
        external_key = (*tier_key, external)
        layout = int(descriptor.get("receiver_layout_hash", 0) or 0)
        layout_key = (*external_key, layout)
        artifact = (
            *layout_key,
            descriptor.get("target_isa"),
            descriptor.get("runtime_abi_hash"),
            descriptor.get("helper_abi_hash"),
            descriptor.get("config_hash"),
        )
        publication = str(descriptor.get("publication_result", ""))
        replan = int(descriptor.get("replan_index", 0) or 0)
        tiers[tier] += 1

        if not publication.startswith("published-"):
            cause = "failed_or_unpublished_product"
        elif replan > 0:
            cause = "exact_pre_regalloc_replan"
        elif artifact in seen_artifacts:
            cause = "duplicate_identical_compile"
        elif function not in seen_functions:
            cause = (
                "new_source_or_dynamic_unit"
                if seen_sources and source not in seen_sources
                else "unique_function"
            )
        elif tier_key not in seen_tiers:
            cause = "new_tier_variant"
        elif external_key not in seen_external:
            cause = "new_external_signature_variant"
        elif layout_key not in seen_layouts:
            cause = "new_receiver_layout_variant"
        else:
            cause = "remaining"
        causes[cause] += 1

        seen_sources.add(source)
        seen_functions.add(function)
        seen_tiers.add(tier_key)
        seen_external.add(external_key)
        seen_layouts.add(layout_key)
        seen_artifacts.add(artifact)

    ordered_causes = {
        name: causes.get(name, 0)
        for name in (
            "unique_function",
            "new_source_or_dynamic_unit",
            "new_tier_variant",
            "new_external_signature_variant",
            "new_receiver_layout_variant",
            "exact_pre_regalloc_replan",
            "duplicate_identical_compile",
            "failed_or_unpublished_product",
            "remaining",
        )
    }
    return {
        "schema": 1,
        "attempt_records": len(descriptors),
        "attribution": ordered_causes,
        "attribution_total": sum(ordered_causes.values()),
        "dimensions": {
            "unique_source_or_unit_identities": len(seen_sources),
            "unique_ir_functions": len(seen_functions),
            "tier_records": dict(sorted(tiers.items())),
            "unique_tier_variants": len(seen_tiers),
            "unique_external_signature_variants": len(seen_external),
            "unique_receiver_layout_variants": len(seen_layouts),
            "unique_artifact_identities": len(seen_artifacts),
        },
        "evidence": {
            "proven": [
                "Counts are derived only from bounded NativeCompileDescriptor records.",
                "The attribution classes are mutually exclusive and sum to attempt_records.",
                "Generic external-signature and receiver-layout components are recorded as zero.",
            ],
            "hypotheses": [],
        },
    }


def write_compile_cause_report(profile_path: Path, report: dict[str, Any]) -> None:
    output = profile_path.parent
    json_path = output / "compile-cause-report.json"
    markdown_path = output / "compile-cause-report.md"
    json_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    attribution = report["attribution"]
    dimensions = report["dimensions"]
    lines = [
        "# Native compile-cause attribution",
        "",
        f"- Attempt records: {report['attempt_records']}",
        f"- Unique IR functions: {dimensions['unique_ir_functions']}",
        f"- Unique source/unit identities: {dimensions['unique_source_or_unit_identities']}",
        "",
        "| Disjoint cause | Attempts |",
        "| --- | ---: |",
    ]
    lines.extend(f"| {name} | {count} |" for name, count in attribution.items())
    lines.extend(
        [
            "",
            "The rows are proven diagnostic-record classifications; no dominant cause is inferred.",
            "",
        ]
    )
    markdown_path.write_text("\n".join(lines), encoding="utf-8")


def require(failures: list[str], condition: bool, message: str) -> None:
    if not condition:
        failures.append(message)


def run_generated_code_fixtures(failures: list[str]) -> None:
    commands = (
        ["cargo", "test", "-q", "-p", "php_jit", "--lib", "optimizing_"],
        [
            "cargo",
            "test",
            "-q",
            "-p",
            "php_jit",
            "--lib",
            "large_php_cfg_compiles_as_one_native_function",
        ],
        [
            "cargo",
            "test",
            "-q",
            "-p",
            "php_jit",
            "--lib",
            "ordinary_optimizing_calls_need_no_pre_regalloc_replan",
        ],
        [
            "cargo",
            "test",
            "-q",
            "-p",
            "php_jit",
            "--lib",
            "optimizing_unknown_array_key_publishes_local_generic_continuation",
        ],
        [
            "cargo",
            "test",
            "-q",
            "-p",
            "php_jit",
            "--lib",
            "generated_include_caller_invokes_resolver_published_entry_and_restores_view",
        ],
        [
            "cargo",
            "test",
            "-q",
            "-p",
            "php_vm",
            "--lib",
            "common_and_exact_native_sources_cannot_import_the_rust_value_plane",
        ],
        [
            "cargo",
            "test",
            "-q",
            "-p",
            "php_vm",
            "--lib",
            "same_unit_call_resolves_on_demand_then_calls_native",
        ],
        [
            "cargo",
            "test",
            "-q",
            "-p",
            "php_vm",
            "--lib",
            "optimizing_scheduler_retains_more_than_one_pending_batch",
        ],
        [
            "cargo",
            "test",
            "-q",
            "-p",
            "php_vm",
            "--lib",
            "generic_native_key_ignores_current_external_signature_state",
        ],
        [
            "cargo",
            "test",
            "-q",
            "-p",
            "php_vm",
            "--lib",
            "server_worker_publishes_optimized_entry_after_hot_baseline_threshold",
        ],
        [
            "cargo",
            "test",
            "-q",
            "-p",
            "php_jit",
            "--lib",
            "by_reference_parameter_and_return_compile_in_both_generated_tiers",
        ],
        [
            "cargo",
            "test",
            "-q",
            "-p",
            "php_jit",
            "--lib",
            "optimizing_unstable_class_allocation_uses_generated_constructor_spine",
        ],
        [
            "cargo",
            "test",
            "-q",
            "-p",
            "php_vm",
            "--lib",
            "compiled_caller_resumes_rejected_optimizing_callee_and_continues",
        ],
        [
            "cargo",
            "test",
            "-q",
            "-p",
            "php_vm",
            "--lib",
            "compiled_caller_preserves_builtin_constants_across_callee_transition",
        ],
        [
            "cargo",
            "test",
            "-q",
            "-p",
            "php_vm",
            "--lib",
            "tiered_baseline_call_miss_cannot_publish_an_optimizing_callee",
        ],
        [
            "cargo",
            "test",
            "-q",
            "-p",
            "php_vm",
            "--lib",
            "foreground_compile_overtakes_queued_background_work",
        ],
    )
    for command in commands:
        completed = subprocess.run(command, cwd=ROOT, check=False)
        if completed.returncode != 0:
            failures.append(f"generated hot-path fixture failed: {' '.join(command)}")

    retired_warm_paths = (
        "baseline_call_dispatch.rs",
        "baseline_call_support.rs",
        "baseline_callables.rs",
        "baseline_dynamic_code.rs",
        "baseline_iterators.rs",
        "baseline_native_builtins.rs",
        "baseline_object_support.rs",
        "baseline_properties.rs",
        "baseline_reference_ownership.rs",
        "baseline_runtime_ops.rs",
        "baseline_semantic_dispatch.rs",
        "baseline_static_properties.rs",
        "baseline_value_plane.rs",
        "baseline_value_semantics.rs",
    )
    abi_root = ROOT / "crates/php_vm/src/vm/jit_abi"
    for relative in retired_warm_paths:
        require(
            failures,
            not (abi_root / relative).exists(),
            f"retired ordinary Rust executor path returned: {relative}",
        )

    deleted_warm_cache_symbols = (
        "JitNativeFunctionEntryCacheRecord",
        "function_entry_cache",
        "resolved_native_entry_address",
        "register_frame_slots",
        "register_state_slot_count",
        "emit_streaming_register_restore_loop",
        "JitNativeArrayCacheEntry",
        "JIT_NATIVE_ARRAY_CACHE_MISS",
        "array_value_caches",
        "publish_array_value_cache",
        "ensure_native_array_view",
        "invalidate_array_value_cache",
        "JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY_MATERIALIZED",
        "direct_materialized_arrays",
        "direct_array_facades",
        "native_array_views",
        "publish_native_array_view",
        "publish_direct_array_facade",
        "release_native_array_view",
        "plain_array_storage_index",
        "runtime_value_is_uniquely_owned",
        "insert_array_at_with",
        "NativeValueIdentity::Array",
        "baseline_optimizing_reentry_blocks",
        "optimizing_reentry_block_is_eligible",
    )
    sources = (
        ROOT / "crates/php_jit/src/abi.rs",
        ROOT / "crates/php_jit/src/cranelift_lowering.rs",
        ROOT / "crates/php_vm/src/vm/jit_abi.rs",
        ROOT / "crates/php_vm/src/vm/jit_abi/native/exact_runtime_ops.rs",
    )
    combined = "\n".join(path.read_text(encoding="utf-8") for path in sources)
    for symbol in deleted_warm_cache_symbols:
        require(
            failures,
            symbol not in combined,
            f"deleted request-local function-entry validation path returned: {symbol}",
        )

    emitted_contract_sources = (
        ROOT / "crates/php_jit/src/lib.rs",
        ROOT / "crates/php_jit/src/cranelift_lowering/executable_region.rs",
        ROOT / "crates/php_jit/src/cranelift_lowering/tests.rs",
    )
    emitted_contract = "\n".join(
        path.read_text(encoding="utf-8") for path in emitted_contract_sources
    )
    for symbol in (
        "JitProductionLoweringClass",
        "JitProductionLoweringMetadata",
        "production_lowering",
        "forced_generic_continuations",
        "lower_optimizing_generic_continuation",
    ):
        require(
            failures,
            symbol in emitted_contract,
            f"emitted production lowering contract disappeared: {symbol}",
        )
    require(
        failures,
        "optimizing_production_lowering_class" not in emitted_contract,
        "syntactic optimizing lowering classifier returned; manifest must record emitted code",
    )
    for test_name in (
        "optimizing_unknown_binary_add_resumes_local_generic",
        "optimizing_scalar_ssa_executes_without_local_truthy_or_lifecycle_helpers",
        "optimizing_unknown_array_key_publishes_local_generic_continuation",
    ):
        require(
            failures,
            test_name in emitted_contract,
            f"emitted-code transition proof disappeared: {test_name}",
        )

    deleted_cutover_symbols = (
        "type NativeRequestFastState = NativeExecutionContext",
        "JitNativePropertyCacheEntry",
        "object_property_caches",
        "jit_baseline_native_call_dispatch_impl",
        "phrust_baseline_native_call_dispatch",
        "phrust_native_call_dispatch",
        "phrust_baseline_native_builtin_dispatch",
        "phrust_native_semantic_dispatch",
        "phrust_baseline_native_semantic_dispatch",
        "phrust_jit_native_builtin_dispatch",
        "jit_native_builtin_dispatch_abi",
        "execute_prepared_runtime_builtin(",
        "execute_native_builtin(",
    )
    production_sources = (
        ROOT / "crates/php_jit/src/abi.rs",
        ROOT / "crates/php_jit/src/cranelift_lowering.rs",
        ROOT / "crates/php_jit/src/cranelift_lowering/executable_region.rs",
        ROOT / "crates/php_vm/src/vm/jit_abi.rs",
        ROOT / "crates/php_vm/src/vm/jit_abi/native/exact_call_dispatch.rs",
        ROOT / "crates/php_vm/src/vm/jit_abi/native/exact_runtime_ops.rs",
    )
    production = "\n".join(
        path.read_text(encoding="utf-8") for path in production_sources
    )
    for symbol in deleted_cutover_symbols:
        require(
            failures,
            symbol not in production,
            f"superseded native cutover path returned: {symbol}",
        )
    require(
        failures,
        "StableNativeArena::new" in production,
        "stable demand-backed native arenas disappeared",
    )
    optimizing_sources = "\n".join(
        path.read_text(encoding="utf-8")
        for path in (
            ROOT / "crates/php_jit/src/cranelift_lowering.rs",
            ROOT / "crates/php_jit/src/cranelift_lowering/executable_region.rs",
            ROOT / "crates/php_jit/src/cranelift_lowering/terminators.rs",
        )
    )
    require(
        failures,
        "NativeStoredValue::Php" not in optimizing_sources,
        "optimizing artifacts recovered the cold Rust Value plane",
    )


def ratchet_profile(
    failures: list[str], counters: dict[str, Any], baseline: dict[str, Any] | None
) -> None:
    helper_calls = number(counters, "runtime_helper_calls")
    reads = sum(mapping(counters, "runtime_helper_local_read_by_reason").values())
    stores = sum(mapping(counters, "runtime_helper_local_store_by_reason").values())
    truthy = sum(mapping(counters, "runtime_helper_truthy_by_value_class").values())
    retain_release = sum(mapping(counters, "runtime_helper_retain_by_reason").values()) + sum(
        mapping(counters, "runtime_helper_release_by_reason").values()
    )
    root_scans = number(counters, "runtime_helper_object_release_root_scans")
    dynamic_calls = number(counters, "call_dynamic", "native_call_dynamic")
    optimizing_transitions = sum(
        count
        for reason, count in mapping(
            counters, "transition_by_reason", "native_transition_by_reason"
        ).items()
        if reason.startswith("optimizer_")
    )
    eligible = sum(
        number(counters, name, f"native_{name}")
        for name in (
            "same_unit_direct_eligible",
            "cross_unit_direct_eligible",
            "method_monomorphic_eligible",
            "builtin_direct_eligible",
        )
    )
    executed = sum(
        number(counters, name, f"native_{name}")
        for name in (
            "same_unit_direct_executed",
            "cross_unit_direct_executed",
            "method_monomorphic_executed",
            "builtin_direct_executed",
        )
    )
    compile_attempts = number(counters, "compile_attempts", "native_compile_attempts")
    helper_nanos = number(counters, "runtime_helper_time_nanos")
    execution_nanos = number(counters, "execution_time_nanos", "native_execution_time_nanos")
    value_allocations = number(
        counters, "value_table_allocations", "native_value_table_allocations"
    )
    value_high_water = number(
        counters, "value_table_high_water", "native_value_table_high_water"
    )
    direct_rust_conversions = number(
        counters, "value_encodes", "native_value_encodes"
    ) + number(counters, "value_decodes", "native_value_decodes")
    call_transport = number(counters, "call_frame_bytes", "native_call_frame_bytes")
    helper_ids = mapping(counters, "runtime_helper_calls_by_id")
    forbidden_execution_boundaries = sum(
        helper_ids.get(name, 0)
        for name in (
            "call_function",
            "call_method",
            "call_callable",
            "call_constructor",
            "dynamic_code",
            "reference_bind",
            "call_builtin_direct",
        )
    )
    array_foreach_helpers = sum(
        helper_ids.get(name, 0)
        for name in (
            "array_new",
            "array_insert",
            "array_fetch",
            "array_unset",
            "array_spread",
            "foreach_init",
            "foreach_next",
            "foreach_cleanup",
        )
    )
    property_helpers = sum(
        helper_ids.get(name, 0)
        for name in (
            "property_fetch",
            "property_assign",
            "semantic_property",
            "semantic_static_property",
        )
    )
    scalar_local_reference_helpers = sum(
        helper_ids.get(name, 0)
        for name in (
            "unary",
            "binary",
            "compare",
            "cast",
            "local_fetch",
            "local_store",
            "reference_bind",
            "truthy",
        )
    )
    generic_prepared_builtins = helper_ids.get("call_builtin_direct", 0)
    release_helpers = sum(mapping(counters, "runtime_helper_release_by_reason").values())
    arena_resident = mapping(
        counters, "arena_resident_bytes", "native_arena_resident_bytes"
    )
    generic_entries = number(counters, "native_generic_entry_executions")
    optimizing_entries = number(counters, "native_optimizing_entry_executions")

    require(failures, helper_calls <= 100_000, f"runtime helper calls {helper_calls} exceed 100000")
    require(failures, reads + stores == 0, f"ordinary local helpers executed {reads + stores} times")
    require(failures, truthy == 0, f"ordinary truthiness helpers executed {truthy} times")
    require(
        failures,
        retain_release <= 5_000,
        f"retain/release helpers {retain_release} exceed 5000",
    )
    require(failures, root_scans <= 250, f"root scans {root_scans} exceed 250")
    require(failures, dynamic_calls <= 250, f"dynamic calls {dynamic_calls} exceed 250")
    require(
        failures,
        optimizing_transitions == 0,
        f"ordinary optimizing execution entered {optimizing_transitions} baseline continuations",
    )
    require(
        failures,
        eligible > 0 and executed * 100 >= eligible * 95,
        f"stable direct-call ratio {executed}/{eligible} is below 95%",
    )
    require(
        failures,
        optimizing_entries > 0
        and optimizing_entries * 100 >= (generic_entries + optimizing_entries) * 95,
        f"Optimizing entry ratio {optimizing_entries}/{generic_entries + optimizing_entries} is below 95%",
    )
    require(
        failures,
        forbidden_execution_boundaries == 0,
        f"retired ordinary execution boundaries ran {forbidden_execution_boundaries} times",
    )
    require(
        failures,
        value_allocations <= 10_000,
        f"value-table allocations {value_allocations} exceed 10000",
    )
    require(
        failures,
        value_high_water <= 30_000,
        f"value-table high-water {value_high_water} exceeds 30000",
    )
    require(failures, call_transport <= 1_000_000, f"call transport {call_transport} exceeds 1MB")
    require(
        failures,
        array_foreach_helpers == 0,
        f"ordinary array/foreach helpers executed {array_foreach_helpers} times",
    )
    require(
        failures,
        property_helpers == 0,
        f"ordinary property helpers executed {property_helpers} times",
    )
    require(
        failures,
        scalar_local_reference_helpers == 0,
        f"ordinary scalar/local/reference helpers executed {scalar_local_reference_helpers} times",
    )
    require(
        failures,
        generic_prepared_builtins == 0,
        f"generic prepared-builtin dispatches executed {generic_prepared_builtins} times",
    )
    require(
        failures,
        release_helpers <= 5_000,
        f"release helpers {release_helpers} exceed 5000",
    )
    require(
        failures,
        direct_rust_conversions == 0,
        f"ordinary direct/Rust value conversions executed {direct_rust_conversions} times",
    )
    require(failures, bool(arena_resident), "per-arena resident-byte counters are missing")
    require(failures, compile_attempts == 0, f"warm profile compiled {compile_attempts} functions")
    require(
        failures,
        execution_nanos > 0 and helper_nanos * 100 <= execution_nanos * 30,
        "helper-exclusive time exceeds 30% of native execution",
    )

    if baseline is not None:
        current_bytes = number(counters, "mapped_executable_bytes", "native_mapped_executable_bytes")
        baseline_bytes = number(baseline, "mapped_executable_bytes", "native_mapped_executable_bytes")
        require(failures, baseline_bytes > 0, "baseline mapped executable bytes are missing")
        require(
            failures,
            baseline_bytes > 0 and current_bytes * 100 <= baseline_bytes * 125,
            f"native code grew from {baseline_bytes} to {current_bytes} bytes (>25%)",
        )


def clean_curve(document: dict[str, Any], engine: str, concurrency: int) -> dict[str, Any]:
    engines = document.get("engines")
    if not isinstance(engines, dict):
        return {}
    selected = engines.get(engine)
    if not isinstance(selected, dict):
        return {}
    curves = selected.get("curves")
    if not isinstance(curves, list):
        return {}
    return next(
        (
            curve
            for curve in curves
            if isinstance(curve, dict) and curve.get("concurrency") == concurrency
        ),
        {},
    )


def curve_number(curve: dict[str, Any], family: str, name: str) -> float | None:
    values = curve.get(family)
    if not isinstance(values, dict):
        return None
    value = values.get(name)
    if isinstance(value, (int, float)) and not isinstance(value, bool):
        return float(value)
    return None


def source_identity(document: dict[str, Any]) -> tuple[str | None, str | None]:
    engines = document.get("engines")
    phrust = engines.get("phrust") if isinstance(engines, dict) else None
    identity = phrust.get("identity") if isinstance(phrust, dict) else None
    if not isinstance(identity, dict):
        return None, None
    commit = identity.get("source_commit")
    patch = identity.get("source_patch_sha256")
    return (
        commit if isinstance(commit, str) and commit else None,
        patch if isinstance(patch, str) and patch else None,
    )


def sample_body_hashes(curve: dict[str, Any]) -> set[str]:
    samples = curve.get("samples")
    if not isinstance(samples, list):
        return set()
    return {
        value
        for sample in samples
        if isinstance(sample, dict)
        and isinstance((value := sample.get("body_sha256")), str)
        and value
    }


def sample_values(curve: dict[str, Any], name: str) -> set[Any]:
    samples = curve.get("samples")
    if not isinstance(samples, list):
        return set()
    return {
        sample.get(name)
        for sample in samples
        if isinstance(sample, dict) and sample.get(name) is not None
    }


def find_number(document: Any, names: tuple[str, ...]) -> float | None:
    if isinstance(document, dict):
        for name in names:
            value = document.get(name)
            if isinstance(value, (int, float)) and not isinstance(value, bool):
                return float(value)
        for value in document.values():
            found = find_number(value, names)
            if found is not None:
                return found
    elif isinstance(document, list):
        for value in document:
            found = find_number(value, names)
            if found is not None:
                return found
    return None


def lowering_document(clean_path: Path, profile_path: Path) -> tuple[Path | None, dict[str, Any]]:
    candidates = (
        clean_path.parent / "lowering-coverage.json",
        profile_path.parent / "lowering-coverage.json",
    )
    for candidate in candidates:
        if candidate.is_file():
            return candidate, load_document(candidate)
    return None, {}


def ratchet_breakthrough(
    failures: list[str],
    clean_path: Path,
    profile_path: Path,
    clean: dict[str, Any],
    counters: dict[str, Any],
    baseline: dict[str, Any] | None,
) -> None:
    require(failures, clean.get("mode") == "clean", "WordPress result is not clean mode")
    require(failures, clean.get("status") == "pass", "clean WordPress result did not pass")
    require(
        failures,
        clean.get("timing_eligible") is True,
        "clean WordPress result is not timing-eligible",
    )
    commit, patch = source_identity(clean)
    require(failures, commit is not None, "clean result has no source commit identity")
    require(failures, patch is not None, "clean result has no dirty-patch identity")
    require(
        failures,
        clean_path.resolve() != profile_path.resolve(),
        "clean timing and diagnostic profile must be separate artifacts",
    )

    c1 = clean_curve(clean, "phrust", 1)
    php_c1 = clean_curve(clean, "php-fpm", 1)
    c4 = clean_curve(clean, "phrust", 4)
    c8 = clean_curve(clean, "phrust", 8)
    p50 = curve_number(c1, "latency_ms", "p50")
    p95 = curve_number(c1, "latency_ms", "p95")
    php_p50 = curve_number(php_c1, "latency_ms", "p50")
    php_p95 = curve_number(php_c1, "latency_ms", "p95")
    throughput = c1.get("requests_per_second")
    throughput = (
        float(throughput)
        if isinstance(throughput, (int, float)) and not isinstance(throughput, bool)
        else None
    )
    c1_rss = curve_number(c1, "process", "peak_rss_bytes")
    c8_rss = curve_number(c8, "process", "peak_rss_bytes")
    completed_c1 = c1.get("completed_samples")
    require(
        failures,
        isinstance(completed_c1, int) and completed_c1 >= 20,
        f"clean c1 has only {completed_c1!r} measured requests; at least 20 required",
    )
    require(failures, bool(c4), "clean c4 curve is missing")
    require(failures, bool(c8), "clean c8 curve is missing")
    require(failures, p50 is not None and p50 <= 80.0, f"clean c1 p50 {p50!r} exceeds 80 ms")
    require(failures, p95 is not None and p95 <= 100.0, f"clean c1 p95 {p95!r} exceeds 100 ms")
    require(
        failures,
        p50 is not None and p50 <= 63.069438,
        f"strategic c1 p50 {p50!r} exceeds 63.069438 ms",
    )
    require(
        failures,
        p95 is not None and p95 <= 97.587082,
        f"strategic c1 p95 {p95!r} exceeds 97.587082 ms",
    )
    require(
        failures,
        throughput is not None and throughput >= 14.8678075,
        f"strategic c1 throughput {throughput!r} is below 14.8678075 req/s",
    )
    require(
        failures,
        p50 is not None and php_p50 is not None and php_p50 > 0 and p50 / php_p50 <= 2.0,
        f"simultaneous p50 ratio is not <=2.0 ({p50!r}/{php_p50!r})",
    )
    require(
        failures,
        p95 is not None and php_p95 is not None and php_p95 > 0 and p95 / php_p95 <= 2.0,
        f"simultaneous p95 ratio is not <=2.0 ({p95!r}/{php_p95!r})",
    )
    require(
        failures,
        c1_rss is not None and c1_rss <= 300 * 1024 * 1024,
        f"clean c1 peak RSS {c1_rss!r} exceeds 300 MiB",
    )
    require(
        failures,
        c8_rss is not None and c8_rss <= 500 * 1024 * 1024,
        f"clean c8 peak RSS {c8_rss!r} exceeds 500 MiB",
    )
    phrust_hashes = sample_body_hashes(c1)
    php_hashes = sample_body_hashes(php_c1)
    require(
        failures,
        len(phrust_hashes) == 1 and phrust_hashes == php_hashes,
        "clean Phrust/PHP response-body hashes are missing or different",
    )
    require(
        failures,
        phrust_hashes == {"7a34e150c5304aea4744a0e4f3b4fd70c4309cca411902513478a9ba7a196072"},
        f"clean WordPress body hash is not the fixed acceptance hash: {phrust_hashes}",
    )
    require(
        failures,
        sample_values(c1, "body_bytes") == {70_949}
        and sample_values(php_c1, "body_bytes") == {70_949},
        "clean WordPress body size is not exactly 70,949 bytes for both engines",
    )
    require(
        failures,
        sample_values(c1, "status") == {200}
        and sample_values(php_c1, "status") == {200},
        "clean WordPress samples are not all HTTP 200",
    )
    correctness = clean.get("correctness")
    correctness_failures = correctness.get("failures") if isinstance(correctness, dict) else None
    require(
        failures,
        isinstance(correctness_failures, list) and not correctness_failures,
        "clean result contains correctness failures",
    )

    if baseline is not None:
        old_c1 = clean_curve(baseline, "phrust", 1)
        old_p50 = curve_number(old_c1, "latency_ms", "p50")
        require(
            failures,
            p50 is not None and old_p50 is not None and p50 < old_p50,
            f"clean c1 p50 did not improve over baseline ({old_p50!r} -> {p50!r})",
        )
    else:
        require(
            failures,
            p50 is not None and p50 < 443.62,
            f"clean c1 p50 did not improve over historical 443.62 ms ({p50!r})",
        )

    lowering_path, lowering = lowering_document(clean_path, profile_path)
    require(failures, lowering_path is not None, "lowering-coverage.json is missing")
    transition_count = find_number(
        lowering,
        (
            "local_generic_continuation_executions",
        ),
    )
    require(
        failures,
        transition_count is not None and transition_count == 0,
        f"executed lowering evidence contains {transition_count} local Generic continuations",
    )
    generic_share = find_number(
        lowering,
        ("generic_hot_time_share_pct",),
    )
    require(
        failures,
        generic_share is not None and generic_share <= 5.0,
        f"Generic inclusive hot-time share {generic_share!r} exceeds 5%",
    )
    optimizing_entries = find_number(
        lowering,
        ("optimizing_entry_executions", "actual_optimizing_entry_executions"),
    )
    require(
        failures,
        optimizing_entries is not None and optimizing_entries > 0,
        "lowering evidence has no actual optimizing entry executions",
    )
    require(
        failures,
        number(counters, "value_encodes", "native_value_encodes")
        + number(counters, "value_decodes", "native_value_decodes")
        == 0,
        "diagnostic ordinary direct/Rust conversion count is nonzero",
    )


def main() -> int:
    if os.environ.get("PHRUST_NATIVE_CUTOVER_ACCEPTANCE") != "1":
        print(
            "BLOCKED: native-hotpath-ratchet is final acceptance work; "
            "do not run it during the active native cutover. "
            "PHRUST_NATIVE_CUTOVER_ACCEPTANCE=1 is reserved for the explicit "
            "post-cutover acceptance run.",
            file=sys.stderr,
        )
        return 2
    args = parse_args()
    failures: list[str] = []
    run_generated_code_fixtures(failures)

    if args.breakthrough:
        require(
            failures,
            args.clean_result is not None,
            "--breakthrough requires --clean-result",
        )
        require(
            failures,
            args.profile is not None,
            "--breakthrough requires --profile",
        )

    if args.profile is not None:
        try:
            counters = load_counters(args.profile)
            baseline = (
                load_counters(args.baseline_profile)
                if args.baseline_profile is not None
                else None
            )
            ratchet_profile(failures, counters, baseline)
            descriptors = compile_descriptors(counters)
            attribution = compile_cause_attribution(descriptors)
            if descriptors:
                write_compile_cause_report(args.profile, attribution)
            if args.breakthrough:
                require(
                    failures,
                    bool(descriptors),
                    "diagnostic profile has no bounded native compile descriptors",
                )
                require(
                    failures,
                    attribution["attribution_total"] == len(descriptors),
                    "compile-cause attribution is not disjoint and complete",
                )
                generic_descriptors = [
                    descriptor
                    for descriptor in descriptors
                    if descriptor.get("tier") == "generic"
                ]
                require(
                    failures,
                    all(
                        int(descriptor.get("external_signatures_hash", 0) or 0) == 0
                        and int(descriptor.get("receiver_layout_hash", 0) or 0) == 0
                        for descriptor in generic_descriptors
                    ),
                    "Generic compile descriptors contain external-signature or receiver-layout state",
                )
            if args.breakthrough and args.clean_result is not None:
                clean = load_document(args.clean_result)
                clean_baseline = (
                    load_document(args.clean_baseline)
                    if args.clean_baseline is not None
                    else None
                )
                ratchet_breakthrough(
                    failures,
                    args.clean_result,
                    args.profile,
                    clean,
                    counters,
                    clean_baseline,
                )
        except (OSError, ValueError, json.JSONDecodeError) as error:
            failures.append(str(error))

    if failures:
        for failure in failures:
            print(f"[fail] {failure}", file=sys.stderr)
        return 1
    suffix = (
        " with breakthrough evidence"
        if args.breakthrough
        else " with runtime counters"
        if args.profile is not None
        else ""
    )
    print(f"[pass] native hot-path generated-code ratchet{suffix}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
