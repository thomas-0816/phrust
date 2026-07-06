#!/usr/bin/env python3
"""Build the fastest-engine hot-path report from existing performance evidence."""

from __future__ import annotations

import argparse
import json
import sys
from collections import Counter
from dataclasses import dataclass
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_BENCHMARK = ROOT / "target/performance/benchmark-smoke.json"
DEFAULT_FRAMEWORK = ROOT / "target/performance/framework-smoke/summary.json"
DEFAULT_ACCELERATION = ROOT / "target/performance/acceleration/summary.json"
DEFAULT_COUNTER_ROOT = ROOT / "target/performance"
DEFAULT_OUT = ROOT / "target/performance/fastest/hotpath-report.json"
DEFAULT_MARKDOWN = ROOT / "target/performance/fastest/hotpath-report.md"
DEFAULT_SUMMARY_DOC = ROOT / "target/performance/fastest/hotpath-report.md"
DEFAULT_CALLGRIND = ROOT / "target/performance/callgrind/summary.json"


@dataclass(frozen=True)
class AreaSpec:
    key: str
    title: str
    counters: tuple[str, ...]
    interpretation: str
    required_next_evidence: str


AREAS = (
    AreaSpec(
        "dispatch",
        "Dispatch",
        ("instructions_executed", "bytecode_instructions_executed"),
        "Interpreter and dense-bytecode dispatch cost.",
        "Dense opcode, quickening, and superinstruction A/B fixtures.",
    ),
    AreaSpec(
        "calls_builtins",
        "Calls And Builtins",
        (
            "function_calls",
            "method_calls",
            "internal_function_dispatches",
            "function_call_ic_hits",
            "function_call_ic_misses",
            "builtin_call_ic_hits",
            "builtin_call_ic_misses",
            "builtin_fast_stub_hits.count",
            "builtin_fast_stub_hits.strlen",
            "builtin_fast_stub_hits.is_int",
            "builtin_fast_stub_hits.is_string",
            "builtin_fast_stub_hits.is_array",
            "slow_path_calls_by_reason.builtin_stub.strlen.type",
            "slow_path_calls_by_reason.builtin_intrinsic.str_contains.type",
            "slow_path_calls_by_reason.jit.known_call",
            "tiny_frame_candidates",
            "specialized_frame_hits",
            "arg_array_avoided",
            "heap_frame_avoided",
            "call_frame_layout_observed.tiny_leaf_frame",
            "call_frame_layout_observed.known_function_frame",
            "call_frame_layout_observed.known_method_frame",
            "generic_frame_fallback_by_reason.not_tiny_leaf",
            "generic_frame_fallback_by_reason.class_context",
            "generic_frame_fallback_by_reason.named_or_variadic",
            "call_ic_megamorphic_fallbacks",
            "method_ic_hits",
            "method_ic_misses",
        ),
        "Function, method, and internal builtin dispatch work.",
        "Call-shape, by-reference, named-argument, method visibility, and stdlib diffs.",
    ),
    AreaSpec(
        "properties_methods",
        "Properties And Methods",
        (
            "property_accesses",
            "property_fetches",
            "property_ic_hits",
            "property_ic_misses",
            "property_ic_fallback_reasons.layout_epoch_mismatch",
            "property_ic_fallback_reasons.receiver_class_mismatch",
            "property_ic_fallback_reasons.uninitialized_typed_property",
            "slow_path_calls_by_reason.property_fetch.layout_epoch_mismatch",
            "slow_path_calls_by_reason.property_assign.type_mismatch",
            "method_ic_hits",
            "method_ic_misses",
        ),
        "Object property lookup, property-shape caches, and method cache work.",
        "Visibility, typed/readonly properties, magic, hooks, dynamic properties, and override fixtures.",
    ),
    AreaSpec(
        "arrays_foreach",
        "Arrays And Foreach",
        (
            "array_dim_fetches",
            "array_packed_append_fast_path_hits",
            "array_packed_read_fast_path_hits",
            "array_sequential_foreach_fast_path_hits",
            "packed_fetch_fast_hits",
            "packed_fetch_bounds_fallbacks",
            "packed_fetch_layout_fallbacks",
            "packed_append_fast_hits",
            "packed_foreach_fast_hits",
            "cow_or_reference_fallbacks",
            "array_packed_to_mixed_transitions",
            "slow_path_calls_by_reason.array.cow_or_reference",
            "slow_path_calls_by_reason.array.key_coercion",
            "slow_path_calls_by_reason.array.layout_or_key",
        ),
        "Packed/mixed array lookup, append, COW/reference fallback, and foreach work.",
        "Packed, mixed, numeric-string key, by-ref foreach, COW, mutation, and order fixtures.",
    ),
    AreaSpec(
        "strings_output",
        "Strings And Output",
        (
            "string_concats",
            "string_concat_fast_path_hits",
            "concat_prealloc_hits",
            "concat_fallback_by_reason.scalar_conversion",
            "concat_fallback_by_reason.object_to_string",
            "slow_path_calls_by_reason.concat.scalar_conversion",
            "slow_path_calls_by_reason.concat.object_to_string",
            "output_bytes",
            "output_buffer_appends",
            "output_buffer_batch_writes",
            "output_batched_appends",
            "output_batch_bytes",
            "output_buffer_flushes",
            "output_fast_appends",
            "output_slow_appends_by_reason.array_conversion_warning",
            "output_slow_appends_by_reason.object_to_string",
            "output_slow_appends_by_reason.resource_conversion",
            "slow_path_calls_by_reason.output.array_conversion_warning",
            "slow_path_calls_by_reason.output.object_to_string",
            "slow_path_calls_by_reason.output.resource_conversion",
        ),
        "String concatenation, conversion-sensitive output, and buffer append work.",
        "Output-buffer callback, object conversion, binary string, and diagnostic-order fixtures.",
    ),
    AreaSpec(
        "include_autoload",
        "Include And Autoload",
        (
            "includes",
            "autoloads",
            "include_graph_hits",
            "include_graph_misses",
            "autoload_graph_hits",
            "autoload_graph_misses",
            "include_path_ic_hits",
            "include_path_ic_misses",
            "autoload_class_lookup_ic_hits",
            "autoload_class_lookup_ic_misses",
            "fallback_by_path_semantics.missing_path",
            "fallback_by_path_semantics.stream_wrapper",
            "slow_path_calls_by_reason.include_autoload.missing_path",
            "slow_path_calls_by_reason.include_autoload.stream_wrapper",
        ),
        "Include-path resolution, dependency graph hits, autoload lookup, and warning fallbacks.",
        "Include/require warning order, stream-wrapper rejection, generated autoload, and invalidation fixtures.",
    ),
    AreaSpec(
        "frontend_byte_scanning",
        "Frontend Byte Scanning",
        (
            "source_bytes_scanned",
            "lexer_bytes_scanned",
            "newlines_counted",
            "ascii_identifier_chunks",
        ),
        "Lexer/source-map byte scanning. Current counters may be absent.",
        "Lexer/parser/CST parity plus byte-kernel tests before call-site replacement.",
    ),
    AreaSpec(
        "optimizer_runtime_allocation",
        "Optimizer And Runtime Allocation",
        (
            "frame_allocations",
            "frame_reuses",
            "frames_allocated",
            "frames_reused",
            "register_files_allocated",
            "register_files_reused",
            "tiny_frame_candidates",
            "specialized_frame_hits",
            "arg_array_avoided",
            "heap_frame_avoided",
            "value_clones",
            "string_allocations",
            "array_handle_clones",
            "cow_separations",
            "reference_cell_creations",
            "object_allocations",
        ),
        "Frame/register reuse, allocation pressure, COW, and optimizer/runtime support work.",
        "Destructor, reference, COW, output-order, and verifier-bracketed optimizer fixtures.",
    ),
    AreaSpec(
        "native_jit_candidates",
        "Native And JIT Candidates",
        (
            "jit_compile_attempts",
            "jit_compiled",
            "jit_executed",
            "jit_bailouts",
            "jit_compile_successes",
            "jit_side_exits",
            "jit_fallbacks",
            "jit_guard_failures",
            "jit_fast_path_hits",
            "jit_helper_calls",
            "slow_path_calls_by_reason.jit.generic",
            "slow_path_calls_by_reason.jit.property_load",
            "jit_compile_cache_hits",
            "jit_compile_cache_misses",
            "executed_regions",
            "side_exits",
            "fast_path_hits",
            "packed_foreach_sum_fast_hits",
            "known_call_fast_hits",
            "string_concat_fast_hits",
        ),
        "Default-off native-tier and side-exit candidate evidence.",
        "Feature-gated JIT rows with interpreter fallback, compile-budget, and side-exit reports.",
    ),
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--benchmark", type=Path, default=DEFAULT_BENCHMARK)
    parser.add_argument("--framework", type=Path, default=DEFAULT_FRAMEWORK)
    parser.add_argument("--acceleration", type=Path, default=DEFAULT_ACCELERATION)
    parser.add_argument("--counter-root", type=Path, default=DEFAULT_COUNTER_ROOT)
    parser.add_argument("--callgrind", type=Path, default=DEFAULT_CALLGRIND)
    parser.add_argument("--json-out", type=Path, default=DEFAULT_OUT)
    parser.add_argument("--markdown-out", type=Path, default=DEFAULT_MARKDOWN)
    parser.add_argument("--summary-doc", type=Path, default=DEFAULT_SUMMARY_DOC)
    parser.add_argument("--self-test", action="store_true")
    return parser.parse_args()


def rel(path: Path) -> str:
    try:
        return path.resolve().relative_to(ROOT).as_posix()
    except ValueError:
        return path.as_posix()


def load_json(path: Path) -> tuple[dict[str, Any] | None, str | None]:
    if not path.is_file():
        return None, f"missing: {rel(path)}"
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except OSError as error:
        return None, f"{rel(path)}: {error}"
    except json.JSONDecodeError as error:
        return None, f"{rel(path)}: invalid JSON: {error}"
    if not isinstance(data, dict):
        return None, f"{rel(path)}: root must be a JSON object"
    return data, None


def flatten_counters(data: dict[str, Any]) -> dict[str, int]:
    flattened: dict[str, int] = {}

    def visit(prefix: str, value: Any) -> None:
        if isinstance(value, bool):
            return
        if isinstance(value, int):
            if value < 0:
                raise ValueError(f"negative counter {prefix}: {value}")
            flattened[prefix] = value
            return
        if isinstance(value, dict):
            for key, nested in value.items():
                if isinstance(key, str):
                    visit(f"{prefix}.{key}" if prefix else key, nested)

    visit("", data)
    return flattened


def add_record(
    records: list[dict[str, Any]],
    *,
    source: str,
    label: str,
    path: str,
    counters: dict[str, Any],
) -> None:
    try:
        flattened = flatten_counters(counters)
    except ValueError as error:
        raise SystemExit(f"{path}: {error}") from error
    if not flattened:
        return
    records.append(
        {
            "source": source,
            "label": label,
            "path": path,
            "counters": flattened,
        }
    )


def scenario_label(measurement: dict[str, Any]) -> tuple[str, str]:
    scenario = measurement.get("scenario")
    if isinstance(scenario, dict):
        label = str(scenario.get("id") or scenario.get("name") or "<unknown>")
        fixture = str(scenario.get("fixture") or label)
        return label, fixture
    return "<unknown>", "<unknown>"


def collect_benchmark(
    report: dict[str, Any] | None,
    error: str | None,
    path: Path,
    records: list[dict[str, Any]],
) -> dict[str, Any]:
    status = {"path": rel(path), "status": "ok" if error is None else "skipped", "reason": error}
    if report is None:
        return status
    measurements = report.get("measurements")
    if not isinstance(measurements, list):
        return {**status, "status": "skipped", "reason": "measurements missing"}
    count = 0
    for measurement in measurements:
        if not isinstance(measurement, dict) or measurement.get("engine") != "rust-vm":
            continue
        counters = measurement.get("vm_counters")
        if not isinstance(counters, dict):
            continue
        label, fixture = scenario_label(measurement)
        add_record(
            records,
            source="benchmark-smoke",
            label=label,
            path=fixture,
            counters=counters,
        )
        count += 1
    return {**status, "record_count": count}


def collect_framework(
    report: dict[str, Any] | None,
    error: str | None,
    path: Path,
    records: list[dict[str, Any]],
) -> dict[str, Any]:
    status = {"path": rel(path), "status": "ok" if error is None else "skipped", "reason": error}
    if report is None:
        return status
    scenarios = report.get("scenarios")
    if not isinstance(scenarios, list):
        return {**status, "status": "skipped", "reason": "scenarios missing"}
    count = 0
    for scenario in scenarios:
        if not isinstance(scenario, dict):
            continue
        counters = scenario.get("counter_focus")
        if not isinstance(counters, dict):
            continue
        add_record(
            records,
            source="framework-smoke",
            label=str(scenario.get("id", "<unknown>")),
            path=str(scenario.get("fixture", "<unknown>")),
            counters=counters,
        )
        count += 1
    return {**status, "record_count": count}


def collect_acceleration(
    report: dict[str, Any] | None,
    error: str | None,
    path: Path,
    records: list[dict[str, Any]],
) -> dict[str, Any]:
    status = {"path": rel(path), "status": "ok" if error is None else "skipped", "reason": error}
    if report is None:
        return status
    rows = report.get("rows")
    if not isinstance(rows, list):
        return {**status, "status": "skipped", "reason": "rows missing"}
    count = 0
    for row in rows:
        if not isinstance(row, dict) or row.get("status") not in (None, "pass"):
            continue
        counters = row.get("counter_focus")
        if not isinstance(counters, dict):
            continue
        add_record(
            records,
            source="acceleration-matrix",
            label=f"{row.get('variant', '<unknown>')}:{row.get('fixture', '<unknown>')}",
            path=str(row.get("fixture", "<unknown>")),
            counters=counters,
        )
        count += 1
    return {**status, "record_count": count}


def collect_counter_files(root: Path, records: list[dict[str, Any]]) -> dict[str, Any]:
    if not root.is_dir():
        return {"path": rel(root), "status": "skipped", "reason": "counter root missing"}
    count = 0
    paths = set(root.glob("**/*.counters.json"))
    paths.update(root.glob("**/*-counters.json"))
    for path in sorted(paths):
        data, error = load_json(path)
        if error is not None or data is None:
            raise SystemExit(error or f"{rel(path)}: missing counters")
        add_record(
            records,
            source="counter-json",
            label=path.stem,
            path=rel(path),
            counters=data,
        )
        count += 1
    return {"path": rel(root), "status": "ok", "record_count": count}


def optional_profiler_status(callgrind_path: Path) -> list[dict[str, Any]]:
    callgrind, callgrind_error = load_json(callgrind_path)
    profilers: list[dict[str, Any]] = []
    if callgrind is None:
        profilers.append(
            {
                "tool": "callgrind",
                "status": "skipped",
                "path": rel(callgrind_path),
                "reason": callgrind_error,
            }
        )
    else:
        profilers.append(
            {
                "tool": "callgrind",
                "status": str(callgrind.get("status", "unknown")),
                "path": rel(callgrind_path),
                "reason": callgrind.get("reason"),
                "measurement_count": len(callgrind.get("measurements", []))
                if isinstance(callgrind.get("measurements"), list)
                else 0,
            }
        )
    perf_root = ROOT / "target/performance/perf"
    perf_candidates = sorted(perf_root.glob("**/*.json")) if perf_root.is_dir() else []
    perf_candidates.extend(sorted((ROOT / "target/performance").glob("linux-perf*.json")))
    if perf_candidates:
        profilers.append(
            {
                "tool": "linux-perf",
                "status": "available",
                "path": ", ".join(rel(path) for path in perf_candidates),
                "reason": None,
            }
        )
    else:
        profilers.append(
            {
                "tool": "linux-perf",
                "status": "skipped",
                "path": "target/performance/perf*.json",
                "reason": "no Linux perf artifact found",
            }
        )
    return profilers


def area_score(record: dict[str, Any], spec: AreaSpec) -> int:
    counters = record["counters"]
    return sum(int(counters.get(counter, 0)) for counter in spec.counters)


def classify(total: int) -> str:
    if total >= 10_000:
        return "very_high"
    if total >= 1_000:
        return "high"
    if total >= 100:
        return "medium"
    if total > 0:
        return "low"
    return "no_current_counter_evidence"


def summarize_areas(records: list[dict[str, Any]]) -> list[dict[str, Any]]:
    areas: list[dict[str, Any]] = []
    for spec in AREAS:
        by_source: Counter[str] = Counter()
        rows = []
        for record in records:
            value = area_score(record, spec)
            if value <= 0:
                continue
            by_source[record["source"]] += value
            rows.append(
                {
                    "source": record["source"],
                    "label": record["label"],
                    "path": record["path"],
                    "value": value,
                    "matching_counters": {
                        counter: record["counters"].get(counter, 0)
                        for counter in spec.counters
                        if record["counters"].get(counter, 0)
                    },
                }
            )
        rows.sort(key=lambda row: (-int(row["value"]), row["source"], row["label"]))
        total = sum(int(row["value"]) for row in rows)
        areas.append(
            {
                "key": spec.key,
                "title": spec.title,
                "total_counter_events": total,
                "rank_class": classify(total),
                "counters": list(spec.counters),
                "by_source": dict(sorted(by_source.items())),
                "top_records": rows[:8],
                "interpretation": spec.interpretation,
                "required_next_evidence": spec.required_next_evidence,
            }
        )
    areas.sort(
        key=lambda area: (
            -int(area["total_counter_events"]),
            area["title"],
        )
    )
    for index, area in enumerate(areas, start=1):
        area["rank"] = index
    return areas


def build_report(args: argparse.Namespace) -> dict[str, Any]:
    records: list[dict[str, Any]] = []

    benchmark, benchmark_error = load_json(args.benchmark)
    framework, framework_error = load_json(args.framework)
    acceleration, acceleration_error = load_json(args.acceleration)
    inputs = {
        "benchmark_smoke": collect_benchmark(
            benchmark, benchmark_error, args.benchmark, records
        ),
        "framework_smoke": collect_framework(
            framework, framework_error, args.framework, records
        ),
        "acceleration_matrix": collect_acceleration(
            acceleration, acceleration_error, args.acceleration, records
        ),
        "counter_json": collect_counter_files(args.counter_root, records),
    }
    if not records:
        raise SystemExit(
            "no counter evidence found; run `nix develop -c just benchmark-smoke` "
            "or `nix develop -c just verify-performance` first"
        )
    areas = summarize_areas(records)
    return {
        "schema_version": 1,
        "status": "ok",
        "inputs": inputs,
        "record_count": len(records),
        "areas": areas,
        "optional_profilers": optional_profiler_status(args.callgrind),
        "ranking_policy": (
            "Areas are ranked by non-negative VM counter totals from existing "
            "benchmark/framework/acceleration/counter artifacts. Wall-clock "
            "timing is not used for priority."
        ),
        "correctness_policy": (
            "The report is advisory for prioritization only; any optimization "
            "must still prove stdout, stderr/runtime diagnostics, exit status, "
            "fallback counters, and focused fixture parity."
        ),
    }


def table_row(cells: list[str]) -> str:
    return "| " + " | ".join(cells) + " |"


def render_markdown(report: dict[str, Any], *, concise: bool = False) -> str:
    title = "# Fastest Engine Hotpaths" if concise else "# Fastest Engine Hot-Path Report"
    lines = [
        title,
        "",
        "This report ranks engine work from VM counters and existing performance artifacts. Wall-clock timings are not used for priority.",
        "",
        "## Inputs",
        "",
        table_row(["Input", "Status", "Records", "Reason"]),
        table_row(["---", "---", "---:", "---"]),
    ]
    for name, status in report["inputs"].items():
        lines.append(
            table_row(
                [
                    f"`{name}`",
                    f"`{status.get('status')}`",
                    str(status.get("record_count", 0)),
                    str(status.get("reason") or ""),
                ]
            )
        )
    lines.extend(
        [
            "",
            "## Ranked Areas",
            "",
            table_row(["Rank", "Area", "Counter events", "Class", "Top evidence", "Next evidence"]),
            table_row(["---:", "---", "---:", "---", "---", "---"]),
        ]
    )
    areas = report["areas"][:8] if concise else report["areas"]
    for area in areas:
        top = area["top_records"][0] if area["top_records"] else {}
        evidence = (
            f"`{top.get('path')}` via `{top.get('source')}` ({top.get('value', 0)})"
            if top
            else "No current counter events"
        )
        lines.append(
            table_row(
                [
                    str(area["rank"]),
                    area["title"],
                    str(area["total_counter_events"]),
                    f"`{area['rank_class']}`",
                    evidence,
                    area["required_next_evidence"],
                ]
            )
        )
    lines.extend(["", "## Optional Profilers", ""])
    for profiler in report["optional_profilers"]:
        reason = profiler.get("reason")
        suffix = f": {reason}" if reason else ""
        lines.append(
            f"- `{profiler['tool']}`: `{profiler['status']}` at `{profiler['path']}`{suffix}"
        )
    lines.extend(
        [
            "",
            "## Correctness Policy",
            "",
            report["correctness_policy"],
        ]
    )
    return "\n".join(lines)


def write_outputs(report: dict[str, Any], args: argparse.Namespace) -> None:
    args.json_out.parent.mkdir(parents=True, exist_ok=True)
    args.json_out.write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    args.markdown_out.parent.mkdir(parents=True, exist_ok=True)
    args.markdown_out.write_text(render_markdown(report) + "\n", encoding="utf-8")
    args.summary_doc.parent.mkdir(parents=True, exist_ok=True)
    args.summary_doc.write_text(render_markdown(report, concise=True) + "\n", encoding="utf-8")


def self_test() -> int:
    report = {
        "schema_version": 1,
        "status": "ok",
        "inputs": {"self_test": {"status": "ok", "record_count": 2, "reason": None}},
        "record_count": 2,
        "areas": summarize_areas(
            [
                {
                    "source": "self-test",
                    "label": "loop",
                    "path": "loop.php",
                    "counters": {
                        "instructions_executed": 100,
                        "function_calls": 5,
                        "output_bytes": 10,
                    },
                },
                {
                    "source": "self-test",
                    "label": "array",
                    "path": "array.php",
                    "counters": {
                        "array_dim_fetches": 20,
                        "packed_fetch_fast_hits": 20,
                    },
                },
            ]
        ),
        "optional_profilers": [
            {
                "tool": "callgrind",
                "status": "skipped",
                "path": "target/performance/callgrind/summary.json",
                "reason": "self-test",
            }
        ],
        "correctness_policy": "self-test",
    }
    markdown = render_markdown(report)
    if "Dispatch" not in markdown or "Arrays And Foreach" not in markdown:
        print("[fail] fastest hotpath markdown missing expected areas", file=sys.stderr)
        return 1
    if report["areas"][0]["key"] != "dispatch":
        print("[fail] expected dispatch to rank first in self-test", file=sys.stderr)
        return 1
    try:
        flatten_counters({"bad": -1})
    except ValueError:
        pass
    else:
        print("[fail] expected negative counter rejection", file=sys.stderr)
        return 1
    print("[pass] fastest_hotpath_report self-test")
    return 0


def main() -> int:
    args = parse_args()
    if args.self_test:
        return self_test()
    report = build_report(args)
    write_outputs(report, args)
    print(
        "[pass] fastest hotpath report wrote "
        f"{rel(args.json_out)}, {rel(args.markdown_out)}, and {rel(args.summary_doc)}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
