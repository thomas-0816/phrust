#!/usr/bin/env python3
"""Fold raw VM counter snapshots into named overhead families.

The app-flow matrix captures one instrumented counters JSON per Phrust row.
This module projects that flat counter surface into a small set of overhead
families so follow-up optimization work can prove it reduced the intended
overhead instead of quoting wall-clock deltas. Counter values are event
counts, not time; families rank by event volume.

Family totals are sums of explicitly listed component counters. Rich-IR and
dense-bytecode interpreters record call/property/dim traffic through disjoint
counters (`function_calls` vs the folded `bytecode_*` opcode map), so a family
sums both sides without double counting. Supporting context that would double
count (inline-cache hit counters, fast-path hit counters for the same events)
is reported as non-summed detail.
"""

from __future__ import annotations

import json
from dataclasses import dataclass
from typing import Any

OVERHEAD_SCHEMA = "phrust-app-flow-overhead-v1"
COUNTER_FAMILY_DOC = "docs/performance/counter-families.md"
MAX_REASONS_IN_MARKDOWN = 3
MAX_REASON_TEXT = 90


@dataclass(frozen=True)
class FamilySpec:
    """One overhead family: which counters sum into it and which explain it."""

    name: str
    description: str
    scalar_keys: tuple[str, ...] = ()
    opcode_keys: tuple[str, ...] = ()
    reason_map_keys: tuple[str, ...] = ()
    detail_keys: tuple[str, ...] = ()
    detail_map_keys: tuple[str, ...] = ()


FAMILY_SPECS: tuple[FamilySpec, ...] = (
    FamilySpec(
        name="value_clones",
        description="Runtime Value copies observed by the layout recorders.",
        scalar_keys=("value_clones",),
    ),
    FamilySpec(
        name="string_allocations",
        description="New string buffer allocations.",
        scalar_keys=("string_allocations",),
        detail_keys=(
            "string_concats",
            "concat_prealloc_hits",
            "symbol_intern_hits",
            "symbol_intern_misses",
            "string_hash_cache_hits",
            "string_hash_cache_misses",
            "symbol_eq_fast_hits",
            "symbol_eq_byte_fallbacks",
        ),
    ),
    FamilySpec(
        name="array_handle_clones",
        description="Shared array handle clones (copy-on-write shares).",
        scalar_keys=("array_handle_clones",),
    ),
    FamilySpec(
        name="cow_separations",
        description="Copy-on-write storage separations before a write.",
        scalar_keys=("cow_separations",),
        detail_keys=("cow_or_reference_fallbacks",),
    ),
    FamilySpec(
        name="object_allocations",
        description="Object storage allocations.",
        scalar_keys=("object_allocations",),
    ),
    FamilySpec(
        name="function_calls",
        description="User function/closure calls across rich and dense interpreters.",
        scalar_keys=("function_calls",),
        opcode_keys=("bytecode_call_function",),
        detail_keys=(
            "dense_direct_call_hits",
            "function_call_ic_hits",
            "function_call_ic_misses",
        ),
    ),
    FamilySpec(
        name="method_calls",
        description="Instance/static method calls across rich and dense interpreters.",
        scalar_keys=("method_calls",),
        opcode_keys=("bytecode_call_method", "bytecode_call_static_method"),
        detail_keys=(
            "method_call_ic_hits",
            "dense_method_call_hits",
            "dense_static_call_hits",
        ),
    ),
    FamilySpec(
        name="property_accesses",
        description="Object property fetch/assign/isset traffic.",
        scalar_keys=("property_accesses",),
        opcode_keys=("bytecode_fetch_property", "bytecode_assign_property"),
        detail_keys=(
            "property_fetches",
            "property_load_ic_hits",
            "property_assign_ic_hits",
            "dense_property_fetch_hits",
            "dense_property_assignment_hits",
        ),
    ),
    FamilySpec(
        name="array_dim_ops",
        description="Array dimension fetch/assign/isset traffic.",
        scalar_keys=("array_dim_fetches",),
        opcode_keys=(
            "bytecode_fetch_dim",
            "bytecode_assign_dim",
            "bytecode_append_dim",
            "bytecode_isset_dim",
            "bytecode_empty_dim",
            "bytecode_unset_dim",
        ),
        detail_keys=("array_mixed_indexed_gets", "packed_fetch_fast_hits"),
    ),
    FamilySpec(
        name="packed_record_arrays",
        description="Packed/record array fast-path hits and their fallbacks.",
        reason_map_keys=(
            "array_fast_path_hits_by_family",
            "array_fast_path_fallback_by_reason",
        ),
        detail_keys=(
            "array_packed_to_mixed_transitions",
            "array_metadata_recomputes",
            "packed_foreach_fast_hits",
            "packed_append_fast_hits",
        ),
    ),
    FamilySpec(
        name="builtin_dispatch",
        description="Builtin/internal function dispatches, caches, and intrinsics.",
        scalar_keys=("internal_function_dispatches",),
        detail_keys=(
            "internal_function_dispatch_cache_hits",
            "internal_function_dispatch_cache_misses",
            "builtin_call_ic_hits",
            "builtin_call_ic_misses",
            "builtin_intrinsic_candidates",
        ),
        detail_map_keys=(
            "intrinsic_hits",
            "intrinsic_misses",
            "builtin_fast_stub_hits",
            "builtin_fast_stub_misses",
        ),
    ),
    FamilySpec(
        name="adaptive_bookkeeping",
        description="Quickening and inline-cache observation events on the dispatch hot path.",
        scalar_keys=("quickening_attempts", "inline_cache_observations"),
        detail_keys=(
            "quickening_specialized",
            "quickening_guard_hits",
            "inline_cache_hits",
            "inline_cache_misses",
        ),
    ),
    FamilySpec(
        name="dense_fallbacks",
        description="Work pushed back to the rich interpreter by dense-lowering gaps.",
        reason_map_keys=(
            "dense_function_fallback_by_reason",
            "dense_call_fallback_by_reason",
            "dense_property_fallback_by_reason",
            "bytecode_auto_fallback_reasons",
            "bytecode_unsupported_reasons",
        ),
        detail_keys=(
            "rich_fallback_functions_planned",
            "rich_fallback_functions_executed",
            "bytecode_unsupported_fallbacks",
        ),
    ),
    FamilySpec(
        name="native_execution",
        description="Native/JIT tier executions and side exits.",
        scalar_keys=("jit_executed", "jit_side_exits"),
        reason_map_keys=("native_side_exits_by_reason", "jit_side_exit_reasons"),
        detail_keys=(
            "native_candidates",
            "native_platform_unavailable",
            "jit_compile_attempts",
            "jit_compiled",
        ),
    ),
)


