#!/usr/bin/env python3
"""Render a human-readable performance performance report."""

from __future__ import annotations

import argparse
import json
from collections import Counter
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_BENCHMARK = ROOT / "target/performance/benchmark-smoke.json"
DEFAULT_BASELINE = ROOT / "target/performance/baseline.json"
DEFAULT_COMPARE = ROOT / "target/performance/perf-compare.md"
DEFAULT_FRAMEWORK_SMOKE = ROOT / "target/performance/framework-smoke/summary.json"
DEFAULT_OUT = ROOT / "target/performance/perf-report.md"
DEFAULT_JSON_OUT = ROOT / "target/performance/perf-report.json"
KNOWN_GAPS = ROOT / "docs/known-gaps-performance.md"

CACHE_COUNTERS = (
    "cache_hits",
    "cache_misses",
    "internal_function_dispatch_cache_hits",
    "internal_function_dispatch_cache_misses",
    "numeric_string_classify_calls",
    "numeric_string_cache_hits",
    "numeric_string_cache_misses",
    "numeric_string_specialization_hits",
    "numeric_string_warning_sensitive_fallbacks",
    "numeric_string_overflow_precision_fallbacks",
)

QUICKENING_COUNTERS = (
    "quickening_attempts",
    "quickening_specialized",
    "quickening_guard_hits",
    "quickening_guard_misses",
    "quickening_guard_failures",
    "quickening_fallback_calls",
    "quickening_dequickens",
    "quickening_megamorphic",
    "quickening_disabled",
)

INLINE_CACHE_COUNTERS = (
    "inline_cache_slots",
    "inline_cache_hits",
    "inline_cache_misses",
    "inline_cache_invalidations",
    "inline_cache_guard_failures",
    "inline_cache_fallback_calls",
    "inline_cache_monomorphic",
    "inline_cache_polymorphic",
    "function_call_ic_hits",
    "function_call_ic_misses",
    "builtin_call_ic_hits",
    "builtin_call_ic_misses",
    "builtin_fast_stub_hits.count",
    "builtin_fast_stub_hits.strlen",
    "builtin_fast_stub_hits.is_int",
    "builtin_fast_stub_hits.is_string",
    "builtin_fast_stub_hits.is_array",
    "builtin_fast_stub_misses.strlen",
    "builtin_intrinsic_candidates",
    "intrinsic_hits.str_contains",
    "intrinsic_hits.str_starts_with",
    "intrinsic_hits.str_ends_with",
    "intrinsic_hits.strtolower",
    "intrinsic_misses.str_contains",
    "intrinsic_fallback_by_reason.str_contains.type",
    "specialized_builtin_opcode_hits.strlen",
    "call_ic_megamorphic_fallbacks",
    "method_ic_hits",
    "method_ic_misses",
    "property_ic_hits",
    "property_ic_misses",
    "property_ic_fallback_reasons.layout_epoch_mismatch",
    "property_ic_fallback_reasons.receiver_class_mismatch",
    "property_ic_fallback_reasons.uninitialized_typed_property",
    "class_static_ic_hits",
    "class_static_ic_misses",
    "include_path_ic_hits",
    "include_path_ic_misses",
    "include_graph_hits",
    "include_graph_misses",
    "autoload_class_lookup_ic_hits",
    "autoload_class_lookup_ic_misses",
    "autoload_graph_hits",
    "autoload_graph_misses",
    "negative_lookup_hits",
    "invalidations_by_reason.file_fingerprint_changed",
    "fallback_by_path_semantics.missing_path",
    "fallback_by_path_semantics.stream_wrapper",
)

OUTPUT_COUNTERS = (
    "output_bytes",
    "output_buffer_appends",
    "output_buffer_batch_writes",
    "output_batched_appends",
    "output_batch_bytes",
    "output_buffer_flushes",
    "output_fast_appends",
    "concat_prealloc_hits",
)

FRAME_REUSE_COUNTERS = (
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
    "fast_path_disabled_by_reference",
    "dequickened_by_reference",
    "IC_invalidated_by_reference",
    "dense_bytecode_fallback_by_reference",
    "value_clones",
    "string_allocations",
    "array_handle_clones",
    "cow_separations",
    "reference_cell_creations",
    "object_allocations",
)

