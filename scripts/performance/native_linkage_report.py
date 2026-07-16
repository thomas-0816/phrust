#!/usr/bin/env python3
"""Summarize diagnostic native call linkage without affecting clean timings."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("counters", type=Path, help="VM counters or request-profile JSON")
    parser.add_argument("--output", type=Path)
    return parser.parse_args()


def counter_object(document: Any) -> dict[str, Any]:
    if not isinstance(document, dict):
        raise ValueError("counter document must be a JSON object")
    for key in ("vm_counters", "counters", "native_counters"):
        value = document.get(key)
        if isinstance(value, dict):
            return value
    return document


def integer(counters: dict[str, Any], name: str) -> int:
    value = counters.get(name, 0)
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise ValueError(f"{name} must be numeric")
    return int(value)


def numeric_map(counters: dict[str, Any], name: str) -> dict[str, int]:
    value = counters.get(name, {})
    if not isinstance(value, dict):
        raise ValueError(f"{name} must be an object")
    return {str(key): int(item) for key, item in value.items() if isinstance(item, (int, float))}


def main() -> int:
    args = parse_args()
    try:
        counters = counter_object(json.loads(args.counters.read_text(encoding="utf-8")))
    except (OSError, ValueError, json.JSONDecodeError) as error:
        raise SystemExit(f"native linkage report: {error}") from error
    direct = integer(counters, "native_call_direct")
    dynamic = integer(counters, "native_call_dynamic")
    total = direct + dynamic
    helper_calls = numeric_map(counters, "runtime_helper_calls_by_id")
    helper_time = numeric_map(counters, "runtime_helper_time_nanos_by_id")
    helper_inclusive_time = numeric_map(
        counters, "runtime_helper_inclusive_time_nanos_by_id"
    )
    call_helpers = sorted(
        (
            {
                "helper": helper,
                "calls": helper_calls.get(helper, 0),
                "exclusive_nanos": nanos,
                "inclusive_nanos": helper_inclusive_time.get(helper, nanos),
            }
            for helper, nanos in helper_time.items()
            if helper.startswith("call_")
        ),
        key=lambda item: (-item["exclusive_nanos"], item["helper"]),
    )
    callsite_calls = numeric_map(counters, "native_callsite_calls_by_id")
    callsite_inclusive = numeric_map(
        counters, "native_callsite_inclusive_time_nanos_by_id"
    )
    callsite_exclusive = numeric_map(
        counters, "native_callsite_exclusive_time_nanos_by_id"
    )
    callsites = [
        {
            "callsite": callsite,
            "calls": calls,
            "inclusive_nanos": callsite_inclusive.get(callsite, 0),
            "exclusive_nanos": callsite_exclusive.get(callsite, 0),
        }
        for callsite, calls in callsite_calls.items()
    ]
    report = {
        "schema_version": 1,
        "source": str(args.counters),
        "calls": {
            "total": total,
            "direct": direct,
            "dynamic": dynamic,
            "direct_ratio": direct / total if total else 0.0,
            "same_unit_direct_eligible": integer(counters, "native_same_unit_direct_eligible"),
            "cross_unit_direct_eligible": integer(counters, "native_cross_unit_direct_eligible"),
            "method_monomorphic_eligible": integer(
                counters, "native_method_monomorphic_eligible"
            ),
            "builtin_direct_eligible": integer(counters, "native_builtin_direct_eligible"),
            "dynamic_reasons": numeric_map(counters, "native_call_dynamic_by_reason"),
        },
        "transitions": {
            "total": integer(counters, "native_transition_count"),
            "reasons": numeric_map(counters, "native_transition_by_reason"),
        },
        "allocation": {
            "argument_bytes": integer(counters, "native_call_argument_allocation_bytes"),
            "frame_bytes": integer(counters, "native_call_frame_bytes"),
        },
        "code": {
            "bytes_by_function": numeric_map(
                counters, "native_code_bytes_by_function"
            ),
            "bytes_by_unit": numeric_map(counters, "native_code_bytes_by_unit"),
            "native_stack_bytes_by_function": numeric_map(
                counters, "native_stack_bytes_by_function"
            ),
            "function_body_compile_count": integer(
                counters, "native_function_body_compile_count"
            ),
            "duplicate_function_body_count": integer(
                counters, "native_duplicate_function_body_count"
            ),
        },
        "inlining": {
            "inlined_calls": integer(counters, "native_inlined_calls"),
            "bytes_added": integer(counters, "native_inline_bytes_added"),
            "calls_removed": integer(counters, "native_inline_calls_removed"),
            "tail_calls": integer(counters, "native_tail_calls"),
            "rejected_reasons": numeric_map(
                counters, "native_inline_rejected_by_reason"
            ),
        },
        "top_call_helpers_by_exclusive_time": call_helpers,
        "top_call_helpers_by_inclusive_time": sorted(
            call_helpers,
            key=lambda item: (-item["inclusive_nanos"], item["helper"]),
        ),
        "top_callsites_by_inclusive_time": sorted(
            callsites,
            key=lambda item: (-item["inclusive_nanos"], item["callsite"]),
        ),
        "top_callsites_by_exclusive_time": sorted(
            callsites,
            key=lambda item: (-item["exclusive_nanos"], item["callsite"]),
        ),
    }
    rendered = json.dumps(report, indent=2, sort_keys=True) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(rendered, encoding="utf-8")
    print(rendered, end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