def _scalar(counters: dict[str, Any], key: str) -> int:
    value = counters.get(key)
    return value if isinstance(value, int) else 0


def _map(counters: dict[str, Any], key: str) -> dict[str, int]:
    value = counters.get(key)
    if not isinstance(value, dict):
        return {}
    return {
        str(reason): count
        for reason, count in value.items()
        if isinstance(count, int) and count != 0
    }


def attribute_family(counters: dict[str, Any], spec: FamilySpec) -> dict[str, Any]:
    """Fold one family: summed components, non-summed details, top reasons."""
    components: dict[str, int] = {}
    for key in spec.scalar_keys:
        value = _scalar(counters, key)
        if value:
            components[key] = value
    opcodes = counters.get("opcodes")
    opcodes = opcodes if isinstance(opcodes, dict) else {}
    for key in spec.opcode_keys:
        value = opcodes.get(key)
        if isinstance(value, int) and value:
            components[key] = value
    reasons: dict[str, int] = {}
    for key in spec.reason_map_keys:
        folded = _map(counters, key)
        components[key] = sum(folded.values())
        for reason, count in folded.items():
            reasons[reason] = reasons.get(reason, 0) + count
    components = {key: value for key, value in components.items() if value}
    details: dict[str, int] = {}
    for key in spec.detail_keys:
        value = _scalar(counters, key)
        if value:
            details[key] = value
    for key in spec.detail_map_keys:
        folded = _map(counters, key)
        if folded:
            details[f"{key}_total"] = sum(folded.values())
    return {
        "total": sum(components.values()),
        "components": components,
        "details": details,
        "reasons": dict(
            sorted(reasons.items(), key=lambda item: (-item[1], item[0]))
        ),
    }


def attribute(counters: dict[str, Any]) -> dict[str, dict[str, Any]]:
    """Project a raw counters snapshot into all overhead families."""
    return {spec.name: attribute_family(counters, spec) for spec in FAMILY_SPECS}


def top_families(
    families: dict[str, dict[str, Any]], count: int = 3
) -> list[dict[str, Any]]:
    """Highest-volume families by summed events, deterministic order."""
    ranked = sorted(
        (
            (name, family)
            for name, family in families.items()
            if isinstance(family, dict) and family.get("total", 0) > 0
        ),
        key=lambda item: (-item[1]["total"], item[0]),
    )
    return [{"family": name, "total": family["total"]} for name, family in ranked[:count]]


