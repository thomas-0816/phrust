#!/usr/bin/env python3
"""Build the deterministic B9 SSA/lifetime acceptance evidence bundle."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

from native_helper_report import markdown, profile


OPERATOR_BASELINE = {
    "runtime_helper_calls": 4_780_000,
    "runtime_helper_calls_by_id": {
        "local_fetch": 1_020_000,
        "local_store": 322_000,
        "truthy": 515_000,
        "value_retain": 476_000,
        "value_release": 694_000,
        "call_function": 339_000,
    },
    "runtime_helper_object_release_root_scans": 8_855,
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--after", type=Path)
    parser.add_argument(
        "--after-kind",
        choices=("validation-fixture", "wordpress"),
        default="validation-fixture",
    )
    parser.add_argument("--clean", type=Path)
    parser.add_argument(
        "--out-dir", type=Path, default=Path("target/post-cutover/ssa-lifetimes")
    )
    parser.add_argument("--self-test", action="store_true")
    return parser.parse_args()


def read_json(path: Path | None) -> dict[str, Any] | None:
    if path is None or not path.is_file():
        return None
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"{path} must contain a JSON object")
    return value


def write_json(path: Path, value: Any) -> None:
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def ownership_report(document: dict[str, Any] | None, evidence: str) -> dict[str, Any]:
    document = document or {}
    return {
        "schema_version": 1,
        "evidence": evidence,
        "moves": int(document.get("native_ownership_moves", 0)),
        "clones": int(document.get("native_ownership_clones", 0)),
        "escapes": int(document.get("native_ownership_escapes", 0)),
        "retains_by_reason": document.get("runtime_helper_retain_by_reason", {}),
        "releases_by_reason": document.get("runtime_helper_release_by_reason", {}),
        "release_to_zero": int(document.get("runtime_helper_release_to_zero", 0)),
        "value_table": {
            "allocations": int(document.get("native_value_table_allocations", 0)),
            "reuses": int(document.get("native_value_table_reuses", 0)),
            "high_water": int(document.get("native_value_table_high_water", 0)),
        },
        "ssa": {
            "promoted_locals": int(document.get("native_ssa_promoted_locals", 0)),
            "promoted_registers": int(document.get("native_ssa_promoted_registers", 0)),
        },
    }


def root_report(document: dict[str, Any] | None, evidence: str) -> dict[str, Any]:
    document = document or {}
    scans = int(document.get("runtime_helper_object_release_root_scans", 0))
    return {
        "schema_version": 1,
        "evidence": evidence,
        "root_index_rebuilds": scans,
        "maximum": 500,
        "passes_for_this_evidence": scans <= 500,
        "rebuilds_by_reason": document.get(
            "runtime_helper_object_release_root_scans_by_reason", {}
        ),
        "unique_release_fast_paths": int(
            document.get("runtime_helper_object_release_fast_paths", 0)
        ),
        "note": (
            "A validation fixture cannot establish the WordPress tranche threshold."
            if evidence != "wordpress"
            else "Measured on the WordPress acceptance workload."
        ),
    }


def merge_contract() -> dict[str, Any]:
    return {
        "schema_version": 1,
        "base_commit": "da2a058d7b0e8e35ee73876456c5aa5563f0589a",
        "new_helper_ids": [],
        "abi_changes": [
            {
                "kind": "runtime_abi_version",
                "before": 17,
                "after": 18,
                "reason": "reserve immutable encoded false/true handles",
            },
            {
                "kind": "runtime_abi_hash",
                "action": "recompute through JIT_RUNTIME_ABI_HASH",
            },
        ],
        "artifact_format_changed": False,
        "direct_call_linkage_changed": False,
        "worker_stack_sizing_changed": False,
        "shared_files": [
            "crates/php_jit/src/cranelift_lowering.rs",
            "crates/php_jit/src/lib.rs",
            "crates/php_jit/src/region_ir/mod.rs",
            "crates/php_vm/src/vm/jit_abi.rs",
            "crates/php_vm/src/vm/jit_abi/runtime_ops.rs",
            "justfile",
        ],
        "required_rebase_actions": [
            "Rebase after semantic coverage and map newly typed operations into the SSA value/effect lattice.",
            "Rebase after linkage work and teach ownership snapshots about the final direct-call ABI.",
            "Resolve runtime ABI hash/version conflicts without converting typed operations into generic helpers.",
        ],
    }


def clean_results(value: dict[str, Any] | None) -> dict[str, Any]:
    if value is None:
        return {
            "schema_version": 1,
            "status": "unavailable",
            "reason": "WordPress checkout/server/database and clean benchmark input are unavailable",
            "requirements": {
                "warm_c1_p50_improvement_minimum": 0.25,
                "c1_c4_c8_p95_regression_allowed": False,
                "native_code_bytes_growth_maximum": 0.10,
            },
        }
    return {"schema_version": 1, "status": "measured", "results": value}


def summary_text(
    after: dict[str, Any] | None,
    after_kind: str,
    clean: dict[str, Any],
) -> str:
    evidence = "unavailable" if after is None else after_kind
    hard_ready = after is not None and after_kind == "wordpress" and clean["status"] == "measured"
    lines = [
        "# Executable SSA and lifetime tranche",
        "",
        f"Hard-acceptance status: **{'ready for threshold audit' if hard_ready else 'blocked'}**.",
        "",
        f"Helper evidence: `{evidence}`.",
        f"Clean benchmark evidence: `{clean['status']}`.",
        "",
    ]
    if not hard_ready:
        lines.extend(
            [
                "The code-level and fixture evidence in this directory is not a substitute for the required WordPress run. The environment lacks the WordPress checkout, `phrust-server`, and database, so warm c1/c4/c8 latency, helper share, and code-size acceptance remain unproven.",
                "",
            ]
        )
    if after is not None:
        report = profile(after)
        lines.extend(
            [
                "## Available helper evidence",
                "",
                f"- Runtime helper calls: {report['runtime_helper_calls']}",
                f"- Local reads: {report['targets']['local_read_calls']['actual']}",
                f"- Local stores: {report['targets']['local_store_calls']['actual']}",
                f"- Truthy calls: {report['targets']['truthy_calls']['actual']}",
                f"- Retain plus release: {report['targets']['retain_release_calls']['actual']}",
                f"- Root-index rebuilds: {report['targets']['object_root_scans']['actual']}",
                "",
            ]
        )
    lines.extend(
        [
            "## Structural evidence",
            "",
            "`native-ssa-ratchet`, JIT executable tests, native smoke, optimizer differential tests, and the runtime/stdlib gates are the required code-level evidence. See the final task handoff for their exact pass/skip state.",
            "",
        ]
    )
    return "\n".join(lines)


def clif_readme() -> str:
    return """# CLIF sample provenance

