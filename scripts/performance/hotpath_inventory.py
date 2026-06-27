#!/usr/bin/env python3
"""Build a performance hot-path inventory from VM counter benchmark reports."""

from __future__ import annotations

import argparse
import json
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_REPORT = ROOT / "target/performance/bench-performance-smoke.json"
DEFAULT_JSON = ROOT / "target/performance/hotpaths.json"
DEFAULT_MARKDOWN = ROOT / "docs/hotpath-inventory.md"


@dataclass(frozen=True)
class CategorySpec:
    key: str
    title: str
    counters: tuple[str, ...]
    optimization_layer: str
    risk: str
    correctness_tests: str


CATEGORIES = (
    CategorySpec(
        "dispatch",
        "Dispatch",
        ("instructions_executed",),
        "interpreter dispatch and bytecode layout",
        "changing dispatch can reorder side effects or diagnostics",
        "runtime fixture diff plus bytecode snapshots for the same fixture family",
    ),
    CategorySpec(
        "calls",
        "Calls",
        ("function_calls", "method_calls"),
        "call frame setup, method lookup, and later inline caches",
        "call semantics include references, late static binding, visibility, and argument coercion",
        "function, method, reference, variadic, and visibility fixtures",
    ),
    CategorySpec(
        "arrays",
        "Arrays",
        ("array_dim_fetches",),
        "array dimension read/write fast paths",
        "PHP array ordering, key coercion, references, and copy-on-write are observable",
        "packed, mixed, append, foreach, reference, and copy-on-write fixtures",
    ),
    CategorySpec(
        "properties",
        "Properties",
        ("property_accesses", "property_fetches"),
        "property lookup and object layout caches",
        "visibility, magic methods, dynamic properties, and typed properties are observable",
        "public/private/protected, magic, dynamic, and typed-property fixtures",
    ),
    CategorySpec(
        "strings",
        "Strings",
        ("string_concats",),
        "string concatenation allocation and conversion fast paths",
        "PHP scalar conversion and binary-safe string behavior must stay exact",
        "concat, scalar conversion, encoding-neutral, and error-order fixtures",
    ),
    CategorySpec(
        "output",
        "Output",
        (
            "output_bytes",
            "output_buffer_appends",
            "output_buffer_flushes",
            "output_fast_appends",
        ),
        "echo/print output buffering and batched internal buffer appends",
        "stdout/stderr bytes, output buffering levels, callbacks, and conversion errors are observable",
        "echo, print, output-buffering, object-to-string, and conversion-error fixtures",
    ),
    CategorySpec(
        "type_checks",
        "Type Checks",
        ("type_checks",),
        "class/interface type-check caches",
        "autoload, inheritance, aliases, and interface checks affect correctness",
        "instanceof, catch type, inheritance, interface, and autoload fixtures",
    ),
    CategorySpec(
        "includes_autoload",
        "Includes/Autoload",
        ("includes", "autoloads"),
        "include path resolution and autoload metadata caches",
        "include side effects, working directory, once semantics, and autoload order are observable",
        "include/require, include_once/require_once, path, and autoload fixtures",
    ),
)

