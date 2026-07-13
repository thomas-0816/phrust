#!/usr/bin/env python3
"""Runtime-layout tranche smoke: counter existence, parity, and local ratchet.

Runs every app-flow fixture on the managed default engine, verifies stdout
parity against the baseline compatibility preset (correctness first: rows
that fail parity report no counters), asserts that the runtime-layout
tranche counters exist and fire on the scenarios that exercise them, and
compares counter families against a local baseline when one exists.

Counter-family regressions are reported by default; hard failure on
regression is opt-in via PHRUST_RATCHET_ENFORCE=1 so local and release
gates can choose strictness without creating flaky CI defaults.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT / "tests/fixtures/performance/app_flows"
SCENARIOS = [
    "collection_transform_pagination",
    "config_bootstrap_merge",
    "dependency_container_resolution",
    "front_controller_routing",
    "middleware_event_pipeline",
    "model_hydration_json",
    "request_validation_errors",
    "session_auth_policy",
    "template_render_escape",
    "translation_lookup_interpolation",
]

# Counters introduced by the runtime-layout tranche that must exist in every
# counter snapshot (value may be zero on scenarios that never hit them).
TRANCHE_COUNTERS = [
    "intrinsic_hits",
    "intrinsic_fallback_by_reason",
    "json_encode_fast_path_hits",
    "json_encode_fast_path_bytes",
    "json_encode_generic_fallback_by_reason",
    "array_slice_packed_fast_hits",
    "count_array_shape_fast_hits",
    "map_update_slot_fast_hits",
    "array_builtin_fast_fallback_by_reason",
    "dense_callable_call_hits",
    "record_lookup_fast_hits",
    "record_lookup_key_miss_exits",
    "record_lookup_layout_exits",
    "superinstructions_executed",
    "sort_callback_direct_call_hits",
    "method_inline_candidates",
    "direct_arg_frame_hits",
    "foreach_no_clone_hits",
    "record_slot_reads",
    "packed_values_storage_reads",
    "symbol_intern_hits",
    "string_hash_cache_hits",
]

# Per-scenario floors: the tranche fast paths that must actually fire.
SCENARIO_FLOORS: dict[str, dict[str, int]] = {
    "template_render_escape": {"intrinsic_hits.htmlspecialchars_default": 1},
    "config_bootstrap_merge": {
        "intrinsic_hits.explode_single_byte": 1,
        "record_slot_reads": 1,
    },
    "model_hydration_json": {
        "intrinsic_hits.strtoupper_ascii": 1,
        "json_encode_fast_path_hits": 1,
    },
    "collection_transform_pagination": {
        "array_slice_packed_fast_hits": 1,
        "map_update_slot_fast_hits": 1,
        "sort_callback_direct_call_hits": 1,
    },
    "session_auth_policy": {
        "count_array_shape_fast_hits": 1,
        "map_update_slot_fast_hits": 1,
    },
    "middleware_event_pipeline": {
        "dense_call_ic_hits": 1,
        "superinstructions_executed.load_local_load_const": 1,
    },
    "front_controller_routing": {
        "superinstructions_executed.load_const_fetch_dim": 1,
    },
    "translation_lookup_interpolation": {"intrinsic_hits.strlen": 1},
}

# Counter families tracked for the local ratchet delta report. Lower is
# better for cost families; higher is better for hit families.
COST_FAMILIES = [
    "value_clones",
    "string_allocations",
    "array_handle_clones",
    "cow_separations",
    "quickening_attempts",
    "rich_fallback_functions_executed",
]
HIT_FAMILIES = [
    "map_update_slot_fast_hits",
    "json_encode_fast_path_hits",
    "array_slice_packed_fast_hits",
    "count_array_shape_fast_hits",
    "dense_functions_executed",
    "dense_call_ic_hits",
    "sort_callback_direct_call_hits",
]


def rel(path: Path) -> str:
    try:
        return str(path.relative_to(ROOT))
    except ValueError:
        return str(path)


def run_engine(engine: Path, args: list[str], timeout: float) -> subprocess.CompletedProcess:
    return subprocess.run(
        [str(engine), *args],
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
        check=False,
    )


def run_with_retries(engine: Path, args: list[str], timeout: float, attempts: int = 3):
    last_error: Exception | None = None
    for _ in range(attempts):
        try:
            return run_engine(engine, args, timeout)
        except subprocess.TimeoutExpired as error:  # host first-exec stalls
            last_error = error
            time.sleep(1.0)
    raise SystemExit(f"[error] engine kept timing out: {last_error}")


def nested_get(counters: dict[str, Any], dotted: str) -> int:
    if "." in dotted:
        outer, inner = dotted.split(".", 1)
        value = counters.get(outer)
        if isinstance(value, dict):
            return int(value.get(inner, 0) or 0)
        return 0
    value = counters.get(dotted, 0)
    if isinstance(value, dict):
        return sum(int(v) for v in value.values())
    return int(value or 0)


def collect(engine: Path, out_dir: Path, timeout: float) -> tuple[list[dict[str, Any]], list[str]]:
    rows: list[dict[str, Any]] = []
    failures: list[str] = []
    # Warm the binary once; fresh executables can stall on first exec.
    run_with_retries(engine, ["run", "/dev/null"], timeout)
    for scenario in SCENARIOS:
        fixture = FIXTURES / f"{scenario}.php"
        counters_path = out_dir / f"{scenario}.counters.json"
        managed = run_with_retries(
            engine,
            ["run", "--counters-json", str(counters_path), str(fixture)],
            timeout,
        )
        baseline = run_with_retries(
            engine,
            ["run", "--engine-preset=baseline", str(fixture)],
            timeout,
        )
        row: dict[str, Any] = {"scenario": scenario}
        # Correctness first: no counter reporting without preset parity.
        if (
            managed.returncode != baseline.returncode
            or managed.stdout != baseline.stdout
        ):
            row["status"] = "parity_failure"
            failures.append(
                f"{scenario}: managed default output diverges from baseline preset"
            )
            rows.append(row)
            continue
        counters = json.loads(counters_path.read_text(encoding="utf-8"))
        missing = [name for name in TRANCHE_COUNTERS if name not in counters]
        if missing:
            row["status"] = "missing_counters"
            failures.append(f"{scenario}: missing tranche counters {missing}")
            rows.append(row)
            continue
        floor_failures = []
        for dotted, floor in SCENARIO_FLOORS.get(scenario, {}).items():
            actual = nested_get(counters, dotted)
            if actual < floor:
                floor_failures.append(f"{dotted}={actual} (< {floor})")
        if floor_failures:
            row["status"] = "floor_failure"
            failures.append(f"{scenario}: {', '.join(floor_failures)}")
            rows.append(row)
            continue
        row["status"] = "pass"
        row["families"] = {
            name: nested_get(counters, name) for name in COST_FAMILIES + HIT_FAMILIES
        }
        rows.append(row)
    return rows, failures


def compare_baseline(rows: list[dict[str, Any]], baseline_path: Path) -> dict[str, Any]:
    if not baseline_path.is_file():
        return {"status": "no_baseline", "regressions": [], "improvements": []}
    baseline = json.loads(baseline_path.read_text(encoding="utf-8"))
    baseline_rows = {
        row["scenario"]: row
        for row in baseline.get("rows", [])
        if row.get("status") == "pass"
    }
    regressions: list[str] = []
    improvements: list[str] = []
    for row in rows:
        if row.get("status") != "pass":
            continue
        before = baseline_rows.get(row["scenario"])
        if not before:
            continue
        for family, current in row["families"].items():
            previous = int(before.get("families", {}).get(family, 0) or 0)
            if family in COST_FAMILIES and current > previous:
                regressions.append(
                    f"{row['scenario']}: {family} {previous} -> {current}"
                )
            elif family in HIT_FAMILIES and current < previous:
                regressions.append(
                    f"{row['scenario']}: {family} {previous} -> {current}"
                )
            elif current != previous:
                improvements.append(
                    f"{row['scenario']}: {family} {previous} -> {current}"
                )
    return {
        "status": "compared",
        "regressions": regressions,
        "improvements": improvements,
    }


def render_markdown(report: dict[str, Any]) -> str:
    lines = [
        "# Runtime-Layout Tranche Smoke",
        "",
        "Generated by `nix develop -c just runtime-layout-performance-smoke`.",
        "Raw counter artifacts stay local under",
        "`target/performance/runtime-layout/`.",
        "",
        "| Scenario | Status | map updates | json fast | slice fast | cow seps | value clones |",
        "| --- | --- | --- | --- | --- | --- | --- |",
    ]
    for row in report["rows"]:
        families = row.get("families", {})
        lines.append(
            "| `{}` | {} | {} | {} | {} | {} | {} |".format(
                row["scenario"],
                row["status"],
                families.get("map_update_slot_fast_hits", "-"),
                families.get("json_encode_fast_path_hits", "-"),
                families.get("array_slice_packed_fast_hits", "-"),
                families.get("cow_separations", "-"),
                families.get("value_clones", "-"),
            )
        )
    ratchet = report["ratchet"]
    lines.extend(["", f"Ratchet: {ratchet['status']}"])
    for item in ratchet["regressions"]:
        lines.append(f"- regression: {item}")
    for item in ratchet["improvements"][:20]:
        lines.append(f"- delta: {item}")
    lines.append("")
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--engine", type=Path, required=True)
    parser.add_argument(
        "--out",
        type=Path,
        default=ROOT / "target/performance/runtime-layout/current.json",
    )
    parser.add_argument(
        "--markdown-out",
        type=Path,
        default=ROOT / "target/performance/runtime-layout/current.md",
    )
    parser.add_argument(
        "--baseline",
        type=Path,
        default=ROOT / "target/performance/runtime-layout/baseline.json",
    )
    parser.add_argument("--write-baseline", action="store_true")
    parser.add_argument("--timeout", type=float, default=60.0)
    args = parser.parse_args()

    engine = args.engine if args.engine.is_absolute() else ROOT / args.engine
    if not engine.is_file():
        raise SystemExit(f"[error] missing engine binary: {rel(engine)}")

    out_dir = args.out.parent
    out_dir.mkdir(parents=True, exist_ok=True)
    rows, failures = collect(engine, out_dir, args.timeout)
    ratchet = compare_baseline(rows, args.baseline)
    report = {
        "gate": "runtime-layout-performance-smoke",
        "status": "fail" if failures else "pass",
        "rows": rows,
        "failures": failures,
        "ratchet": ratchet,
    }
    args.out.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    args.markdown_out.write_text(render_markdown(report), encoding="utf-8")
    if args.write_baseline:
        args.baseline.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print(f"[ok] wrote runtime-layout baseline to {rel(args.baseline)}")

    for failure in failures:
        print(f"[error] {failure}", file=sys.stderr)
    for regression in ratchet["regressions"]:
        print(f"[warn] counter regression: {regression}", file=sys.stderr)
    if failures:
        return 1
    if ratchet["regressions"] and os.environ.get("PHRUST_RATCHET_ENFORCE") == "1":
        print(
            "[error] counter regressions rejected (PHRUST_RATCHET_ENFORCE=1)",
            file=sys.stderr,
        )
        return 1
    print(
        f"[pass] runtime-layout smoke checked {len(rows)} scenario(s); "
        f"ratchet {ratchet['status']} with {len(ratchet['regressions'])} regression(s); "
        f"wrote {rel(args.out)} and {rel(args.markdown_out)}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