ARRAY_SHAPE_COUNTERS = (
    "record_shape_hits",
    "record_shape_misses",
    "small_map_hits",
    "small_map_misses",
    "key_coercion_fallbacks",
    "order_semantics_fallbacks",
    "cow_or_reference_fallbacks",
)

FRAMEWORK_COUNTER_COLUMNS = (
    "instructions_executed",
    "function_calls",
    "method_calls",
    "frames_allocated",
    "frames_reused",
    "value_clones",
    "string_allocations",
    "cow_separations",
    "array_dim_fetches",
    "property_accesses",
    "internal_function_dispatches",
    "inline_cache_hits",
    "fast_path_disabled_by_reference",
    "output_bytes",
    "output_fast_appends",
    "output_batched_appends",
    "concat_prealloc_hits",
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--benchmark", type=Path, default=DEFAULT_BENCHMARK)
    parser.add_argument("--baseline", type=Path, default=DEFAULT_BASELINE)
    parser.add_argument("--compare", type=Path, default=DEFAULT_COMPARE)
    parser.add_argument("--framework-smoke", type=Path, default=DEFAULT_FRAMEWORK_SMOKE)
    parser.add_argument("--out", type=Path, default=DEFAULT_OUT)
    parser.add_argument("--json-out", type=Path, default=DEFAULT_JSON_OUT)
    parser.add_argument("--known-gaps", type=Path, default=KNOWN_GAPS)
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


def metric_value(measurement: dict[str, Any], name: str) -> str:
    metrics = measurement.get("metrics", [])
    if not isinstance(metrics, list):
        return "-"
    for metric in metrics:
        if not isinstance(metric, dict) or metric.get("name") != name:
            continue
        value = metric.get("value")
        unit = metric.get("unit")
        if isinstance(value, (int, float)):
            suffix = f" {unit}" if isinstance(unit, str) and unit else ""
            return f"{value:.6g}{suffix}"
    return "-"


def measurement_scenario(measurement: dict[str, Any]) -> dict[str, Any]:
    scenario = measurement.get("scenario")
    return scenario if isinstance(scenario, dict) else {}


def measurement_counters(measurement: dict[str, Any]) -> dict[str, int]:
    counters = measurement.get("vm_counters")
    if not isinstance(counters, dict):
        return {}
    parsed = {}
    for key, value in counters.items():
        if isinstance(key, str) and isinstance(value, int):
            parsed[key] = value
        elif key in {
            "frame_reuse_blocked_by_reason",
            "call_frame_layout_observed",
            "generic_frame_fallback_by_reason",
            "frame_alias_state",
            "alias_state_transitions",
            "builtin_fast_stub_hits",
            "builtin_fast_stub_misses",
            "property_ic_fallback_reasons",
            "property_assign_ic_fallback_reasons",
            "output_slow_appends_by_reason",
            "slow_path_calls_by_reason",
            "array_shape_observed_by_kind",
        } and isinstance(value, dict):
            for reason, count in value.items():
                if isinstance(reason, str) and isinstance(count, int):
                    parsed[f"{key}.{reason}"] = count
    return parsed


def phase_timings(measurement: dict[str, Any]) -> dict[str, float]:
    timings = measurement.get("phase_timings")
    if not isinstance(timings, dict):
        return {}
    phases = timings.get("phases")
    if not isinstance(phases, dict):
        return {}
    parsed: dict[str, float] = {}
    for key, value in phases.items():
        if isinstance(key, str) and isinstance(value, (int, float)):
            parsed[key] = float(value)
    return parsed


def aggregate_phase_timings(measurements: list[dict[str, Any]]) -> Counter[str]:
    totals: Counter[str] = Counter()
    for measurement in measurements:
        totals.update(phase_timings(measurement))
    return totals


def metric_float(measurement: dict[str, Any], name: str) -> float | None:
    metrics = measurement.get("metrics", [])
    if not isinstance(metrics, list):
        return None
    for metric in metrics:
        if not isinstance(metric, dict) or metric.get("name") != name:
            continue
        value = metric.get("value")
        if isinstance(value, (int, float)):
            return float(value)
    return None


def aggregate_counters(measurements: list[dict[str, Any]]) -> Counter[str]:
    totals: Counter[str] = Counter()
    for measurement in measurements:
        totals.update(measurement_counters(measurement))
    return totals


def gap_rows(path: Path) -> list[list[str]]:
    try:
        text = path.read_text(encoding="utf-8")
    except OSError:
        return []
    rows = []
    for line in text.splitlines():
        if not line.startswith("| PERF-GAP-"):
            continue
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        if len(cells) >= 5:
            rows.append(cells[:5])
    return rows


def summarize_report(
    benchmark: dict[str, Any] | None,
    benchmark_error: str | None,
    benchmark_path: Path,
    baseline_path: Path,
    compare_path: Path,
    framework_smoke_path: Path,
    known_gaps_path: Path,
) -> dict[str, Any]:
    measurements = []
    if benchmark is not None and isinstance(benchmark.get("measurements"), list):
        measurements = [
            item for item in benchmark["measurements"] if isinstance(item, dict)
        ]
    rust_measurements = [
        item for item in measurements if item.get("engine") == "rust-vm"
    ]
    counter_totals = aggregate_counters(rust_measurements)
    phase_totals = aggregate_phase_timings(rust_measurements)
    failed = [
        measurement_scenario(item).get("id", "<unknown>")
        for item in measurements
        if item.get("status") not in (None, "pass")
    ]
    framework_smoke, framework_smoke_error = load_json(framework_smoke_path)
    return {
        "schema_version": 1,
        "status": "missing_benchmark" if benchmark_error else "ok",
        "benchmark_input": rel(benchmark_path),
        "benchmark_error": benchmark_error,
        "baseline": {"path": rel(baseline_path), "exists": baseline_path.is_file()},
        "compare": {"path": rel(compare_path), "exists": compare_path.is_file()},
        "framework_smoke": {
            "path": rel(framework_smoke_path),
            "exists": framework_smoke_path.is_file(),
            "error": framework_smoke_error,
            "data": framework_smoke,
        },
        "environment": benchmark.get("environment", {}) if benchmark else {},
        "run_id": benchmark.get("run_id") if benchmark else None,
        "measurement_count": len(measurements),
        "rust_measurement_count": len(rust_measurements),
        "failed_scenarios": failed,
        "counter_hotspots": counter_totals.most_common(15),
        "phase_hotspots": phase_totals.most_common(15),
        "timing_warnings": [
            warning
            for measurement in rust_measurements
            for warning in measurement.get("timing_warnings", [])
            if isinstance(warning, str)
        ],
        "compile_heavy": sorted(
            (
                {
                    "scenario": measurement_scenario(item).get("id", "<unknown>"),
                    "compile_total_ms": metric_float(item, "compile_total_ms"),
                    "execute_ms": metric_float(item, "execute_ms"),
                    "compile_share_percent": metric_float(item, "compile_share_percent"),
                }
                for item in rust_measurements
                if metric_float(item, "compile_total_ms") is not None
            ),
            key=lambda item: float(item.get("compile_total_ms") or 0.0),
            reverse=True,
        )[:10],
        "execute_heavy": sorted(
            (
                {
                    "scenario": measurement_scenario(item).get("id", "<unknown>"),
                    "compile_total_ms": metric_float(item, "compile_total_ms"),
                    "execute_ms": metric_float(item, "execute_ms"),
                    "execute_share_percent": metric_float(item, "execute_share_percent"),
                }
                for item in rust_measurements
                if metric_float(item, "execute_ms") is not None
            ),
            key=lambda item: float(item.get("execute_ms") or 0.0),
            reverse=True,
        )[:10],
        "cache_counters": {key: counter_totals.get(key, 0) for key in CACHE_COUNTERS},
        "quickening_counters": {
            key: counter_totals.get(key, 0) for key in QUICKENING_COUNTERS
        },
        "inline_cache_counters": {
            key: counter_totals.get(key, 0) for key in INLINE_CACHE_COUNTERS
        },
        "output_counters": {key: counter_totals.get(key, 0) for key in OUTPUT_COUNTERS},
        "output_slow_appends_by_reason": dict(
            sorted(
                (
                    (key.removeprefix("output_slow_appends_by_reason."), value)
                    for key, value in counter_totals.items()
                    if key.startswith("output_slow_appends_by_reason.")
                )
            )
        ),
        "slow_path_calls_by_reason": dict(
            sorted(
                (
                    (key.removeprefix("slow_path_calls_by_reason."), value)
                    for key, value in counter_totals.items()
                    if key.startswith("slow_path_calls_by_reason.")
                )
            )
        ),
        "frame_reuse_counters": {
            key: counter_totals.get(key, 0) for key in FRAME_REUSE_COUNTERS
        },
        "frame_reuse_blocked_by_reason": dict(
            sorted(
                (
                    (key.removeprefix("frame_reuse_blocked_by_reason."), value)
                    for key, value in counter_totals.items()
                    if key.startswith("frame_reuse_blocked_by_reason.")
                )
            )
        ),
        "call_frame_layout_observed": dict(
            sorted(
                (
                    (key.removeprefix("call_frame_layout_observed."), value)
                    for key, value in counter_totals.items()
                    if key.startswith("call_frame_layout_observed.")
                )
            )
        ),
        "generic_frame_fallback_by_reason": dict(
            sorted(
                (
                    (key.removeprefix("generic_frame_fallback_by_reason."), value)
                    for key, value in counter_totals.items()
                    if key.startswith("generic_frame_fallback_by_reason.")
                )
            )
        ),
        "array_shape_counters": {
            key: counter_totals.get(key, 0) for key in ARRAY_SHAPE_COUNTERS
        },
        "array_shape_observed_by_kind": dict(
            sorted(
                (
                    (key.removeprefix("array_shape_observed_by_kind."), value)
                    for key, value in counter_totals.items()
                    if key.startswith("array_shape_observed_by_kind.")
                )
            )
        ),
        "known_gaps": gap_rows(known_gaps_path),
        "measurements": measurements,
    }


def md_table(headers: list[str], rows: list[list[str]]) -> list[str]:
    lines = [
        "| " + " | ".join(headers) + " |",
        "| " + " | ".join("---" for _ in headers) + " |",
    ]
    lines.extend("| " + " | ".join(row) + " |" for row in rows)
    return lines


def render_counter_table(title: str, counters: dict[str, int]) -> list[str]:
    lines = [f"## {title}", ""]
    if not counters:
        return lines + ["No counters available.", ""]
    rows = [[f"`{key}`", str(value)] for key, value in sorted(counters.items())]
    return lines + md_table(["Counter", "Total"], rows) + [""]


def render_markdown(summary: dict[str, Any]) -> str:
    env = summary["environment"] if isinstance(summary["environment"], dict) else {}
    extra = env.get("extra") if isinstance(env.get("extra"), dict) else {}
    opt_flags = env.get("opt_flags") if isinstance(env.get("opt_flags"), list) else []
    feature_flags = (
        env.get("feature_flags") if isinstance(env.get("feature_flags"), dict) else {}
    )

    lines = [
        "# performance Performance Report",
        "",
        "This report summarizes local performance benchmark JSON and VM counters.",
        "It does not compare timings unless a separate `perf-compare` report exists.",
        "",
        "## Inputs",
        "",
        "| Input | Status |",
        "| --- | --- |",
    ]
    if summary["benchmark_error"]:
        lines.append(f"| Benchmark JSON | {summary['benchmark_error']} |")
    else:
        lines.append(f"| Benchmark JSON | `{summary['benchmark_input']}` |")
    baseline_status = "present" if summary["baseline"]["exists"] else "missing"
    compare_status = "present" if summary["compare"]["exists"] else "missing"
    lines.extend(
        [
            f"| Baseline | `{summary['baseline']['path']}` ({baseline_status}) |",
            f"| Comparison | `{summary['compare']['path']}` ({compare_status}) |",
            "",
        ]
    )

    if summary["benchmark_error"]:
        lines.extend(
            [
                "## Missing Benchmarks",
                "",
                "No scenario table or counter summary can be rendered yet.",
                "Run `nix develop -c just benchmark-smoke` or "
                "`nix develop -c just perf-baseline`, then rerun "
                "`nix develop -c just perf-report`.",
                "",
            ]
        )
    lines.extend(
        [
            "## Environment",
            "",
            "| Field | Value |",
            "| --- | --- |",
            f"| Run ID | `{summary.get('run_id') or '-'}` |",
            f"| Engine version | `{env.get('engine_version', '-')}` |",
            f"| Git commit | `{env.get('git_commit') or '-'}` |",
            f"| Rust target | `{env.get('rust_target_triple', '-')}` |",
            f"| Opt flags | `{', '.join(str(flag) for flag in opt_flags) or '-'}` |",
            f"| Feature flags | `{json.dumps(feature_flags, sort_keys=True)}` |",
            f"| TZ | `{extra.get('tz', extra.get('TZ', '-'))}` |",
            f"| LC_ALL | `{extra.get('lc_all', extra.get('LC_ALL', '-'))}` |",
            f"| Platform | `{extra.get('platform', '-')}` |",
            "",
        ]
    )

    measurement_rows = []
    for measurement in summary["measurements"]:
        scenario = measurement_scenario(measurement)
        measurement_rows.append(
            [
                f"`{scenario.get('id', '<unknown>')}`",
                f"`{measurement.get('engine', '-')}`",
                str(measurement.get("status", "-")),
                str(measurement.get("iterations", "-")),
                str(measurement.get("warmups", "-")),
                metric_value(measurement, "wall_time_median"),
                metric_value(measurement, "stdout_bytes"),
                str(len(measurement_counters(measurement))),
            ]
        )
    lines.extend(["## Scenarios", ""])
    if measurement_rows:
        lines.extend(
            md_table(
                [
                    "Scenario",
                    "Engine",
                    "Status",
                    "Iterations",
                    "Warmups",
                    "Median",
                    "Stdout",
                    "Counter keys",
                ],
                measurement_rows,
            )
        )
    else:
        lines.append("No measurements were available.")
    lines.append("")

    lines.extend(["## Counter Hotspots", ""])
    if summary["counter_hotspots"]:
        lines.extend(
            md_table(
                ["Counter", "Total"],
                [[f"`{key}`", str(value)] for key, value in summary["counter_hotspots"]],
            )
        )
    else:
        lines.append("No Rust VM counters were available.")
    lines.append("")

    lines.extend(["## Phase Timing Hotspots", ""])
    if summary["phase_hotspots"]:
        lines.extend(
            md_table(
                ["Phase", "Total ms"],
                [[f"`{key}`", f"{value:.3f}"] for key, value in summary["phase_hotspots"]],
            )
        )
    else:
        lines.append("No Rust VM phase timings were available.")
    lines.append("")

    lines.extend(["## Compile-Heavy Rows", ""])
    if summary["compile_heavy"]:
        lines.extend(
            md_table(
                ["Scenario", "Compile ms", "Execute ms", "Compile share"],
                [
                    [
                        f"`{row['scenario']}`",
                        f"{float(row.get('compile_total_ms') or 0.0):.3f}",
                        f"{float(row.get('execute_ms') or 0.0):.3f}",
                        f"{float(row.get('compile_share_percent') or 0.0):.1f}%",
                    ]
                    for row in summary["compile_heavy"]
                ],
            )
        )
    else:
        lines.append("No compile phase metrics were available.")
    lines.append("")

    lines.extend(["## Execute-Heavy Rows", ""])
    if summary["execute_heavy"]:
        lines.extend(
            md_table(
                ["Scenario", "Execute ms", "Compile ms", "Execute share"],
                [
                    [
                        f"`{row['scenario']}`",
                        f"{float(row.get('execute_ms') or 0.0):.3f}",
                        f"{float(row.get('compile_total_ms') or 0.0):.3f}",
                        f"{float(row.get('execute_share_percent') or 0.0):.1f}%",
                    ]
                    for row in summary["execute_heavy"]
                ],
            )
        )
    else:
        lines.append("No execute phase metrics were available.")
    lines.append("")

    lines.extend(["## Timing Warnings", ""])
    if summary["timing_warnings"]:
        for warning in summary["timing_warnings"]:
            lines.append(f"- {warning}")
    else:
        lines.append("No missing or malformed timing sidecars were reported.")
    lines.append("")

    lines.extend(render_counter_table("Cache Hits and Misses", summary["cache_counters"]))
    lines.extend(render_counter_table("Quickening Counters", summary["quickening_counters"]))
    lines.extend(
        render_counter_table("Inline Cache Hits and Misses", summary["inline_cache_counters"])
    )
    lines.extend(render_counter_table("Output Fast Paths", summary["output_counters"]))
    lines.extend(
        render_counter_table(
            "Output Slow Append Reasons",
            summary["output_slow_appends_by_reason"],
        )
    )
    lines.extend(
        render_counter_table(
            "Slow Path Calls By Reason",
            summary["slow_path_calls_by_reason"],
        )
    )
    lines.extend(render_counter_table("Frame and Register Reuse", summary["frame_reuse_counters"]))
    lines.extend(
        render_counter_table(
            "Frame Reuse Blocked Reasons",
            summary["frame_reuse_blocked_by_reason"],
        )
    )
    lines.extend(
        render_counter_table(
            "Call Frame Layouts Observed",
            summary["call_frame_layout_observed"],
        )
    )
    lines.extend(
        render_counter_table(
            "Specialized Frame Fallback Reasons",
            summary["generic_frame_fallback_by_reason"],
        )
    )
    lines.extend(render_counter_table("Array Shape Fast Paths", summary["array_shape_counters"]))
    lines.extend(
        render_counter_table(
            "Array Shapes Observed",
            summary["array_shape_observed_by_kind"],
        )
    )

    framework = summary.get("framework_smoke")
    lines.extend(["## Framework Micro-Smokes", ""])
    if isinstance(framework, dict) and framework.get("data"):
        data = framework["data"]
        scenarios = data.get("scenarios") if isinstance(data, dict) else None
        rows = []
        if isinstance(scenarios, list):
            for scenario in scenarios:
                if not isinstance(scenario, dict):
                    continue
                focus = scenario.get("counter_focus")
                if not isinstance(focus, dict):
                    focus = {}
                rows.append(
                    [
                        f"`{scenario.get('id', '<unknown>')}`",
                        *[str(focus.get(name, "-")) for name in FRAMEWORK_COUNTER_COLUMNS],
                    ]
                )
        if rows:
            lines.extend(
                md_table(
                    [
                        "Scenario",
                        "Instructions",
                        "Function calls",
                        "Method calls",
                        "Frames allocated",
                        "Frames reused",
                        "Value clones",
                        "String allocs",
                        "COW separations",
                        "Array fetches",
                        "Property access",
                        "Builtin calls",
                        "IC hits",
                        "Output bytes",
                        "Output fast appends",
                    ],
                    rows,
                )
            )
        else:
            lines.append("Framework smoke summary exists but contains no scenarios.")
    elif isinstance(framework, dict):
        lines.append(
            f"No framework smoke summary found. Run `nix develop -c just framework-smoke` "
            f"to create `{framework.get('path', DEFAULT_FRAMEWORK_SMOKE)}`."
        )
    else:
        lines.append("No framework smoke summary found.")
    lines.append("")

    lines.extend(["## Perf Compare", ""])
    if summary["compare"]["exists"]:
        lines.append(f"Latest comparison report: `{summary['compare']['path']}`.")
    elif summary["baseline"]["exists"]:
        lines.append(
            "Baseline exists. Run `nix develop -c just perf-compare` to create a "
            "separate timing comparison report."
        )
    else:
        lines.append(
            "No baseline is available. Run `nix develop -c just perf-baseline` "
            "before comparing local timing trends."
        )
    lines.append("")

    lines.extend(["## Known Gaps", ""])
    if summary["known_gaps"]:
        rows = [
            [f"`{row[0]}`", row[1], row[2], row[3], row[4]]
            for row in summary["known_gaps"]
        ]
        lines.extend(md_table(["Gap", "Layer", "Evidence", "Risk", "Handoff"], rows))
    else:
        lines.append("No performance known gaps were listed.")
    lines.append("")
    return "\n".join(lines)


def write_outputs(summary: dict[str, Any], out: Path, json_out: Path) -> None:
    out.parent.mkdir(parents=True, exist_ok=True)
    json_out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(render_markdown(summary), encoding="utf-8")
    json_out.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def run_self_test() -> int:
    report = {
        "schema_version": 1,
        "run_id": "performance-report-self-test",
        "environment": {
            "engine_version": "fixture",
            "git_commit": "abc123",
            "rust_target_triple": "fixture-target",
            "opt_flags": ["--opt-level=1"],
            "feature_flags": {"cranelift": True},
            "extra": {"tz": "UTC", "lc_all": "C", "platform": "fixture"},
        },
        "measurements": [
            {
                "scenario": {
                    "id": "performance.perf_smoke.rust-vm.loop",
                    "name": "loop",
                    "group": "perf_smoke.rust-vm",
                    "fixture": "loop.php",
                },
                "engine": "rust-vm",
                "status": "pass",
                "iterations": 1,
                "warmups": 0,
                "metrics": [
                    {
                        "name": "wall_time_median",
                        "unit": "ms",
                        "value": 1.25,
                        "lower_is_better": True,
                    },
                    {
                        "name": "compile_total_ms",
                        "unit": "ms",
                        "value": 0.75,
                        "lower_is_better": True,
                    },
                    {
                        "name": "execute_ms",
                        "unit": "ms",
                        "value": 0.25,
                        "lower_is_better": True,
                    },
                    {
                        "name": "compile_share_percent",
                        "unit": "percent",
                        "value": 75.0,
                        "lower_is_better": True,
                    },
                    {
                        "name": "execute_share_percent",
                        "unit": "percent",
                        "value": 25.0,
                        "lower_is_better": True,
                    }
                ],
                "phase_timings": {
                    "phases": {"frontend_analyze_ms": 0.5, "execute_ms": 0.25}
                },
                "vm_counters": {
                    "instructions_executed": 10,
                    "cache_hits": 2,
                    "cache_misses": 1,
                    "quickening_guard_hits": 3,
                    "inline_cache_hits": 4,
                    "inline_cache_misses": 1,
                    "array_shape_observed_by_kind": {
                        "shape_stable_record_like": 2,
                        "small_inline_map": 1,
                    },
                    "record_shape_hits": 5,
                    "record_shape_misses": 1,
                    "small_map_hits": 2,
                    "key_coercion_fallbacks": 1,
                },
            }
        ],
    }
    summary = summarize_report(
        report,
        None,
        Path("fixture.json"),
        Path("missing"),
        Path("missing"),
        Path("missing"),
        KNOWN_GAPS,
    )
    markdown = render_markdown(summary)
    assert "performance Performance Report" in markdown
    assert "performance.perf_smoke.rust-vm.loop" in markdown
    assert "Cache Hits and Misses" in markdown
    assert "Phase Timing Hotspots" in markdown
    assert summary["phase_hotspots"][0][0] == "frontend_analyze_ms"
    assert summary["compile_heavy"][0]["compile_total_ms"] == 0.75
    assert "Array Shape Fast Paths" in markdown
    assert summary["array_shape_observed_by_kind"]["shape_stable_record_like"] == 2
    assert summary["array_shape_counters"]["record_shape_hits"] == 5
    assert summary["cache_counters"]["cache_hits"] == 2

    missing = summarize_report(
        None,
        "missing: fixture.json",
        Path("fixture.json"),
        Path("missing"),
        Path("missing"),
        Path("missing"),
        KNOWN_GAPS,
    )
    missing_markdown = render_markdown(missing)
    assert "Missing Benchmarks" in missing_markdown
    assert "benchmark-smoke" in missing_markdown
    print("[pass] perf_report self-test")
    return 0


def main() -> int:
    args = parse_args()
    if args.self_test:
        return run_self_test()
    benchmark, benchmark_error = load_json(args.benchmark)
    summary = summarize_report(
        benchmark,
        benchmark_error,
        args.benchmark,
        args.baseline,
        args.compare,
        args.framework_smoke,
        args.known_gaps,
    )
    write_outputs(summary, args.out, args.json_out)
    if benchmark_error:
        print(
            f"[warn] performance perf report wrote {rel(args.out)} without benchmark "
            f"data: {benchmark_error}"
        )
    else:
        print(f"[pass] performance perf report wrote {rel(args.out)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
