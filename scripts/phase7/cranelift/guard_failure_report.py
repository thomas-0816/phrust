#!/usr/bin/env python3
"""Analyze Cranelift guard failures and side exits from a Big-Win report."""

from __future__ import annotations

import argparse
import json
import sys
from collections import Counter
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[3]
DEFAULT_INPUT = ROOT / "target/phase7/cranelift/big_wins_report.json"
DEFAULT_OUT = ROOT / "target/phase7/cranelift/guard-report.json"
DEFAULT_TEXT_OUT = ROOT / "target/phase7/cranelift/guard-report.txt"
MINIMIZER = ROOT / "scripts/minimize_phase5_failure.py"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", type=Path, default=DEFAULT_INPUT, help="Big-Win report JSON input")
    parser.add_argument("--out", type=Path, default=DEFAULT_OUT, help="machine-readable guard report JSON")
    parser.add_argument("--text-out", type=Path, default=DEFAULT_TEXT_OUT, help="human-readable guard report text")
    parser.add_argument(
        "--minimize-dir",
        type=Path,
        default=ROOT / "target/phase7/cranelift/minimized",
        help="temp/output directory suggested for optional minimization hooks",
    )
    parser.add_argument(
        "--experimental-ic-report",
        type=Path,
        default=None,
        help="optional local polymorphic-IC experiment report to include in the guard output",
    )
    return parser.parse_args()


def rel(path: Path) -> str:
    try:
        return path.resolve().relative_to(ROOT).as_posix()
    except ValueError:
        return path.as_posix()


def load_report(path: Path) -> dict[str, Any]:
    if not path.is_file():
        raise SystemExit(f"missing Big-Win report JSON: {path}")
    try:
        decoded = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        raise SystemExit(f"invalid Big-Win report JSON {path}: {exc}") from exc
    if not isinstance(decoded, dict):
        raise SystemExit(f"Big-Win report is not a JSON object: {path}")
    rows = decoded.get("rows")
    if not isinstance(rows, list):
        raise SystemExit(f"Big-Win report has no rows array: {path}")
    return decoded


def load_optional_ic_report(path: Path | None) -> dict[str, Any] | None:
    if path is None:
        return None
    if not path.is_file():
        raise SystemExit(f"missing experimental IC report JSON: {path}")
    try:
        decoded = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        raise SystemExit(f"invalid experimental IC report JSON {path}: {exc}") from exc
    if not isinstance(decoded, dict):
        raise SystemExit(f"experimental IC report is not a JSON object: {path}")
    return decoded


def int_from(mapping: dict[str, Any], key: str) -> int:
    value = mapping.get(key, 0)
    return value if isinstance(value, int) else 0


def reason_map(row: dict[str, Any], key: str) -> dict[str, int]:
    raw = row.get(key)
    if not isinstance(raw, dict):
        return {}
    return {str(name): int(value) for name, value in raw.items() if isinstance(value, int)}


def failure_events(row: dict[str, Any]) -> int:
    counters = row.get("counters") if isinstance(row.get("counters"), dict) else {}
    return (
        int_from(counters, "side_exits")
        + int_from(counters, "guard_failures")
        + int_from(counters, "blacklisted_regions")
        + int_from(counters, "bailouts")
    )


def total_events(row: dict[str, Any]) -> int:
    counters = row.get("counters") if isinstance(row.get("counters"), dict) else {}
    return max(
        1,
        int_from(counters, "executed_regions")
        + failure_events(row),
    )


def recommended_action(row: dict[str, Any]) -> str:
    counters = row.get("counters") if isinstance(row.get("counters"), dict) else {}
    known_gaps = row.get("known_gaps") if isinstance(row.get("known_gaps"), list) else []
    reasons = reason_map(row, "side_exit_reasons")
    blacklist_reasons = reason_map(row, "blacklist_reasons")
    rate = failure_events(row) / total_events(row)
    if known_gaps or row.get("jit_status") == "fallback":
        return "unsupported"
    if int_from(counters, "blacklisted_regions") > 0 or blacklist_reasons:
        return "blacklist"
    if any(reasons.get(reason, 0) > 0 for reason in ("guard_failed", "type_mismatch", "helper_status")):
        return "specialize"
    if rate >= 0.5 and failure_events(row) > 0:
        return "blacklist"
    return "keep"


def candidate_summary(row: dict[str, Any]) -> dict[str, Any]:
    counters = row.get("counters") if isinstance(row.get("counters"), dict) else {}
    failures = failure_events(row)
    total = total_events(row)
    return {
        "scenario": row.get("scenario"),
        "fixture": row.get("fixture"),
        "target": row.get("target"),
        "matrix_family": row.get("matrix_family"),
        "jit_status": row.get("jit_status"),
        "failure_events": failures,
        "total_events": total,
        "failure_rate": failures / total,
        "side_exits": int_from(counters, "side_exits"),
        "guard_failures": int_from(counters, "guard_failures"),
        "blacklisted_regions": int_from(counters, "blacklisted_regions"),
        "bailouts": int_from(counters, "bailouts"),
        "side_exit_reasons": reason_map(row, "side_exit_reasons"),
        "blacklist_reasons": reason_map(row, "blacklist_reasons"),
        "known_gaps": row.get("known_gaps") if isinstance(row.get("known_gaps"), list) else [],
        "recommended_action": recommended_action(row),
    }


