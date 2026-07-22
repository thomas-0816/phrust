#!/usr/bin/env python3
"""Build the canonical Prompt-Pack B hot-native evidence bundle."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--before", type=Path, required=True)
    parser.add_argument("--after", type=Path, required=True)
    parser.add_argument("--clean", type=Path, required=True)
    parser.add_argument("--baseline-clean", type=Path)
    parser.add_argument("--out-dir", type=Path, required=True)
    parser.add_argument("--base-commit", default="be91339047d931d4c364d4ce6a16ddbd9786be96")
    parser.add_argument("--runtime-abi-before", type=int, default=20)
    parser.add_argument("--runtime-abi-after", type=int, default=73)
    return parser.parse_args()


def load(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"{path}: expected a JSON object")
    return value


def native(document: dict[str, Any]) -> dict[str, Any]:
    value = document.get("native")
    if value is None:
        profile = document.get("profile")
        if isinstance(profile, dict):
            value = profile.get("native")
    if value is None:
        value = document
    if not isinstance(value, dict):
        raise ValueError("native counters must be an object")
    return value


def number(counters: dict[str, Any], *names: str) -> int:
    for name in names:
        value = counters.get(name)
        if isinstance(value, (int, float)) and not isinstance(value, bool):
            return int(value)
    return 0


def counter_map(counters: dict[str, Any], *names: str) -> dict[str, int]:
    for name in names:
        value = counters.get(name)
        if isinstance(value, dict):
            return {
                str(key): int(count)
                for key, count in value.items()
                if isinstance(count, (int, float)) and not isinstance(count, bool)
            }
    return {}


def write_json(path: Path, value: Any) -> None:
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def delta_map(before: dict[str, int], after: dict[str, int]) -> dict[str, dict[str, int]]:
    return {
        key: {
            "before": before.get(key, 0),
            "after": after.get(key, 0),
            "delta": after.get(key, 0) - before.get(key, 0),
        }
        for key in sorted(before.keys() | after.keys())
    }


def sum_map(counters: dict[str, Any], name: str) -> int:
    return sum(counter_map(counters, name).values())


def direct_ratio(counters: dict[str, Any]) -> tuple[int, int, float]:
    eligible = sum(
        number(counters, name)
        for name in (
            "same_unit_direct_eligible",
            "cross_unit_direct_eligible",
            "method_monomorphic_eligible",
            "builtin_direct_eligible",
        )
    )
    executed = sum(
        number(counters, name)
        for name in (
            "same_unit_direct_executed",
            "cross_unit_direct_executed",
            "method_monomorphic_executed",
            "builtin_direct_executed",
        )
    )
    ratio = executed / eligible if eligible else 0.0
    return eligible, executed, ratio


def production_lowering(counters: dict[str, Any]) -> list[dict[str, Any]]:
    rows = []
    for site, operation_local_transition in counter_map(
        counters,
        "production_lowering_by_site",
        "native_production_lowering_by_site",
    ).items():
        parts = site.split("|", 5)
        if len(parts) != 6:
            continue
        unit, region, function, continuation, operation, lowering_class = parts
        rows.append(
            {
                "unit": unit,
                "region": region,
                "function": int(function),
                "continuation": int(continuation),
                "operation": operation,
                "class": lowering_class,
                "operation_local_transition": operation_local_transition != 0,
            }
        )
    return sorted(
        rows,
        key=lambda row: (
            row["unit"],
            row["region"],
            row["function"],
            row["continuation"],
        ),
    )


def curve(clean: dict[str, Any], engine: str, concurrency: int) -> dict[str, Any] | None:
    engines = clean.get("engines")
    if not isinstance(engines, dict) or not isinstance(engines.get(engine), dict):
        return None
    curves = engines[engine].get("curves", [])
    if not isinstance(curves, list):
        return None
    return next(
        (
            item
            for item in curves
            if isinstance(item, dict) and item.get("concurrency") == concurrency
        ),
        None,
    )


def p50(curve_value: dict[str, Any] | None) -> float:
    if curve_value is None or not isinstance(curve_value.get("latency_ms"), dict):
        return 0.0
    value = curve_value["latency_ms"].get("p50", 0.0)
    return float(value) if isinstance(value, (int, float)) else 0.0


def p95(curve_value: dict[str, Any] | None) -> float:
    if curve_value is None or not isinstance(curve_value.get("latency_ms"), dict):
        return 0.0
    value = curve_value["latency_ms"].get("p95", 0.0)
    return float(value) if isinstance(value, (int, float)) else 0.0


def throughput(curve_value: dict[str, Any] | None) -> float:
    if curve_value is None:
        return 0.0
    value = curve_value.get("requests_per_second", 0.0)
    return float(value) if isinstance(value, (int, float)) else 0.0


def main() -> int:
    args = parse_args()
    before_document = load(args.before)
    after_document = load(args.after)
    clean_document = load(args.clean)
    baseline_clean_document = load(args.baseline_clean) if args.baseline_clean else None
    before = native(before_document)
    after = native(after_document)
    args.out_dir.mkdir(parents=True, exist_ok=True)

    write_json(args.out_dir / "before.json", before_document)
    write_json(args.out_dir / "after.json", after_document)
    write_json(args.out_dir / "clean-c1-c4-c8.json", clean_document)

    lowering_rows = production_lowering(after)
    transition_by_reason = counter_map(
        after, "transition_by_reason", "native_transition_by_reason"
    )
    transition_time_by_reason = counter_map(
        after,
        "transition_time_nanos_by_reason",
        "native_transition_time_nanos_by_reason",
    )
    operation_transition_executions = sum(
        count for reason, count in transition_by_reason.items() if reason.startswith("optimizer_")
    )
    operation_transition_time = sum(
        nanos
        for reason, nanos in transition_time_by_reason.items()
        if reason.startswith("optimizer_")
    )
    execution_time = number(after, "execution_time_nanos", "native_execution_time_nanos")
    optimizing_entry_executions = number(
        after, "optimizing_entry_executions", "native_optimizing_entry_executions"
    )
    baseline_hot_time_share_pct = (
        0.0
        if execution_time == 0
        else 100.0
        if optimizing_entry_executions == 0
        else operation_transition_time * 100.0 / execution_time
    )
    write_json(
        args.out_dir / "lowering-coverage.json",
        {
            "schema_version": 1,
            "production_lowering": lowering_rows,
            "operation_local_transition_metadata_count": sum(
                1 for row in lowering_rows if row["operation_local_transition"]
            ),
            "operation_local_transition_executions": operation_transition_executions,
            "baseline_hot_time_share_pct": baseline_hot_time_share_pct,
            "baseline_entry_executions": number(
                after, "baseline_entry_executions", "native_baseline_entry_executions"
            ),
            "optimizing_entry_executions": optimizing_entry_executions,
        },
    )
    write_json(
        args.out_dir / "transition-by-operation.json",
        {
            "schema_version": 1,
            "count_by_reason": transition_by_reason,
            "time_nanos_by_reason": transition_time_by_reason,
            "operation_local_transition_executions": operation_transition_executions,
            "operation_local_transition_time_nanos": operation_transition_time,
        },
    )
    write_json(
        args.out_dir / "value-plane.json",
        {
            "schema_version": 1,
            "direct_to_rust_conversions": number(
                after, "value_decodes", "native_value_decodes"
            ),
            "rust_to_direct_conversions": number(
                after, "value_encodes", "native_value_encodes"
            ),
            "old_value_table_allocations": number(
                after, "value_table_allocations", "native_value_table_allocations"
            ),
            "old_value_table_high_water": number(
                after, "value_table_high_water", "native_value_table_high_water"
            ),
            "materializations_by_kind_and_origin": counter_map(
                after,
                "value_table_materializations_by_kind_and_origin",
                "native_value_table_materializations_by_kind_and_origin",
            ),
            "arena_high_water_bytes": counter_map(
                after, "arena_high_water_bytes", "native_arena_high_water_bytes"
            ),
            "arena_reused_bytes": counter_map(
                after, "arena_reused_bytes", "native_arena_reused_bytes"
            ),
        },
    )
    write_json(
        args.out_dir / "memory-map.json",
        {
            "schema_version": 1,
            "arena_reserved_bytes": counter_map(
                after, "arena_reserved_bytes", "native_arena_reserved_bytes"
            ),
            "arena_high_water_bytes": counter_map(
                after, "arena_high_water_bytes", "native_arena_high_water_bytes"
            ),
            "arena_resident_bytes": counter_map(
                after, "arena_resident_bytes", "native_arena_resident_bytes"
            ),
            "arena_reused_bytes": counter_map(
                after, "arena_reused_bytes", "native_arena_reused_bytes"
            ),
            "arena_reset_bytes": counter_map(
                after, "arena_reset_bytes", "native_arena_reset_bytes"
            ),
        },
    )
    write_json(
        args.out_dir / "code-size.json",
        {
            "schema_version": 1,
            "mapped_executable_bytes": number(
                after, "mapped_executable_bytes", "native_mapped_executable_bytes"
            ),
            "code_bytes_by_function": counter_map(
                after, "code_bytes_by_function", "native_code_bytes_by_function"
            ),
            "code_bytes_by_unit": counter_map(
                after, "code_bytes_by_unit", "native_code_bytes_by_unit"
            ),
        },
    )

    before_helpers = counter_map(before, "runtime_helper_calls_by_id")
    after_helpers = counter_map(after, "runtime_helper_calls_by_id")
    write_json(
        args.out_dir / "helper-delta.json",
        {
            "schema_version": 1,
            "total": {
                "before": number(before, "runtime_helper_calls"),
                "after": number(after, "runtime_helper_calls"),
                "delta": number(after, "runtime_helper_calls")
                - number(before, "runtime_helper_calls"),
            },
            "by_id": delta_map(before_helpers, after_helpers),
            "exclusive_time_nanos_by_id": delta_map(
                counter_map(before, "runtime_helper_time_nanos_by_id"),
                counter_map(after, "runtime_helper_time_nanos_by_id"),
            ),
        },
    )

    eligible, executed, ratio = direct_ratio(after)
    write_json(
        args.out_dir / "call-linkage.json",
        {
            "schema_version": 1,
            "direct": number(after, "call_direct", "native_call_direct"),
            "dynamic": number(after, "call_dynamic", "native_call_dynamic"),
            "stable_eligible": eligible,
            "stable_executed": executed,
            "stable_direct_ratio": ratio,
            "dynamic_by_reason": counter_map(after, "call_dynamic_by_reason"),
            "dynamic_by_target": counter_map(after, "call_dynamic_by_target"),
        },
    )
    write_json(
        args.out_dir / "ownership.json",
        {
            "schema_version": 1,
            "retain_by_reason": counter_map(after, "runtime_helper_retain_by_reason"),
            "release_by_reason": counter_map(after, "runtime_helper_release_by_reason"),
            "retain_release_total": sum_map(after, "runtime_helper_retain_by_reason")
            + sum_map(after, "runtime_helper_release_by_reason"),
            "moves": number(after, "ownership_moves", "native_ownership_moves"),
            "clones": number(after, "ownership_clones", "native_ownership_clones"),
            "escapes": number(after, "ownership_escapes", "native_ownership_escapes"),
        },
    )
    write_json(
        args.out_dir / "root-index.json",
        {
            "schema_version": 1,
            "release_fast_paths": number(after, "runtime_helper_object_release_fast_paths"),
            "release_root_scans": number(after, "runtime_helper_object_release_root_scans"),
            "by_reason": counter_map(after, "runtime_helper_object_release_root_scans_by_reason"),
        },
    )
    write_json(
        args.out_dir / "optimized-fragments.json",
        {
            "schema_version": 1,
            "ssa_promoted_locals": number(after, "ssa_promoted_locals", "native_ssa_promoted_locals"),
            "ssa_promoted_registers": number(
                after, "ssa_promoted_registers", "native_ssa_promoted_registers"
            ),
            "versions_published": number(after, "versions_published", "native_version_published"),
            "transition_count": number(after, "transition_count", "native_transition_count"),
            "transition_by_reason": counter_map(after, "transition_by_reason"),
            "code_bytes_by_function": counter_map(after, "code_bytes_by_function"),
            "code_bytes_by_unit": counter_map(after, "code_bytes_by_unit"),
            "mapped_executable_bytes": number(
                after, "mapped_executable_bytes", "native_mapped_executable_bytes"
            ),
            "inlined_calls": number(after, "inlined_calls", "native_inlined_calls"),
            "inline_calls_removed": number(
                after, "inline_calls_removed", "native_inline_calls_removed"
            ),
            "inline_bytes_added": number(after, "inline_bytes_added", "native_inline_bytes_added"),
            "inline_rejected_by_reason": counter_map(after, "inline_rejected_by_reason"),
            "publication_contract": {
                "threshold_entries": 8,
                "background_compile": True,
                "foreground_priority": True,
                "atomic_indirection_cell": True,
                "prewarm_before_readiness": True,
                "maximum_versions_per_function": 2,
            },
        },
    )
    write_json(
        args.out_dir / "merge-contract.json",
        {
            "base_commit": args.base_commit,
            "runtime_abi_before": args.runtime_abi_before,
            "runtime_abi_after": args.runtime_abi_after,
            "new_value_tags": [
                "runtime_float",
                "runtime_string",
                "runtime_array",
                "runtime_object",
                "runtime_reference",
                "runtime_callable",
                "runtime_resource",
                "runtime_generator",
                "runtime_fiber",
            ],
            "new_helper_ids": [48, 49],
            "fragment_abi_assumptions": [
                "optimized fragments are bounded to 128 IR instructions",
                "live values cross fragment boundaries through the existing frame ABI",
                "guard failures transition to native baseline continuations or typed slow paths",
                "string and array lengths use versioned stable descriptors, never Rust offsets",
            ],
            "shared_files_changed": [
                "crates/php_jit/src/cranelift_lowering.rs",
                "crates/php_jit/src/abi.rs",
                "crates/php_jit/src/helpers.rs",
                "crates/php_jit/src/region_ir",
                "crates/php_vm/src/vm/jit_abi.rs",
            ],
            "fast_baseline_rebase_actions": [
                "preserve runtime ABI 24 and helper IDs 48-49",
                "retain bounded optimizing fragment publication slots",
            ],
        },
    )

    helper_calls = number(after, "runtime_helper_calls")
    reads = sum_map(after, "runtime_helper_local_read_by_reason")
    stores = sum_map(after, "runtime_helper_local_store_by_reason")
    roots = number(after, "runtime_helper_object_release_root_scans")
    dynamic = number(after, "call_dynamic", "native_call_dynamic")
    helper_ids = counter_map(after, "runtime_helper_calls_by_id")
    array_foreach = sum(
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
    properties = sum(
        helper_ids.get(name, 0)
        for name in (
            "property_fetch",
            "property_assign",
            "semantic_property",
            "semantic_static_property",
        )
    )
    scalar_local_reference = sum(
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
    generic_builtins = helper_ids.get("call_builtin_direct", 0)
    releases = sum_map(after, "runtime_helper_release_by_reason")
    value_allocations = number(
        after, "value_table_allocations", "native_value_table_allocations"
    )
    helper_nanos = number(after, "runtime_helper_time_nanos")
    execution_nanos = number(after, "execution_time_nanos", "native_execution_time_nanos")
    helper_share = helper_nanos / execution_nanos if execution_nanos else 0.0
    rows = (
        ("runtime helper calls", helper_calls, 100_000, helper_calls <= 100_000),
        ("local reads + stores", reads + stores, 20_000, reads + stores <= 20_000),
        ("array + foreach helpers", array_foreach, 20_000, array_foreach <= 20_000),
        ("property helpers", properties, 5_000, properties <= 5_000),
        (
            "scalar + local + reference helpers",
            scalar_local_reference,
            20_000,
            scalar_local_reference <= 20_000,
        ),
        ("generic prepared builtins", generic_builtins, 5_000, generic_builtins <= 5_000),
        ("release helpers", releases, 5_000, releases <= 5_000),
        ("old value allocations", value_allocations, 10_000, value_allocations <= 10_000),
        ("root scans", roots, 250, roots <= 250),
        ("dynamic calls", dynamic, 250, dynamic <= 250),
    )
    lines = [
        "# Hot Native Execution Report",
        "",
        "## Structural acceptance",
        "",
        "| Metric | After | Target | Status |",
        "| --- | ---: | ---: | --- |",
    ]
    lines.extend(
        f"| {name} | {value} | {target} | {'pass' if passed else 'fail'} |"
        for name, value, target, passed in rows
    )
    lines.extend(
        [
            f"| stable direct ratio | {ratio:.3%} | 95.000% | {'pass' if ratio >= .95 else 'fail'} |",
            f"| helper-exclusive CPU share | {helper_share:.3%} | 30.000% | {'pass' if execution_nanos and helper_share <= .3 else 'fail'} |",
            "",
            "## Clean timing",
            "",
        ]
    )
    for concurrency in (1, 4, 8):
        phrust = p50(curve(clean_document, "phrust", concurrency))
        php = p50(curve(clean_document, "php-fpm", concurrency))
        ratio_to_php = phrust / php if php else 0.0
        lines.append(
            f"- c{concurrency}: Phrust p50 {phrust:.3f} ms; PHP p50 {php:.3f} ms; ratio {ratio_to_php:.3f}x"
        )
    if baseline_clean_document is not None:
        baseline_c1 = curve(baseline_clean_document, "phrust", 1)
        current_c1 = curve(clean_document, "phrust", 1)
        baseline_c8 = curve(baseline_clean_document, "phrust", 8)
        current_c8 = curve(clean_document, "phrust", 8)
        baseline_c1_p50 = p50(baseline_c1)
        current_c1_p50 = p50(current_c1)
        c1_improvement = (
            (baseline_c1_p50 - current_c1_p50) / baseline_c1_p50
            if baseline_c1_p50
            else 0.0
        )
        baseline_c8_throughput = throughput(baseline_c8)
        current_c8_throughput = throughput(current_c8)
        c8_improvement = (
            (current_c8_throughput - baseline_c8_throughput) / baseline_c8_throughput
            if baseline_c8_throughput
            else 0.0
        )
        p95_regressions = []
        for concurrency in (1, 4, 8):
            baseline_p95 = p95(curve(baseline_clean_document, "phrust", concurrency))
            current_p95 = p95(curve(clean_document, "phrust", concurrency))
            if baseline_p95 and current_p95 > baseline_p95:
                p95_regressions.append(f"c{concurrency}")
        baseline_status = baseline_clean_document.get("status", "unknown")
        baseline_failures = (
            (baseline_clean_document.get("correctness") or {}).get("failures") or []
        )
        lines.extend(
            [
                "",
                "## Branch-parent comparison",
                "",
                f"- c1 p50 improvement: {c1_improvement:.3%} (target 50%; {'pass' if c1_improvement >= .5 else 'fail'})",
                f"- c8 throughput improvement: {c8_improvement:.3%} (target 50%; {'pass' if c8_improvement >= .5 else 'fail'})",
                f"- p95 regressions: {', '.join(p95_regressions) if p95_regressions else 'none'}",
                f"- parent artifact status: `{baseline_status}` with {len(baseline_failures)} correctness failure(s)",
            ]
        )
    helper_times = counter_map(after, "runtime_helper_time_nanos_by_id")
    dominant = sorted(helper_times.items(), key=lambda item: item[1], reverse=True)[:5]
    lines.extend(
        [
            "",
            "## Remaining blockers",
            "",
            "- Threshold optimization publishes in the background for non-prewarmed server entries; benchmark entries are optimized before readiness.",
            "- Exact-parent native code growth cannot be computed because the B0 profile predates code-byte attribution.",
            "- Largest helper-time families: "
            + ", ".join(f"{name}={nanos / 1_000_000:.1f} ms" for name, nanos in dominant),
            "",
            "Diagnostic counters are instrumented and are not used as clean latency samples.",
            "",
        ]
    )
    (args.out_dir / "summary.md").write_text("\n".join(lines), encoding="utf-8")
    print(f"[ok] wrote hot-native report bundle to {args.out_dir}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