STD_LIB_FIXTURES = {
    "performance.perf_smoke.rust-vm.arrays_packed": ("count",),
    "performance.perf_smoke.rust-vm.autoload_smoke": (
        "spl_autoload_register",
        "strtolower",
    ),
    "performance.perf_smoke.rust-vm.stdlib_dispatch": (
        "count",
        "strlen",
        "is_int",
        "array_values",
        "strtolower",
    ),
    "performance.perf_smoke.rust-vm.strings_concat": ("strlen",),
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("report", nargs="?", type=Path, default=DEFAULT_REPORT)
    parser.add_argument("--json-out", type=Path, default=DEFAULT_JSON)
    parser.add_argument("--markdown-out", type=Path, default=DEFAULT_MARKDOWN)
    parser.add_argument("--self-test", action="store_true")
    return parser.parse_args()


def rel(path: Path) -> str:
    try:
        return path.resolve().relative_to(ROOT).as_posix()
    except ValueError:
        return path.as_posix()


def load_report(path: Path) -> dict[str, Any]:
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except OSError as error:
        raise SystemExit(f"{path}: {error}") from error
    except json.JSONDecodeError as error:
        raise SystemExit(f"{path}: invalid JSON: {error}") from error
    if not isinstance(data, dict):
        raise SystemExit(f"{path}: report root must be a JSON object")
    if not isinstance(data.get("measurements"), list):
        raise SystemExit(f"{path}: measurements must be a JSON array")
    return data


def rust_counter_measurements(report: dict[str, Any]) -> list[dict[str, Any]]:
    measurements = []
    for measurement in report.get("measurements", []):
        if not isinstance(measurement, dict):
            continue
        if measurement.get("engine") != "rust-vm":
            continue
        counters = measurement.get("vm_counters")
        if not isinstance(counters, dict):
            continue
        measurements.append(measurement)
    return measurements


def scenario_id(measurement: dict[str, Any]) -> str:
    scenario = measurement.get("scenario")
    if isinstance(scenario, dict) and isinstance(scenario.get("id"), str):
        return scenario["id"]
    return "<unknown>"


def scenario_fixture(measurement: dict[str, Any]) -> str:
    scenario = measurement.get("scenario")
    if isinstance(scenario, dict) and isinstance(scenario.get("fixture"), str):
        return scenario["fixture"]
    return "<unknown>"


def scenario_name(measurement: dict[str, Any]) -> str:
    scenario = measurement.get("scenario")
    if isinstance(scenario, dict) and isinstance(scenario.get("name"), str):
        return scenario["name"]
    return scenario_id(measurement)


def counter_value(counters: dict[str, Any], names: tuple[str, ...]) -> int:
    total = 0
    for name in names:
        value = counters.get(name, 0)
        if isinstance(value, int):
            total += value
    return total


def benefit_class(category: str, value: int) -> str:
    if value <= 0:
        return "none"
    if category == "dispatch":
        if value >= 200:
            return "high"
        if value >= 100:
            return "medium"
        return "low"
    if value >= 8:
        return "high"
    if value >= 3:
        return "medium"
    return "low"


def build_inventory(report: dict[str, Any], report_path: Path) -> dict[str, Any]:
    measurements = rust_counter_measurements(report)
    if not measurements:
        raise SystemExit(
            f"{report_path}: no rust-vm measurements with vm_counters found; "
            "run `just bench-performance-smoke` with counters enabled"
        )

    categories: list[dict[str, Any]] = []
    candidates: list[dict[str, Any]] = []
    gaps: list[dict[str, str]] = []

    for spec in CATEGORIES:
        rows = []
        for measurement in measurements:
            counters = measurement["vm_counters"]
            value = counter_value(counters, spec.counters)
            rows.append(
                {
                    "scenario_id": scenario_id(measurement),
                    "fixture": scenario_fixture(measurement),
                    "name": scenario_name(measurement),
                    "value": value,
                    "counters": {name: counters.get(name, 0) for name in spec.counters},
                }
            )
        rows.sort(key=lambda row: (-row["value"], row["scenario_id"]))
        total = sum(row["value"] for row in rows)
        categories.append(
            {
                "key": spec.key,
                "title": spec.title,
                "total": total,
                "counters": list(spec.counters),
                "top_scenarios": rows[:5],
                "coverage": "complete_for_current_counter_set" if total > 0 else "no_events_in_smoke_corpus",
            }
        )
        if total > 0:
            top = rows[0]
            candidates.append(
                {
                    "id": f"performance.hotpath.{spec.key}",
                    "category": spec.title,
                    "scenario_id": top["scenario_id"],
                    "fixture": top["fixture"],
                    "evidence": f"{top['value']} counted event(s) in {top['fixture']}",
                    "primary_counter_value": top["value"],
                    "optimization_layer": spec.optimization_layer,
                    "risk": spec.risk,
                    "correctness_tests": spec.correctness_tests,
                    "expected_benefit_class": benefit_class(spec.key, top["value"]),
                }
            )
        else:
            gaps.append(
                {
                    "id": f"PERF-GAP-HOTPATH-{spec.key.upper()}-NO-EVENTS",
                    "description": (
                        f"No performance smoke fixture currently emits {spec.title} counter events; "
                        "the category is listed but not prioritized."
                    ),
                }
            )

    stdlib_rows = []
    for measurement in measurements:
        sid = scenario_id(measurement)
        functions = STD_LIB_FIXTURES.get(sid, ())
        if not functions:
            continue
        counters = measurement["vm_counters"]
        stdlib_rows.append(
            {
                "scenario_id": sid,
                "fixture": scenario_fixture(measurement),
                "functions": list(functions),
                "function_calls": counters.get("function_calls", 0),
                "internal_function_dispatches": counters.get("internal_function_dispatches", 0),
                "dispatch_cache_hits": counters.get("internal_function_dispatch_cache_hits", 0),
                "dispatch_cache_misses": counters.get("internal_function_dispatch_cache_misses", 0),
            }
        )
    stdlib_rows.sort(
        key=lambda row: (
            -int(row["dispatch_cache_hits"]),
            -int(row["internal_function_dispatches"]),
            row["scenario_id"],
        )
    )
    categories.append(
        {
            "key": "stdlib_calls",
            "title": "Standard Library Calls",
            "total": sum(int(row["internal_function_dispatches"]) for row in stdlib_rows),
            "counters": [
                "internal_function_dispatches",
                "internal_function_dispatch_cache_hits",
                "internal_function_dispatch_cache_misses",
            ],
            "top_scenarios": stdlib_rows[:5],
            "coverage": "builtin_dispatch_counters_visible_in_smoke_corpus",
        }
    )
    if stdlib_rows:
        top = stdlib_rows[0]
        candidates.append(
            {
                "id": "performance.hotpath.stdlib_calls",
                "category": "Standard Library Calls",
                "scenario_id": top["scenario_id"],
                "fixture": top["fixture"],
                "evidence": (
                    f"{top['internal_function_dispatches']} internal dispatch(es), "
                    f"{top['dispatch_cache_hits']} dispatch-cache hit(s), and "
                    f"{top['dispatch_cache_misses']} miss(es) in {top['fixture']} "
                    f"covering {', '.join(top['functions'])}"
                ),
                "primary_counter_value": int(top["dispatch_cache_hits"]),
                "optimization_layer": "builtin dispatch and standard-library call shims",
                "risk": "cache must not bypass named-argument conversion, arity checks, TypeError/ValueError diagnostics, or reflection metadata",
                "correctness_tests": "per-builtin fixtures plus differential stdlib gates before any fast path",
                "expected_benefit_class": benefit_class("stdlib_calls", int(top["dispatch_cache_hits"])),
            }
        )

    gaps.append(
        {
            "id": "PERF-GAP-HOTPATH-CORPUS-REPRESENTATIVENESS",
            "description": "The current smoke corpus and optional framework micro-smokes are deterministic but too small to represent real Composer/framework workloads.",
        }
    )

    candidates.sort(
        key=lambda row: (
            {"high": 0, "medium": 1, "low": 2, "none": 3}[row["expected_benefit_class"]],
            -int(row["primary_counter_value"]),
            row["id"],
        )
    )
    for index, candidate in enumerate(candidates, start=1):
        candidate["priority"] = index

    return {
        "schema_version": 1,
        "source_report": rel(report_path),
        "run_id": report.get("run_id"),
        "rust_measurements": len(measurements),
        "categories": categories,
        "candidates": candidates,
        "counter_gaps": gaps,
        "non_representative_notes": [
            "The smoke corpus uses tiny deterministic loops so instruction counts are useful for ranking within the corpus, not for real-world throughput claims.",
            "No fixture exercises real Composer autoload trees, large arrays, I/O-heavy includes, closures with captures, generators, fibers, or exception-heavy paths at framework scale.",
            "Wall-clock timings are intentionally excluded from the hot-path priority calculation.",
        ],
        "no_go_areas": [
            "Do not change PHP-visible evaluation order, diagnostics, include side effects, or autoload ordering for a performance win.",
            "Do not implement JIT, standard-library ABI shortcuts, or semantic rewritesfrom this inventory.",
            "Do not promote a candidate without differential correctness fixtures for its risk area.",
        ],
    }


def top_fixture(category: dict[str, Any]) -> str:
    if category.get("total") == 0:
        return "none observed"
    scenarios = category.get("top_scenarios", [])
    if not scenarios:
        return "-"
    top = scenarios[0]
    if not isinstance(top, dict):
        return "-"
    fixture = top.get("fixture", "-")
    value = top.get("value", top.get("function_calls", 0))
    return f"`{fixture}` ({value})"


def render_markdown(inventory: dict[str, Any]) -> str:
    lines = [
        "# performance Hot-Path Inventory",
        "",
        f"Source report: `{inventory['source_report']}`.",
        "",
        "This inventory is derived from Rust VM counters in the performance smoke benchmark report. It uses counter totals, not wall-clock timings, to avoid host-specific priorities.",
        "",
        "## Category Summary",
        "",
        "| Category | Counter(s) | Total | Top fixture | Coverage |",
        "| --- | --- | ---: | --- | --- |",
    ]
    for category in inventory["categories"]:
        lines.append(
            f"| {category['title']} | `{', '.join(category['counters'])}` | "
            f"{category['total']} | {top_fixture(category)} | {category['coverage']} |"
        )

    lines.extend(
        [
            "",
            "## Prioritized Candidates",
            "",
            "| Priority | Hot path | Evidence | Optimization layer | Risk | Required correctness tests | Benefit |",
            "| ---: | --- | --- | --- | --- | --- | --- |",
        ]
    )
    for candidate in inventory["candidates"]:
        lines.append(
            f"| {candidate['priority']} | {candidate['category']} | {candidate['evidence']} | "
            f"{candidate['optimization_layer']} | {candidate['risk']} | "
            f"{candidate['correctness_tests']} | {candidate['expected_benefit_class']} |"
        )

    lines.extend(
        [
            "",
            "## Counter Gaps",
            "",
        ]
    )
    for gap in inventory["counter_gaps"]:
        lines.append(f"- `{gap['id']}`: {gap['description']}")

    lines.extend(
        [
            "",
            "## Non-Representative Fixture Notes",
            "",
        ]
    )
    lines.extend(f"- {note}" for note in inventory["non_representative_notes"])

    lines.extend(
        [
            "",
            "## No-Go Areas",
            "",
        ]
    )
    lines.extend(f"- {note}" for note in inventory["no_go_areas"])
    return "\n".join(lines) + "\n"


def write_outputs(inventory: dict[str, Any], json_path: Path, markdown_path: Path) -> None:
    json_path.parent.mkdir(parents=True, exist_ok=True)
    json_path.write_text(json.dumps(inventory, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    markdown_path.parent.mkdir(parents=True, exist_ok=True)
    markdown_path.write_text(render_markdown(inventory), encoding="utf-8")


def self_test() -> int:
    report = {
        "run_id": "self-test",
        "measurements": [
            {
                "engine": "rust-vm",
                "scenario": {
                    "id": "performance.perf_smoke.rust-vm.function_calls",
                    "name": "function_calls (rust-vm)",
                    "fixture": "tests/fixtures/performance/perf_smoke/function_calls.php",
                },
                "vm_counters": {
                    "instructions_executed": 10,
                    "function_calls": 5,
                    "method_calls": 0,
                    "array_dim_fetches": 0,
                    "property_accesses": 0,
                    "property_fetches": 0,
                    "string_concats": 0,
                    "type_checks": 0,
                    "includes": 0,
                    "autoloads": 0,
                },
            }
        ],
    }
    inventory = build_inventory(report, Path("self-test.json"))
    if len(inventory["candidates"]) < 2:
        print("[fail] expected at least dispatch and calls candidates", file=sys.stderr)
        return 1
    markdown = render_markdown(inventory)
    if "Prioritized Candidates" not in markdown or "Dispatch" not in markdown:
        print("[fail] markdown missing expected sections", file=sys.stderr)
        return 1
    print("[pass] hotpath_inventory self-test")
    return 0


def main() -> int:
    args = parse_args()
    if args.self_test:
        return self_test()
    report = load_report(args.report)
    inventory = build_inventory(report, args.report)
    write_outputs(inventory, args.json_out, args.markdown_out)
    print(f"[pass] performance hotpath inventory wrote {rel(args.json_out)} and {rel(args.markdown_out)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
