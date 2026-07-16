#!/usr/bin/env python3
"""Assemble the PHP 8.5 native-surface acceptance tranche."""

from __future__ import annotations

import argparse
import json
import re
import shutil
import sys
from collections import Counter
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
INVENTORY = ROOT / "target/native-surface"
OUT = ROOT / "target/post-cutover/php85-coverage"
MANIFEST = ROOT / "tests/oracle/manifests/generated-probes.jsonl"
BASE_COMMIT = "da2a058d7b0e8e35ee73876456c5aa5563f0589a"
SUPPORTED = {
    "native_direct",
    "native_typed_runtime",
    "native_generic_builtin",
    "environmentally_unavailable_reference_identical",
}


def load_jsonl(path: Path) -> list[dict]:
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line]


def write_json(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def latest_oracle_report() -> tuple[Path | None, dict | None]:
    reports = list((ROOT / "target/oracle/probes").glob("*/runtime-semantics-diff-report.json"))
    if not reports:
        return None, None
    # A focused rerun is often newer than the complete probe sweep. Prefer
    # breadth first, then freshness, so acceptance cannot silently shrink to
    # the final handful of failures from an implementation wave.
    path = max(
        reports,
        key=lambda candidate: (
            report_selected(candidate),
            candidate.stat().st_mtime_ns,
        ),
    )
    return path, json.loads(path.read_text(encoding="utf-8"))


def report_selected(path: Path) -> int:
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return -1
    return int(payload.get("selected") or payload.get("summary", {}).get("total") or 0)


def load_report(path: Path) -> dict | None:
    if not path.is_file():
        return None
    return json.loads(path.read_text(encoding="utf-8"))


def result_key(result: dict) -> str:
    metadata = result.get("metadata") or {}
    return str(
        metadata.get("oracle_probe_id")
        or result.get("fixture_id")
        or result.get("file")
        or result.get("path")
        or ""
    )


def report_delta(before: dict, after: dict) -> dict:
    before_by_key = {result_key(result): result for result in before.get("results", [])}
    after_by_key = {result_key(result): result for result in after.get("results", [])}
    common = sorted(set(before_by_key) & set(after_by_key))
    transitions = Counter(
        f"{before_by_key[key].get('status', 'unknown')}->{after_by_key[key].get('status', 'unknown')}"
        for key in common
    )
    regressions = [
        key
        for key in common
        if before_by_key[key].get("status") == "pass"
        and after_by_key[key].get("status") != "pass"
    ]
    new_passing = [
        key
        for key in common
        if before_by_key[key].get("status") != "pass"
        and after_by_key[key].get("status") == "pass"
    ]
    remaining = Counter(
        str(result.get("failure_category") or "unclassified")
        for result in after.get("results", [])
        if result.get("status") == "fail"
    )
    return {
        "before_summary": before.get("summary"),
        "after_summary": after.get("summary"),
        "common_probes": len(common),
        "added_probes": len(set(after_by_key) - set(before_by_key)),
        "removed_probes": len(set(before_by_key) - set(after_by_key)),
        "transitions": dict(sorted(transitions.items())),
        "new_passing_probes": len(new_passing),
        "new_passing_probe_ids": new_passing[:100],
        "regressions": len(regressions),
        "regression_probe_ids": regressions[:100],
        "remaining_gap_families": dict(sorted(remaining.items())),
    }


def implementation_waves() -> list[dict]:
    specifications = [
        (
            "generic-builtin-contract-and-behavior",
            ROOT / "target/oracle/probes/full-current/runtime-semantics-diff-report.json",
            ROOT / "target/oracle/probes/full-current-2/runtime-semantics-diff-report.json",
        ),
        (
            "internal-class-method-property-descriptors",
            ROOT / "target/oracle/probes/full-current-2/runtime-semantics-diff-report.json",
            ROOT / "target/oracle/probes/full-current-3/runtime-semantics-diff-report.json",
        ),
    ]
    waves = []
    for name, before_path, after_path in specifications:
        before = load_report(before_path)
        after = load_report(after_path)
        if before is None or after is None:
            waves.append(
                {
                    "name": name,
                    "status": "skipped",
                    "reason": "required before/after oracle report is unavailable",
                }
            )
            continue
        waves.append(
            {
                "name": name,
                "status": "available",
                "before_report": before_path.relative_to(ROOT).as_posix(),
                "after_report": after_path.relative_to(ROOT).as_posix(),
                **report_delta(before, after),
                # Generated API probes are not PHPT executions; keep that
                # evidence channel explicit instead of inventing a delta.
                "new_passing_phpts": None,
            }
        )
    return waves


def runtime_corpus_evidence() -> dict:
    path = ROOT / "target/runtime-semantics/php85-coverage-current/runtime-semantics-diff-report.json"
    report = load_report(path)
    if report is None:
        return {"status": "skipped", "reason": "current runtime corpus report is unavailable"}
    runtime_sources = [
        ROOT / "crates/php_runtime/src/builtins",
        ROOT / "crates/php_runtime/src/datetime.rs",
        ROOT / "crates/php_vm/src/vm/jit_abi",
    ]
    newest_source = max(
        path.stat().st_mtime_ns
        for source in runtime_sources
        for path in ([source] if source.is_file() else source.rglob("*.rs"))
    )
    summary = report.get("summary") or {}
    stale = path.stat().st_mtime_ns < newest_source
    failing = int(summary.get("fail") or 0)
    return {
        "status": "stale" if stale else ("fail" if failing else "available"),
        "reason": (
            "runtime corpus result predates runtime implementation changes"
            if stale
            else (f"runtime corpus contains {failing} failure(s)" if failing else None)
        ),
        "report": path.relative_to(ROOT).as_posix(),
        "summary": summary,
    }


def phpt_evidence() -> dict:
    runs = list((ROOT / "target/phpt-work/full-runs").glob("*/results.jsonl"))
    if not runs:
        return {
            "status": "skipped",
            "reason": "no full PHPT result set is available in target",
            "regressions": None,
        }
    comparable_runs = [path for path in runs if (path.parent / "result-delta.json").is_file()]
    current = max(comparable_runs or runs, key=lambda path: path.stat().st_mtime_ns)
    delta_path = current.parent / "result-delta.json"
    delta = load_report(delta_path)
    outcome_counts = Counter(
        json.loads(line).get("outcome", "UNKNOWN")
        for line in current.read_text(encoding="utf-8").splitlines()
        if line
    )
    source_paths = [
        ROOT / "crates/php_syntax/src/grammar/expressions.rs",
        ROOT / "crates/php_vm/src/vm/jit_abi/native_builtins.rs",
        ROOT / "crates/php_vm/src/vm/jit_abi.rs",
        ROOT / "crates/php_vm/src/vm/jit_abi/object_support.rs",
        ROOT / "crates/php_vm/src/vm/jit_abi/internal_classes/pdo.rs",
        ROOT / "crates/php_runtime/src/builtins/modules/date.rs",
        ROOT / "crates/php_runtime/src/builtins/modules/fileinfo.rs",
        ROOT / "crates/php_runtime/src/builtins/modules/openssl.rs",
        ROOT / "crates/php_runtime/src/builtins/modules/streams.rs",
        ROOT / "crates/php_runtime/src/builtins/modules/strings.rs",
        ROOT / "crates/php_runtime/src/datetime.rs",
        ROOT / "crates/php_phpt_tools/src/commands/baseline.rs",
        ROOT / "scripts/phpt/result_delta.py",
        ROOT / ".cargo/config.toml",
    ]
    stale = current.stat().st_mtime_ns < max(path.stat().st_mtime_ns for path in source_paths)
    regressions = delta.get("regressions") if delta else None
    status = "stale" if stale else ("fail" if regressions else "available")
    reason = None
    if stale:
        reason = "latest comparable full PHPT result set predates native-surface changes"
    elif delta is None:
        reason = "full PHPT result exists without a baseline transition report"
    elif regressions:
        reason = f"full PHPT result contains {regressions} PASS-to-failure regression(s)"
    return {
        "status": status,
        "reason": reason,
        "current_report": current.relative_to(ROOT).as_posix(),
        "delta_report": delta_path.relative_to(ROOT).as_posix() if delta else None,
        "outcomes": dict(sorted(outcome_counts.items())),
        "regressions": regressions,
        "pass_to_skip": delta.get("pass_to_skip") if delta else None,
        "pass_to_skip_paths": delta.get("pass_to_skip_paths") if delta else [],
        "activated_failures": delta.get("activated_failures") if delta else None,
        "activated_failure_paths": delta.get("activated_failure_paths") if delta else [],
        "new_passes": delta.get("new_passes") if delta else None,
        "new_pass_paths": delta.get("new_pass_paths") if delta else [],
    }


def wordpress_evidence() -> dict:
    path = ROOT / "target/performance/wordpress-root/php85-coverage-current/summary.json"
    baseline_path = (
        ROOT
        / "target/performance/wordpress-root/"
        "object-root-visitor-snapshot-c1-c4-c8-baseline/summary.json"
    )
    report = load_report(path)
    if report is None:
        return {
            "status": "skipped",
            "reason": "current WordPress acceptance benchmark is unavailable",
            "report": path.relative_to(ROOT).as_posix(),
        }
    failures = list((report.get("correctness") or {}).get("failures") or [])
    c1 = next(
        (
            comparison
            for comparison in report.get("baseline_comparisons", [])
            if comparison.get("concurrency") == 1
        ),
        None,
    )
    improvement = c1.get("phrust_p50_improvement_pct") if c1 else None
    source_paths = [
        ROOT / "crates/php_runtime/src",
        ROOT / "crates/php_vm/src",
        ROOT / "crates/php_jit/src",
        ROOT / "crates/php_server/src",
    ]
    newest_source = max(
        candidate.stat().st_mtime_ns
        for source in source_paths
        for candidate in source.rglob("*.rs")
    )
    stale = path.stat().st_mtime_ns < newest_source
    reasons = []
    if stale:
        reasons.append("WordPress benchmark predates production Rust changes")
    if report.get("status") != "pass" or failures:
        reasons.append("WordPress correctness comparison failed")
    if not report.get("timing_eligible"):
        reasons.append("WordPress timing sample is ineligible")
    if improvement is None:
        reasons.append("WordPress c1 baseline comparison is unavailable")
    elif improvement < -5.0:
        reasons.append(f"warm WordPress c1 p50 regressed by {-improvement:.2f}%")
    return {
        "status": "fail" if reasons else "available",
        "reason": "; ".join(reasons) if reasons else None,
        "report": path.relative_to(ROOT).as_posix(),
        "correctness_failures": failures,
        "timing_eligible": bool(report.get("timing_eligible")),
        "c1_p50_improvement_pct": improvement,
        "c1_p50_regression_pct": -improvement if improvement is not None else None,
        "baseline_report": (
            baseline_path.relative_to(ROOT).as_posix()
            if (report.get("baseline") or {}).get("configured") and baseline_path.is_file()
            else None
        ),
    }


def semantic_operation_ids() -> list[dict[str, object]]:
    source = (ROOT / "crates/php_jit/src/region_ir/semantic_ops.rs").read_text(encoding="utf-8")
    match = re.search(
        r"pub enum RegionSemanticOperationId\s*\{(?P<body>.*?)\n\}", source, re.DOTALL
    )
    if match is None:
        return []
    return [
        {"name": name, "id": int(raw)}
        for name, raw in re.findall(
            r"^\s*([A-Za-z][A-Za-z0-9_]*)\s*=\s*(\d+),", match["body"], re.MULTILINE
        )
    ]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true", help="fail for incomplete classification")
    args = parser.parse_args()

    required = [
        INVENTORY / "functions.jsonl",
        INVENTORY / "methods.jsonl",
        INVENTORY / "classes.jsonl",
        INVENTORY / "constants.jsonl",
        INVENTORY / "language-operations.json",
        MANIFEST,
    ]
    missing = [path for path in required if not path.is_file()]
    if missing:
        print("native PHP surface report inputs missing:", file=sys.stderr)
        print("\n".join(f"- {path.relative_to(ROOT)}" for path in missing), file=sys.stderr)
        return 2

    OUT.mkdir(parents=True, exist_ok=True)
    surfaces = {
        "functions": load_jsonl(INVENTORY / "functions.jsonl"),
        "methods": load_jsonl(INVENTORY / "methods.jsonl"),
        "classes": load_jsonl(INVENTORY / "classes.jsonl"),
        "constants": load_jsonl(INVENTORY / "constants.jsonl"),
        "language_operations": json.loads(
            (INVENTORY / "language-operations.json").read_text(encoding="utf-8")
        ),
    }
    entries = [entry for values in surfaces.values() for entry in surface_entries(values)]
    support_classes = Counter(entry.get("support_class", "unclassified") for entry in entries)
    supported_count = sum(support_classes[name] for name in SUPPORTED)
    manifest = load_jsonl(MANIFEST)
    builtin_symbols = {
        row["symbol"].lower()
        for row in manifest
        if row.get("area") == "builtin_contract"
        and row.get("kind") == "function"
        and row.get("probe_case") == "availability"
    }
    method_symbols = {
        row["symbol"].lower()
        for row in manifest
        if row.get("area") == "internal_api_contract" and row.get("kind") == "method"
    }
    registered_functions = {
        str(entry["name"]).lower()
        for entry in surfaces["functions"]
        if entry.get("registry_availability")
    }
    registered_methods = {
        f"{entry['class']}::{entry['name']}".lower()
        for entry in surfaces["methods"]
        if entry.get("registry_availability")
    }
    phpt = phpt_evidence()
    checks = {
        "missing_lowering_absent": not any(
            "MissingLowering" in path.read_text(encoding="utf-8")
            for path in (ROOT / "crates/php_jit/src").rglob("*.rs")
        ),
        "synthetic_semantic_calls_absent": "__phrust_"
        not in (ROOT / "crates/php_jit/src/region_ir/executable.rs").read_text(encoding="utf-8"),
        "all_registered_functions_probed": registered_functions <= builtin_symbols,
        "all_registered_methods_probed": registered_methods <= method_symbols,
        "all_entries_classified": "unclassified" not in support_classes,
        "phpt_current": phpt["status"] in {"available", "fail"},
        "phpt_no_regressions": phpt.get("regressions") == 0,
    }
    support = {
        "php_version": "8.5.7",
        "total": len(entries),
        "supported": supported_count,
        "remaining_or_unproven": len(entries) - supported_count,
        "support_classes": dict(sorted(support_classes.items())),
        "surface_counts": {
            key: len(surface_entries(value))
            for key, value in surfaces.items()
        },
        "generated_probe_count": len(manifest),
        "checks": checks,
    }
    shutil.copyfile(INVENTORY / "functions.jsonl", OUT / "function-support.jsonl")
    shutil.copyfile(INVENTORY / "methods.jsonl", OUT / "method-support.jsonl")

    oracle_path, oracle = latest_oracle_report()
    vm_binary = ROOT / "target/debug/php-vm"
    runtime_sources = [
        ROOT / "crates/php_vm/src/vm/jit_abi/native_builtins.rs",
        ROOT / "crates/php_vm/src/vm/jit_abi/call_dispatch.rs",
        ROOT / "crates/php_jit/src/region_ir/semantic_ops.rs",
    ]
    oracle_stale = bool(oracle) and (
        oracle_path is None
        or not vm_binary.is_file()
        or vm_binary.stat().st_mtime_ns
        < max(source.stat().st_mtime_ns for source in runtime_sources)
        or oracle_path.stat().st_mtime_ns < vm_binary.stat().st_mtime_ns
    )
    oracle_status = "stale" if oracle_stale else ("available" if oracle else "skipped")
    oracle_reason = (
        "report used a VM binary older than the native-surface implementation"
        if oracle_stale
        else (None if oracle else "no generated oracle differential report exists")
    )
    baseline_path = ROOT / "target/oracle/probes/full-current/runtime-semantics-diff-report.json"
    baseline = load_report(baseline_path)
    oracle_delta = report_delta(baseline, oracle) if baseline and oracle else None
    waves = implementation_waves()
    runtime_evidence = runtime_corpus_evidence()
    wordpress = wordpress_evidence()
    checks.update(
        {
            "oracle_current": oracle_status == "available",
            "oracle_no_regressions": bool(oracle_delta)
            and oracle_delta.get("regressions") == 0,
            "runtime_corpus_current": runtime_evidence.get("status") == "available",
            "wordpress_correct_and_within_budget": wordpress.get("status") == "available",
        }
    )
    support["checks"] = checks
    support["wordpress"] = wordpress
    write_json(OUT / "support.json", support)
    write_json(
        OUT / "oracle-delta.json",
        {
            "status": oracle_status,
            "reason": oracle_reason,
            "baseline_report": (
                baseline_path.relative_to(ROOT).as_posix() if baseline else None
            ),
            "current_report": oracle_path.relative_to(ROOT).as_posix() if oracle_path else None,
            "current_summary": oracle.get("summary") if oracle else None,
            "generated_probe_count": len(manifest),
            "delta": oracle_delta,
            "implementation_waves": waves,
            "runtime_corpus": runtime_evidence,
        },
    )
    write_json(OUT / "implementation-waves.json", waves)
    write_json(OUT / "phpt-delta.json", phpt)
    write_json(OUT / "wordpress-evidence.json", wordpress)
    operations = semantic_operation_ids()
    write_json(
        OUT / "merge-contract.json",
        {
            "base_commit": BASE_COMMIT,
            "new_region_variants": ["RegionCallTarget::Semantic", "RegionSemanticOp"],
            "new_helper_ids": operations,
            "abi_hash_changed": True,
            "shared_files_touched": [
                "crates/php_jit/src/region_ir/executable.rs",
                "crates/php_jit/src/abi.rs",
                "crates/php_jit/src/cranelift_lowering/call_metadata.rs",
                "crates/php_vm/src/vm/jit_abi.rs",
                "crates/php_vm/src/vm/jit_abi/call_dispatch.rs",
            ],
            "required_rebase_actions": [
                "consume JitNativeCallKind::SEMANTIC_OPERATION",
                "preserve append-only RegionSemanticOperationId values",
            ],
        },
    )
    summary = [
        "# PHP 8.5 Native Surface Coverage",
        "",
        f"- Inventoried entries: `{len(entries)}`",
        f"- Reference-compatible and probed: `{supported_count}`",
        f"- Remaining or unproven: `{len(entries) - supported_count}`",
        f"- Generated probes: `{len(manifest)}`",
        f"- Typed semantic operation IDs: `{len(operations)}`",
        f"- Generated oracle result: `{oracle_status}`",
        f"- Runtime semantic corpus: `{runtime_evidence.get('status')}`",
        f"- PHPT evidence: `{phpt.get('status')}`",
        f"- WordPress correctness/performance evidence: `{wordpress.get('status')}`",
        "",
        "## Acceptance checks",
        "",
        *[f"- `{name}`: `{'pass' if passed else 'fail'}`" for name, passed in checks.items()],
        "",
        "Descriptor-availability probes inventory internal methods and properties but do not count as executable method-body support.",
        "",
        f"Oracle regressions from the expanded baseline: `{oracle_delta.get('regressions') if oracle_delta else 'unavailable'}`.",
        f"PHPT status: `{phpt.get('status')}` ({phpt.get('reason') or 'current full result set available'}).",
        f"PHPT PASS-to-failure regressions: `{phpt.get('regressions')}`; PASS-to-SKIP environmental reclassifications: `{phpt.get('pass_to_skip')}`.",
        f"PHPT newly passing: `{phpt.get('new_passes')}`; activated prior-SKIP gaps: `{phpt.get('activated_failures')}`.",
        f"WordPress status: `{wordpress.get('status')}` ({wordpress.get('reason') or 'correct and within the c1 p50 budget'}).",
        f"Warm WordPress c1 p50 improvement versus baseline: `{wordpress.get('c1_p50_improvement_pct')}` percent.",
    ]
    (OUT / "summary.md").write_text("\n".join(summary) + "\n", encoding="utf-8")

    failures = [name for name, passed in checks.items() if not passed]
    if args.check and failures:
        print("native PHP surface acceptance failed: " + ", ".join(failures), file=sys.stderr)
        return 1
    print(
        f"[ok] wrote PHP 8.5 coverage tranche ({len(entries)} entries, {len(manifest)} probes)"
    )
    return 0


def surface_entries(value: object) -> list[dict]:
    if isinstance(value, list):
        return value
    if isinstance(value, dict) and isinstance(value.get("entries"), list):
        return value["entries"]
    raise ValueError("native surface payload is neither an entry list nor an entries object")


if __name__ == "__main__":
    raise SystemExit(main())
