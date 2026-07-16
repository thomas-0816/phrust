#!/usr/bin/env python3
"""Build the complete C13 linkage/footprint tranche without invented data."""

from __future__ import annotations

import argparse
import json
import platform
from pathlib import Path
from typing import Any

MIB = 1024 * 1024
GIB = 1024 * MIB


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=Path("target/post-cutover/linkage-footprint"),
    )
    parser.add_argument("--linkage-after", type=Path)
    parser.add_argument(
        "--smoke-linkage",
        type=Path,
        help="optional synthetic native-smoke linkage report (not acceptance data)",
    )
    parser.add_argument("--footprint-after", type=Path)
    parser.add_argument("--rss-after", type=Path)
    parser.add_argument("--cpu-after", type=Path)
    parser.add_argument(
        "--gate",
        action="append",
        default=[],
        metavar="NAME=STATUS",
        help="record a validation status (passed, failed, or skipped)",
    )
    parser.add_argument(
        "--skip-reason",
        action="append",
        default=[],
        metavar="NAME=REASON",
    )
    return parser.parse_args()


def read_object(path: Path | None, label: str) -> dict[str, Any] | None:
    if path is None:
        return None
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise SystemExit(f"native linkage tranche: cannot read {label}: {error}") from error
    if not isinstance(value, dict):
        raise SystemExit(f"native linkage tranche: {label} must be a JSON object")
    return value


def assignments(values: list[str], label: str) -> dict[str, str]:
    result: dict[str, str] = {}
    for value in values:
        if "=" not in value:
            raise SystemExit(f"native linkage tranche: {label} requires NAME=VALUE: {value}")
        name, item = value.split("=", 1)
        if not name or not item:
            raise SystemExit(f"native linkage tranche: empty {label}: {value}")
        result[name] = item
    return result


def write_json(path: Path, value: Any) -> None:
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def metric(value: Any, target: str) -> str:
    if value is None:
        return f"not measured (target {target})"
    return f"{value} (target {target})"