def minimizer_hook(candidate: dict[str, Any], minimize_dir: Path) -> dict[str, Any] | None:
    fixture = candidate.get("fixture")
    if not isinstance(fixture, str) or not fixture.endswith(".php"):
        return None
    fixture_path = ROOT / fixture
    if not fixture_path.is_file() or not MINIMIZER.is_file():
        return None
    output = minimize_dir / fixture_path.name
    return {
        "available": True,
        "writes_only_under": rel(minimize_dir),
        "command": [
            rel(MINIMIZER),
            rel(fixture_path),
            "--out",
            rel(output),
            "--rust-vm",
            "target/debug/php-vm",
        ],
        "note": "advisory hook only; run manually with REFERENCE_PHP set if the row becomes a differential failure",
    }


def analyze(
    report: dict[str, Any],
    minimize_dir: Path,
    experimental_ic_report: dict[str, Any] | None = None,
) -> dict[str, Any]:
    rows = [row for row in report["rows"] if isinstance(row, dict) and row.get("jit_mode") == "cranelift"]
    side_exit_counts: Counter[str] = Counter()
    blacklist_counts: Counter[str] = Counter()
    candidates = [candidate_summary(row) for row in rows]
    for row in rows:
        side_exit_counts.update(reason_map(row, "side_exit_reasons"))
        blacklist_counts.update(reason_map(row, "blacklist_reasons"))

    failing = [candidate for candidate in candidates if candidate["failure_events"] > 0]
    failing.sort(key=lambda item: (item["failure_rate"], item["failure_events"], str(item["scenario"])), reverse=True)
    blacklisted = [
        candidate
        for candidate in candidates
        if candidate["blacklisted_regions"] > 0 or candidate["blacklist_reasons"]
    ]
    recommendations = Counter(candidate["recommended_action"] for candidate in candidates)
    minimizer_hooks = [
        hook
        for candidate in failing[:10]
        for hook in [minimizer_hook(candidate, minimize_dir)]
        if hook is not None
    ]

    ic_guards: list[dict[str, Any]] = []
    if experimental_ic_report is not None:
        raw_guards = experimental_ic_report.get("guard_report_extension")
        if isinstance(raw_guards, list):
            ic_guards = [guard for guard in raw_guards if isinstance(guard, dict)]

    return {
        "schema_version": 1,
        "gate": "cranelift-guard-report",
        "status": "pass",
        "input": report.get("run_id", "unknown"),
        "source_report_status": report.get("status"),
        "top_side_exit_reasons": [
            {"reason": reason, "count": count}
            for reason, count in side_exit_counts.most_common()
        ],
        "blacklist_reasons": [
            {"reason": reason, "count": count}
            for reason, count in blacklist_counts.most_common()
        ],
        "high_failure_rate_functions": failing[:10],
        "blacklisted_candidates": blacklisted,
        "recommendation_counts": dict(sorted(recommendations.items())),
        "row_recommendations": candidates,
        "minimizer_hooks": minimizer_hooks,
        "experimental_ic_guards": ic_guards,
    }


def render_text(analysis: dict[str, Any]) -> str:
    lines = [
        "# Cranelift Guard Failure Report",
        "",
        f"Status: {analysis['status']}",
        f"Source report status: {analysis.get('source_report_status', 'unknown')}",
        "",
        "## Top Side-Exit Reasons",
    ]
    if analysis["top_side_exit_reasons"]:
        for item in analysis["top_side_exit_reasons"]:
            lines.append(f"- {item['reason']}: {item['count']}")
    else:
        lines.append("- none")

    lines.extend(["", "## High Failure Rate Functions"])
    if analysis["high_failure_rate_functions"]:
        for item in analysis["high_failure_rate_functions"]:
            lines.append(
                "- {scenario} ({fixture}): rate={rate:.2f}, failures={failures}, action={action}".format(
                    scenario=item["scenario"],
                    fixture=item["fixture"],
                    rate=item["failure_rate"],
                    failures=item["failure_events"],
                    action=item["recommended_action"],
                )
            )
    else:
        lines.append("- none")

    lines.extend(["", "## Blacklisted Candidates"])
    if analysis["blacklisted_candidates"]:
        for item in analysis["blacklisted_candidates"]:
            reasons = ", ".join(sorted(item["blacklist_reasons"])) or "blacklisted_regions"
            lines.append(f"- {item['scenario']} ({item['fixture']}): {reasons}")
    else:
        lines.append("- none")

    lines.extend(["", "## Recommended Actions"])
    for action, count in analysis["recommendation_counts"].items():
        lines.append(f"- {action}: {count}")

    lines.extend(["", "## Experimental Polymorphic IC Guards"])
    if analysis["experimental_ic_guards"]:
        for item in analysis["experimental_ic_guards"]:
            lines.append(
                "- {scenario}: {kind} state={state}, entries={entries}/{limit}, fallback={fallback}".format(
                    scenario=item.get("scenario", "unknown"),
                    kind=item.get("kind", "unknown"),
                    state=item.get("state", "unknown"),
                    entries=item.get("guard_entry_count", 0),
                    limit=item.get("max_polymorphic_entries", 0),
                    fallback=item.get("fallback", "none"),
                )
            )
    else:
        lines.append("- none")

    lines.extend(["", "## Minimizer Hooks"])
    if analysis["minimizer_hooks"]:
        for hook in analysis["minimizer_hooks"]:
            lines.append(f"- {' '.join(hook['command'])}")
    else:
        lines.append("- none available")
    return "\n".join(lines) + "\n"


def main() -> int:
    args = parse_args()
    report = load_report(args.input)
    experimental_ic_report = load_optional_ic_report(args.experimental_ic_report)
    analysis = analyze(report, args.minimize_dir, experimental_ic_report)
    text = render_text(analysis)

    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.text_out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(analysis, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    args.text_out.write_text(text, encoding="utf-8")
    print(text, end="")
    print(f"[pass] Cranelift guard failure report wrote {rel(args.out)} and {rel(args.text_out)}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
