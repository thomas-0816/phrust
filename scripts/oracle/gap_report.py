#!/usr/bin/env python3
"""Merge oracle outputs into a prioritized gap queue."""

from __future__ import annotations

import argparse
import glob
import hashlib
import json
import sys
import tempfile
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_OUT = REPO_ROOT / "target/oracle/gap-report.json"
DEFAULT_DOC = REPO_ROOT / "target/oracle/gap-report-summary.md"
DEFAULT_BASELINE = REPO_ROOT / "tests/oracle/gap-report-baseline.json"
API_JSONL = REPO_ROOT / "target/oracle/api/php-source-api-symbols.jsonl"
PRIORITY_RANK = {"P0": 0, "P1": 1, "P2": 2, "P3": 3, "P4": 4}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--api", type=Path, default=API_JSONL)
    parser.add_argument("--out", type=Path, default=DEFAULT_OUT)
    parser.add_argument("--summary", type=Path, default=DEFAULT_DOC)
    parser.add_argument("--baseline", type=Path, default=DEFAULT_BASELINE)
    parser.add_argument(
        "--cheap",
        action="store_true",
        help="use deterministic oracle/API/probe inputs only (default)",
    )
    parser.add_argument(
        "--full",
        action="store_true",
        help="also include broad runtime, stdlib, PHPT, and app-smoke artifacts under target/",
    )
    parser.add_argument("--check", action="store_true", help="enforce oracle gap ratchets")
    parser.add_argument("--fail-on-unclassified", action="store_true")
    parser.add_argument("--update-baseline", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--self-test-only", action="store_true")
    args = parser.parse_args()
    if args.cheap and args.full:
        parser.error("--cheap and --full are mutually exclusive")
    cheap_mode = not args.full

    try:
        if args.self_test or args.self_test_only:
            run_self_tests()
        if args.self_test_only:
            return 0

        entries = collect_entries(args.api, cheap=cheap_mode)
        entries = sorted(entries, key=entry_sort_key)
        report = build_report(entries, mode="cheap" if cheap_mode else "full")
        unclassified = [
            entry for entry in entries if entry["status"] == "unclassified_failure"
        ]
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        args.summary.parent.mkdir(parents=True, exist_ok=True)
        args.summary.write_text(render_summary(report), encoding="utf-8")
        if args.update_baseline:
            write_baseline(args.baseline, report)
        if args.check:
            check_report(report, args.baseline)
    except Exception as error:  # noqa: BLE001 - script boundary.
        print(f"oracle gap report error: {error}", file=sys.stderr)
        return 1

    print(f"[ok] wrote {relative(args.out)}")
    print(f"[ok] wrote {relative(args.summary)}")
    if args.update_baseline:
        print(f"[ok] wrote {relative(args.baseline)}")
    if args.fail_on_unclassified and unclassified:
        print(f"[fail] unclassified oracle failures: {len(unclassified)}", file=sys.stderr)
        return 1
    return 0


def collect_entries(api_path: Path, *, cheap: bool) -> list[dict[str, Any]]:
    entries = []
    entries.extend(api_entries(api_path))
    entries.extend(runtime_report_entries(oracle_only=cheap))
    if not cheap:
        entries.extend(stdlib_report_entries())
    if not cheap:
        entries.extend(phpt_entries())
        entries.extend(wordpress_entries())
    return entries


def api_entries(path: Path) -> list[dict[str, Any]]:
    if not path.is_file():
        return []
    entries = []
    for row in read_jsonl(path):
        status = row.get("status")
        if status == "matched":
            continue
        if status == "reference_unavailable":
            continue
        if status == "extractor_gap":
            entries.append(
                entry(
                    gap_id=stable_id("API-EXTRACTOR", row),
                    status="known_gap",
                    layer="oracle_extractor",
                    pattern_family="metadata_extraction",
                    extension=row.get("extension"),
                    symbol=row.get("name"),
                    source=row.get("source"),
                    priority="P3",
                    confidence="medium",
                    suggested_owner="oracle-tooling",
                    suggested_next_probe="Add a reduced stub parser fixture for this declaration.",
                    oracle_reference="php-src stub extractor",
                    reason=runtime_value(row),
                )
            )
            continue
        priority = "P3" if status in {"metadata_mismatch", "reference_only_known_gap"} else "P2"
        layer = "stdlib_metadata" if row.get("kind") in {"function", "constant", "extension"} else "runtime_api"
        entries.append(
            entry(
                gap_id=stable_id(f"API-{status}", row),
                status=status,
                layer=layer,
                pattern_family=api_pattern(row),
                extension=row.get("extension"),
                symbol=qualified_symbol(row),
                source=row.get("source"),
                priority=priority,
                confidence="high",
                suggested_owner=owner_for_layer(layer),
                suggested_next_probe=f"Generate a reflection/API probe for {qualified_symbol(row)}.",
                oracle_reference="php-source-api-symbols.jsonl",
                diagnostic_id=None,
            )
        )
    return entries


def runtime_report_entries(*, oracle_only: bool = False) -> list[dict[str, Any]]:
    entries = []
    for path in glob_paths("target/oracle/probes/**/runtime-semantics-diff-report.json"):
        entries.extend(runtime_entries_from_report(path, oracle_reference="oracle probe diff"))
    if oracle_only:
        return entries
    for path in glob_paths("target/runtime-semantics/**/runtime-semantics-diff-report.json"):
        entries.extend(runtime_entries_from_report(path, oracle_reference="runtime semantics diff"))
    return entries


def runtime_entries_from_report(path: Path, oracle_reference: str) -> list[dict[str, Any]]:
    payload = read_json(path)
    entries = []
    for result in payload.get("results", []):
        if result.get("status") not in {"fail", "known_gap"}:
            continue
        metadata = result.get("metadata") or {}
        diagnostic = result.get("primary_diagnostic") or {}
        layer = layer_from_failure(result)
        status = "known_gap" if result.get("status") == "known_gap" else "unclassified_failure"
        known_gap = result.get("known_gap_id")
        if status == "known_gap" and not known_gap:
            status = "unclassified_failure"
        fixture = result.get("file")
        entries.append(
            entry(
                gap_id=known_gap or stable_id("RUNTIME-FAIL", result),
                status=status,
                layer=layer,
                pattern_family=result.get("failure_category") or metadata.get("failure_category") or "unclassified",
                extension=None,
                symbol=metadata.get("oracle_probe_id") or metadata.get("fixture_id"),
                source=relative(path),
                fixture=fixture,
                diagnostic_id=diagnostic.get("id") or first_diagnostic_id(result),
                oracle_reference=oracle_reference,
                priority=priority_for_layer(layer, status),
                confidence="high" if status == "known_gap" else "medium",
                suggested_owner=owner_for_layer(layer),
                suggested_next_probe=f"Reduce and promote {fixture} into the owning fixture category.",
                reason=result.get("message"),
            )
        )
    return entries


def stdlib_report_entries() -> list[dict[str, Any]]:
    entries = []
    for path in glob_paths("target/stdlib/**/stdlib-diff-report.json"):
        payload = read_json(path)
        for result in payload.get("results", []):
            if result.get("status") not in {"fail", "known_gap"}:
                continue
            entries.append(
                entry(
                    gap_id=result.get("known_gap_id") or stable_id("STDLIB", result),
                    status="known_gap" if result.get("status") == "known_gap" else "unclassified_failure",
                    layer="stdlib_runtime",
                    pattern_family=result.get("area") or "stdlib_diff",
                    extension=result.get("extension"),
                    symbol=result.get("function") or result.get("symbol"),
                    source=relative(path),
                    fixture=result.get("file"),
                    diagnostic_id=first_diagnostic_id(result),
                    oracle_reference="stdlib differential report",
                    priority="P2",
                    confidence="medium",
                    suggested_owner="php_std/php_runtime",
                    suggested_next_probe="Promote this stdlib diff into a focused oracle probe.",
                    reason=result.get("message"),
                )
            )
    return entries


def phpt_entries() -> list[dict[str, Any]]:
    entries = []
    for path in glob_paths("target/phpt-work/**/*.jsonl"):
        for row in read_jsonl(path):
            status = str(row.get("status", "")).lower()
            if status in {"pass", "skip", "known_gap"}:
                continue
            entries.append(
                entry(
                    gap_id=stable_id("PHPT", row),
                    status="unclassified_failure",
                    layer="phpt",
                    pattern_family=row.get("module") or "phpt",
                    extension=row.get("extension"),
                    symbol=row.get("test") or row.get("file"),
                    source=relative(path),
                    fixture=row.get("file"),
                    diagnostic_id=first_diagnostic_id(row),
                    oracle_reference="PHPT result",
                    priority="P1",
                    confidence="medium",
                    suggested_owner="phpt/runtime owner",
                    suggested_next_probe="Reduce the PHPT failure into an oracle_generated fixture.",
                    reason=row.get("reason") or row.get("message"),
                )
            )
    return entries


def wordpress_entries() -> list[dict[str, Any]]:
    entries = []
    for path in glob_paths("target/**/classified_failure.json"):
        row = read_json(path)
        entries.append(
            entry(
                gap_id=stable_id("APP", row),
                status="unclassified_failure",
                layer="app_smoke",
                pattern_family=row.get("failure_category") or "wordpress_smoke",
                extension=None,
                symbol=row.get("symbol"),
                source=relative(path),
                fixture=row.get("fixture") or row.get("request"),
                diagnostic_id=first_diagnostic_id(row),
                oracle_reference="application smoke failure extract",
                priority="P0",
                confidence="medium",
                suggested_owner="app-smoke owning runtime layer",
                suggested_next_probe="Create a reduced runtime_semantics fixture from the extracted first failure.",
                reason=row.get("message") or row.get("reason"),
            )
        )
    return entries


def entry(**kwargs: Any) -> dict[str, Any]:
    fields = {
        "gap_id": None,
        "status": None,
        "layer": None,
        "pattern_family": None,
        "extension": None,
        "symbol": None,
        "source": None,
        "fixture": None,
        "diagnostic_id": None,
        "oracle_reference": None,
        "priority": None,
        "confidence": None,
        "suggested_owner": None,
        "suggested_next_probe": None,
        "reason": None,
    }
    fields.update(kwargs)
    return fields


def build_report(entries: list[dict[str, Any]], *, mode: str = "cheap") -> dict[str, Any]:
    return {
        "summary": {
            "total": len(entries),
            "by_priority": count_by(entries, "priority"),
            "by_status": count_by(entries, "status"),
            "by_layer": count_by(entries, "layer"),
            "unclassified_failures": sum(
                1 for entry in entries if entry["status"] == "unclassified_failure"
            ),
        },
        "entries": entries,
        "mode": mode,
        "ratchet": {
            "unclassified_failures_fail_oracle_smoke": True,
            "known_gap_required_fields": [
                "gap_id",
                "fixture",
                "source",
                "layer",
                "priority",
                "reason",
            ],
            "p0_p1_require_reduced_fixture": True,
        },
    }


def check_report(report: dict[str, Any], baseline_path: Path) -> None:
    errors: list[str] = []
    entries = report.get("entries")
    if not isinstance(entries, list):
        raise ValueError("gap report entries must be a list")

    unclassified = [
        entry for entry in entries if entry.get("status") == "unclassified_failure"
    ]
    if unclassified:
        errors.append(f"unclassified oracle failures: {len(unclassified)}")

    required = report.get("ratchet", {}).get("known_gap_required_fields") or []
    for index, item in enumerate(entries, 1):
        if not isinstance(item, dict):
            errors.append(f"entry {index}: entry must be an object")
            continue
        if item.get("status") == "known_gap":
            for field in required:
                value = item.get(field)
                if value is None or value == "":
                    errors.append(
                        f"{item.get('gap_id') or f'entry {index}'}: known gap missing {field}"
                    )
        priority = item.get("priority")
        if priority in {"P0", "P1"} and not item.get("fixture"):
            errors.append(
                f"{item.get('gap_id') or f'entry {index}'}: {priority} gap requires a reduced fixture"
            )

    baseline = read_baseline(baseline_path)
    current = p0_p1_counts(report)
    limits = baseline.get("limits", {})
    for priority in ["P0", "P1"]:
        limit = int(limits.get(priority, 0))
        count = current.get(priority, 0)
        if count > limit:
            errors.append(
                f"{priority} gap count increased from baseline {limit} to {count}; "
                "run with --update-baseline only for an intentional ratchet update"
            )

    if errors:
        raise ValueError("oracle ratchet check failed:\n- " + "\n- ".join(errors))


def read_baseline(path: Path) -> dict[str, Any]:
    if not path.is_file():
        raise FileNotFoundError(f"missing oracle gap baseline: {relative(path)}")
    payload = read_json(path)
    limits = payload.get("limits")
    if not isinstance(limits, dict):
        raise ValueError(f"{relative(path)}: limits must be an object")
    return payload


def write_baseline(path: Path, report: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = {
        "description": "Oracle gap report ratchet limits. Update intentionally with gap evidence.",
        "limits": p0_p1_counts(report),
        "source_report": relative(DEFAULT_OUT),
    }
    path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def p0_p1_counts(report: dict[str, Any]) -> dict[str, int]:
    by_priority = report.get("summary", {}).get("by_priority", {})
    return {
        "P0": int(by_priority.get("P0", 0)),
        "P1": int(by_priority.get("P1", 0)),
    }


def run_self_tests() -> None:
    fixture = "fixtures/runtime_semantics/oracle_generated/smoke/example.php"
    valid_known_gap = entry(
        gap_id="RUNTIME-SEED",
        status="known_gap",
        layer="runtime_semantics",
        pattern_family="reference_binding",
        fixture=fixture,
        source="target/oracle/probes/smoke/runtime-semantics-diff-report.json",
        priority="P1",
        reason="reference requires by-ref binding semantics",
    )
    unclassified = {**valid_known_gap, "gap_id": "RUNTIME-NEW", "status": "unclassified_failure"}
    malformed_known_gap = {**valid_known_gap, "gap_id": "RUNTIME-BAD", "fixture": None}
    increased = {**valid_known_gap, "gap_id": "RUNTIME-INCREASE"}

    with tempfile.TemporaryDirectory() as raw_dir:
        baseline_path = Path(raw_dir) / "baseline.json"
        baseline_path.write_text(
            json.dumps({"limits": {"P0": 0, "P1": 1}}) + "\n",
            encoding="utf-8",
        )
        check_report(build_report([valid_known_gap]), baseline_path)
        expect_check_failure(
            build_report([unclassified]),
            baseline_path,
            "unclassified oracle failures",
        )
        expect_check_failure(
            build_report([malformed_known_gap]),
            baseline_path,
            "known gap missing fixture",
        )

        baseline_path.write_text(
            json.dumps({"limits": {"P0": 0, "P1": 0}}) + "\n",
            encoding="utf-8",
        )
        expect_check_failure(
            build_report([increased]),
            baseline_path,
            "P1 gap count increased",
        )


def expect_check_failure(report: dict[str, Any], baseline_path: Path, needle: str) -> None:
    try:
        check_report(report, baseline_path)
    except ValueError as error:
        if needle not in str(error):
            raise AssertionError(f"expected {needle!r} in {error}") from error
        return
    raise AssertionError(f"expected check failure containing {needle!r}")


def render_summary(report: dict[str, Any]) -> str:
    summary = report["summary"]
    lines = [
        "# Oracle Gap Report Summary",
        "",
        "Generated by `scripts/oracle/gap_report.py` via `just oracle-gap-report`.",
        "",
        f"- Mode: `{report.get('mode', 'cheap')}`",
        f"- Total queue items: {summary['total']}",
        f"- Unclassified failures: {summary['unclassified_failures']}",
        f"- Machine report: `target/oracle/gap-report.json`",
        "",
        "## Priority Counts",
        "",
        "| Priority | Count |",
        "| --- | ---: |",
    ]
    for priority in ["P0", "P1", "P2", "P3", "P4"]:
        lines.append(f"| `{priority}` | {summary['by_priority'].get(priority, 0)} |")
    lines.extend(["", "## Layer Counts", "", "| Layer | Count |", "| --- | ---: |"])
    for layer, count in sorted(summary["by_layer"].items()):
        lines.append(f"| `{layer}` | {count} |")
    lines.extend(["", "## Fix Next", ""])
    for item in report["entries"][:25]:
        lines.append(
            f"- `{item['priority']}` `{item['gap_id']}` "
            f"{item['layer']}/{item['pattern_family']} "
            f"{item.get('symbol') or item.get('fixture') or ''}"
        )
    if not report["entries"]:
        lines.append("No oracle queue items found.")
    lines.extend(["", "## Ratchet", ""])
    lines.append("- `oracle-smoke` fails on new unclassified oracle failures.")
    lines.append("- Known gaps require an ID, fixture/source pointer, layer, priority, and reason.")
    lines.append("- P0/P1 gaps require reduced fixtures and real-world reproduction pointers when available.")
    lines.append("")
    return "\n".join(lines)


def api_pattern(row: dict[str, Any]) -> str:
    kind = row.get("kind")
    if kind == "function":
        return "builtin_function_surface"
    if kind in {"class", "interface", "trait", "enum"}:
        return "classlike_surface"
    if kind == "method":
        return "method_metadata"
    if kind in {"constant", "class_constant"}:
        return "constant_surface"
    return f"{kind}_surface"


def qualified_symbol(row: dict[str, Any]) -> str:
    if row.get("class"):
        return f"{row['class']}::{row.get('name')}"
    return str(row.get("name"))


def layer_from_failure(result: dict[str, Any]) -> str:
    category = result.get("failure_category") or (result.get("metadata") or {}).get("failure_category")
    if category in {"frontend_lowering", "parser", "semantic_folding", "ir_lowering"}:
        return "frontend_lowering" if category == "frontend_lowering" else category
    if category in {"reference_binding", "callable_dispatch", "name_resolution"}:
        return "runtime_semantics"
    if category == "reflection":
        return "reflection_metadata"
    return "vm_runtime"


def priority_for_layer(layer: str, status: str) -> str:
    if status == "unclassified_failure":
        return "P1"
    if layer in {"runtime_semantics", "frontend_lowering", "ir_lowering"}:
        return "P1"
    if layer in {"reflection_metadata", "stdlib_metadata"}:
        return "P3"
    return "P2"


def owner_for_layer(layer: str) -> str:
    if layer in {"frontend_lowering", "semantic_folding", "ir_lowering"}:
        return "php_semantics/php_ir"
    if layer in {"runtime_semantics", "vm_runtime"}:
        return "php_runtime/php_vm"
    if layer in {"stdlib_metadata", "stdlib_runtime", "reflection_metadata"}:
        return "php_std/php_runtime"
    if layer == "oracle_extractor":
        return "oracle-tooling"
    return "owning runtime layer"


def read_json(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def read_jsonl(path: Path) -> list[dict[str, Any]]:
    rows = []
    for raw in path.read_text(encoding="utf-8", errors="replace").splitlines():
        if raw.strip():
            rows.append(json.loads(raw))
    return rows


def glob_paths(pattern: str) -> list[Path]:
    return sorted(Path(path) for path in glob.glob(str(REPO_ROOT / pattern), recursive=True))


def count_by(entries: list[dict[str, Any]], field: str) -> dict[str, int]:
    counts: dict[str, int] = {}
    for item in entries:
        key = item.get(field) or "unknown"
        counts[key] = counts.get(key, 0) + 1
    return counts


def stable_id(prefix: str, value: Any) -> str:
    encoded = json.dumps(value, sort_keys=True, default=str)
    return f"{prefix}-{hashlib.sha1(encoded.encode('utf-8')).hexdigest()[:16]}"


def entry_sort_key(item: dict[str, Any]) -> tuple[int, str, str, str]:
    return (
        PRIORITY_RANK.get(item.get("priority"), 9),
        item.get("layer") or "",
        item.get("pattern_family") or "",
        item.get("gap_id") or "",
    )


def first_diagnostic_id(value: Any) -> str | None:
    text = json.dumps(value, default=str)
    marker = "E_PHP_"
    index = text.find(marker)
    if index < 0:
        return None
    end = index
    while end < len(text) and (text[end].isalnum() or text[end] == "_"):
        end += 1
    return text[index:end]


def runtime_value(row: dict[str, Any]) -> str | None:
    value = row.get("runtime_value")
    if isinstance(value, dict):
        return str(value.get("value") or value.get("type"))
    return None


def relative(path: Path | str) -> str:
    path = Path(path)
    try:
        return path.relative_to(REPO_ROOT).as_posix()
    except ValueError:
        return str(path)


if __name__ == "__main__":
    raise SystemExit(main())