def main() -> int:
    args = parse_args()
    gates = assignments(args.gate, "gate")
    reasons = assignments(args.skip_reason, "skip reason")
    invalid = sorted(set(gates.values()) - {"passed", "failed", "skipped"})
    if invalid:
        raise SystemExit(
            "native linkage tranche: gate statuses must be passed, failed, or skipped: "
            + ", ".join(invalid)
        )
    output = args.output_dir
    output.mkdir(parents=True, exist_ok=True)

    before_linkage = {
        "schema_version": 1,
        "source": "operator-provided baseline in prompt3.md",
        "native_call_direct": 0,
        "native_call_dynamic": 338_758,
        "native_transition_count": 82_677,
        "call_function_exclusive_seconds_approx": 8.72,
        "method_calls_exclusive_seconds_approx": 1.63,
    }
    linkage_after = read_object(args.linkage_after, "linkage-after report")
    smoke_linkage = read_object(args.smoke_linkage, "smoke-linkage report")
    after_linkage = linkage_after or {
        "schema_version": 1,
        "status": "not_measured",
        "reason": reasons.get(
            "wordpress-root-diagnostics",
            "no instrumented WordPress request was supplied",
        ),
        "calls": None,
        "transitions": None,
    }
    before_artifacts = {
        "schema_version": 1,
        "source": "operator-provided baseline in prompt3.md",
        "artifact_count": 416,
        "artifact_bytes_approx": 348 * MIB,
        "section_bytes": None,
        "section_attribution_status": "unavailable_in_operator_baseline",
    }
    footprint_after = read_object(args.footprint_after, "footprint-after report")
    after_artifacts = footprint_after or {
        "schema_version": 1,
        "status": "not_measured",
        "reason": reasons.get(
            "native-footprint-report",
            "no populated PNA cache directory or live server PID was supplied",
        ),
        "artifact_bytes": None,
        "section_bytes": None,
        "process_memory": None,
    }
    rss_after = read_object(args.rss_after, "RSS-after report")
    rss = {
        "schema_version": 1,
        "source": "operator baseline plus optional current-host measurements",
        "host": {
            "system": platform.system(),
            "release": platform.release(),
            "machine": platform.machine(),
        },
        "before_bytes_approx": {
            "c1": int(5.94 * GIB),
            "c4": int(7.81 * GIB),
            "c8": int(8.10 * GIB),
        },
        "after": rss_after,
        "status": "measured" if rss_after else "not_measured",
        "reason": None
        if rss_after
        else reasons.get("rss-c1-c4-c8", "no comparable c1/c4/c8 server run was supplied"),
    }
    cpu_after = read_object(args.cpu_after, "CPU-after report")
    cpu = {
        "schema_version": 1,
        "source": "operator baseline plus optional current-host measurements",
        "before_cpu_seconds_per_request_approx": {"c1": 15.2, "c4": 19.1, "c8": 26.2},
        "after": cpu_after,
        "status": "measured" if cpu_after else "not_measured",
        "reason": None
        if cpu_after
        else reasons.get(
            "cpu-time-c1-c4-c8", "no comparable c1/c4/c8 server run was supplied"
        ),
    }
    clean_results = {
        "schema_version": 1,
        "instrumentation": "clean gates are separate from diagnostic counter runs",
        "diagnostic_evidence": {
            "native_smoke_linkage": smoke_linkage,
            "acceptance_eligible": False if smoke_linkage is not None else None,
        },
        "gates": {
            name: {"status": status, "reason": reasons.get(name)}
            for name, status in sorted(gates.items())
        },
    }
    merge_contract = {
        "schema_version": 1,
        "runtime_call_abi": {
            "change": "append-only frame flags and frame-arena helper IDs",
            "persistent_process_addresses": False,
            "typed_callsite_descriptor": True,
        },
        "native_cache": {
            "writer": "PNA2",
            "metadata": "PRM4 graph bundle with compact root indices",
            "read_compatibility": ["PNA2/PRM4", "PNA1/PRM3 (one migration window)"],
            "function_entry_abi_identity": "stored once per bundle section",
        },
        "integration": {
            "semantic_coverage_branch": "merge first; typed operations remain authoritative",
            "linkage_branch": "merge second; owns linkage/cache/frame ABI",
            "ssa_branch": "rebase last; no scalar SSA or ownership model duplicated here",
        },
    }

    write_json(output / "clean-results.json", clean_results)
    write_json(output / "call-linkage-before.json", before_linkage)
    write_json(output / "call-linkage-after.json", after_linkage)
    write_json(output / "artifact-breakdown-before.json", before_artifacts)
    write_json(output / "artifact-breakdown-after.json", after_artifacts)
    write_json(output / "rss-c1-c4-c8.json", rss)
    write_json(output / "cpu-time-c1-c4-c8.json", cpu)
    write_json(output / "merge-contract.json", merge_contract)

    calls = after_linkage.get("calls") if isinstance(after_linkage, dict) else None
    direct_ratio = calls.get("direct_ratio") if isinstance(calls, dict) else None
    dynamic = calls.get("dynamic") if isinstance(calls, dict) else None
    transitions = (
        after_linkage.get("transitions", {}).get("total")
        if isinstance(after_linkage.get("transitions"), dict)
        else None
    )
    artifact_bytes = after_artifacts.get("artifact_bytes")
    smoke_calls = smoke_linkage.get("calls") if isinstance(smoke_linkage, dict) else None
    smoke_direct = smoke_calls.get("direct") if isinstance(smoke_calls, dict) else None
    smoke_dynamic = smoke_calls.get("dynamic") if isinstance(smoke_calls, dict) else None
    smoke_duplicates = (
        smoke_linkage.get("code", {}).get("duplicate_function_body_count")
        if isinstance(smoke_linkage, dict)
        and isinstance(smoke_linkage.get("code"), dict)
        else None
    )
    smoke_summary = (
        f"- Synthetic native smoke (not acceptance-eligible): direct={smoke_direct}, "
        f"dynamic={smoke_dynamic}, duplicate bodies={smoke_duplicates}"
        if smoke_linkage is not None
        else "- Synthetic native smoke: not supplied"
    )
    summary = f"""# Native linkage and footprint tranche

This report distinguishes structural implementation evidence from measurements. Missing WordPress or host-comparability inputs remain explicitly unmeasured; they are never treated as passes.

## Current measurements

- Stable direct-call ratio: {metric(direct_ratio, ">= 0.80")}
- Dynamic native calls: {metric(dynamic, "<= 75,000")}
- Native transitions: {metric(transitions, "<= 20,000")}
- Native artifact bytes: {metric(artifact_bytes, f"<= {120 * MIB}")}
- c1/c4/c8 RSS: {rss['status']} — {rss.get('reason') or 'see rss-c1-c4-c8.json'}
- c1/c4/c8 CPU/request: {cpu['status']} — {cpu.get('reason') or 'see cpu-time-c1-c4-c8.json'}

## Structural smoke evidence

{smoke_summary}

## Validation

The exact gate statuses and skip reasons are in `clean-results.json`. Structural results alone do not establish the WordPress performance tranche.
"""
    (output / "summary.md").write_text(summary, encoding="utf-8")
    print(f"native linkage tranche: wrote {output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