Direct scalar and ownership lowering is executable-tested by the `optimizing_` tests in
`crates/php_jit/src/cranelift_lowering/tests.rs`. The ratchet runs those compiled functions
with forbidden helper callbacks, so a source-only or report-only implementation cannot pass.

The repository does not currently expose production CLIF through the CLI. Use
`build_trivial_add_clif_smoke` for a stable textual CLIF sample and the optimizing tests for
production lowering execution. No fabricated production dump is stored here.
"""


def generate(args: argparse.Namespace) -> None:
    after = read_json(args.after)
    clean_input = read_json(args.clean)
    output = args.out_dir
    output.mkdir(parents=True, exist_ok=True)
    (output / "clif-samples").mkdir(exist_ok=True)

    before = profile(OPERATOR_BASELINE)
    before["evidence"] = "operator-provided approximate baseline"
    write_json(output / "baseline-helper-profile.json", before)
    (output / "baseline-helper-profile.md").write_text(
        markdown(before)
        + "\nEvidence: operator-provided approximate baseline; per-reason attribution was not captured.\n",
        encoding="utf-8",
    )
    write_json(output / "helper-before.json", before)

    if after is None:
        after_profile: dict[str, Any] = {
            "schema_version": 1,
            "evidence": "unavailable",
        }
        evidence = "unavailable"
    else:
        after_profile = profile(after)
        after_profile["evidence"] = args.after_kind
        evidence = args.after_kind
    write_json(output / "helper-after.json", after_profile)
    write_json(output / "ownership-report.json", ownership_report(after, evidence))
    write_json(output / "root-index-report.json", root_report(after, evidence))
    clean = clean_results(clean_input)
    write_json(output / "clean-results.json", clean)
    write_json(output / "merge-contract.json", merge_contract())
    (output / "summary.md").write_text(
        summary_text(after, args.after_kind, clean), encoding="utf-8"
    )
    (output / "clif-samples" / "README.md").write_text(
        clif_readme(), encoding="utf-8"
    )


def self_test() -> int:
    assert merge_contract()["artifact_format_changed"] is False
    assert clean_results(None)["status"] == "unavailable"
    assert profile(OPERATOR_BASELINE)["runtime_helper_calls"] == 4_780_000
    print("[pass] native SSA acceptance report self-test")
    return 0


def main() -> int:
    args = parse_args()
    if args.self_test:
        return self_test()
    try:
        generate(args)
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"native SSA acceptance report failed: {error}", file=sys.stderr)
        return 2
    print(args.out_dir)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