def build_report(rows: list[dict[str, Any]]) -> dict[str, Any]:
    """Assemble the overhead report from per-row scenario attributions.

    Each input row must carry `scenario`, `row`, and raw `counters`.
    """
    report_rows = []
    for row in rows:
        families = attribute(row.get("counters", {}))
        report_rows.append(
            {
                "scenario": row["scenario"],
                "row": row["row"],
                "families": families,
                "top_families": top_families(families),
            }
        )
    return {
        "schema": OVERHEAD_SCHEMA,
        "generated_by": "app_flow_matrix.py",
        "family_doc": COUNTER_FAMILY_DOC,
        "families": [
            {"name": spec.name, "description": spec.description}
            for spec in FAMILY_SPECS
        ],
        "rows": report_rows,
    }


def _clip(text: str) -> str:
    if len(text) <= MAX_REASON_TEXT:
        return text
    return text[: MAX_REASON_TEXT - 1] + "…"


def render_markdown(report: dict[str, Any]) -> str:
    lines = [
        "# Application-Flow Overhead Attribution",
        "",
        "Local artifact generated by the app-flow matrix from each row's",
        "dedicated instrumented run. Values are event counts, not time.",
        f"Family definitions: `{report['family_doc']}`. Do not commit this file.",
        "",
        "| Scenario | Row | Top overhead families (events) |",
        "| --- | --- | --- |",
    ]
    for row in report["rows"]:
        top = ", ".join(
            f"{entry['family']}={entry['total']}" for entry in row["top_families"]
        )
        lines.append(f"| `{row['scenario']}` | `{row['row']}` | {top or 'n/a'} |")
    for row in report["rows"]:
        lines.extend(["", f"## `{row['scenario']}` — `{row['row']}`", ""])
        lines.append("| Family | Events | Components | Top reasons |")
        lines.append("| --- | --- | --- | --- |")
        for name, family in row["families"].items():
            if family["total"] == 0 and not family["details"]:
                continue
            components = ", ".join(
                f"{key}={value}" for key, value in family["components"].items()
            )
            reasons = "; ".join(
                f"{_clip(reason)}×{count}"
                for reason, count in list(family["reasons"].items())[
                    :MAX_REASONS_IN_MARKDOWN
                ]
            )
            lines.append(
                f"| `{name}` | {family['total']} | {components or 'n/a'} | {reasons or 'n/a'} |"
            )
    return "\n".join(lines) + "\n"


def self_test() -> int:
    counters = {
        "value_clones": 100,
        "string_allocations": 40,
        "array_handle_clones": 60,
        "cow_separations": 5,
        "object_allocations": 2,
        "function_calls": 7,
        "method_calls": 3,
        "property_accesses": 4,
        "array_dim_fetches": 9,
        "internal_function_dispatches": 6,
        "opcodes": {
            "bytecode_call_function": 11,
            "bytecode_call_method": 2,
            "bytecode_fetch_dim": 5,
            "load_local": 999,
        },
        "array_fast_path_hits_by_family": {"packed_append": 8},
        "array_fast_path_fallback_by_reason": {"cow_or_reference": 4},
        "dense_function_fallback_by_reason": {"instruction_subset": 1},
        "intrinsic_hits": {"count": 3},
        "rich_fallback_functions_executed": 1,
        "quickening_attempts": 20,
        "inline_cache_observations": 15,
    }
    families = attribute(counters)
    assert families["value_clones"]["total"] == 100, families["value_clones"]
    assert families["function_calls"]["total"] == 18, families["function_calls"]
    assert families["method_calls"]["total"] == 5, families["method_calls"]
    assert families["array_dim_ops"]["total"] == 14, families["array_dim_ops"]
    assert families["packed_record_arrays"]["total"] == 12
    assert families["packed_record_arrays"]["reasons"] == {
        "packed_append": 8,
        "cow_or_reference": 4,
    }
    assert families["dense_fallbacks"]["total"] == 1
    assert families["dense_fallbacks"]["details"] == {
        "rich_fallback_functions_executed": 1
    }
    assert families["builtin_dispatch"]["details"]["intrinsic_hits_total"] == 3
    assert families["adaptive_bookkeeping"]["total"] == 35
    assert families["native_execution"]["total"] == 0
    top = top_families(families)
    assert [entry["family"] for entry in top] == [
        "value_clones",
        "array_handle_clones",
        "string_allocations",
    ], top
    report = build_report(
        [{"scenario": "sample", "row": "phrust-default", "counters": counters}]
    )
    assert report["schema"] == OVERHEAD_SCHEMA
    rendered = render_markdown(report)
    assert "value_clones=100" in rendered
    assert "`sample`" in rendered
    json.dumps(report)  # must be JSON-serializable
    empty = attribute({})
    assert all(family["total"] == 0 for family in empty.values())
    assert top_families(empty) == []
    print(f"[pass] overhead attribution self-test validated {len(FAMILY_SPECS)} families")
    return 0


if __name__ == "__main__":
    raise SystemExit(self_test())
