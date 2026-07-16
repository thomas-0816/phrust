#!/usr/bin/env python3
"""Compare two PHPT result sets and emit a focused regression manifest."""

from __future__ import annotations

import argparse
import json
import sys
from collections import Counter
from pathlib import Path


PASS = "PASS"
NON_FAILURE = {PASS, "SKIP"}


def load_results(path: Path) -> dict[str, dict]:
    results: dict[str, dict] = {}
    for line_number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not line:
            continue
        row = json.loads(line)
        name = str(row.get("path") or "")
        if not name:
            raise ValueError(f"{path}:{line_number}: result has no path")
        if name in results:
            raise ValueError(f"{path}:{line_number}: duplicate result path {name}")
        results[name] = row
    return results


def compare(baseline: dict[str, dict], current: dict[str, dict]) -> dict:
    common = sorted(set(baseline) & set(current))
    transitions = Counter(
        f"{baseline[path].get('outcome', 'UNKNOWN')}->{current[path].get('outcome', 'UNKNOWN')}"
        for path in common
    )
    regressions = [
        path
        for path in common
        if baseline[path].get("outcome") == PASS
        and current[path].get("outcome") not in NON_FAILURE
    ]
    reclassifications = [
        path
        for path in common
        if baseline[path].get("outcome") == PASS
        and current[path].get("outcome") == "SKIP"
    ]
    activated_failures = [
        path
        for path in common
        if baseline[path].get("outcome") == "SKIP"
        and current[path].get("outcome") not in {"PASS", "SKIP", "XFAIL"}
    ]
    improvements = [
        path
        for path in common
        if baseline[path].get("outcome") != PASS
        and current[path].get("outcome") == PASS
    ]
    return {
        "baseline_total": len(baseline),
        "current_total": len(current),
        "common": len(common),
        "added": len(set(current) - set(baseline)),
        "removed": len(set(baseline) - set(current)),
        "baseline_outcomes": dict(
            sorted(Counter(row.get("outcome", "UNKNOWN") for row in baseline.values()).items())
        ),
        "current_outcomes": dict(
            sorted(Counter(row.get("outcome", "UNKNOWN") for row in current.values()).items())
        ),
        "transitions": dict(sorted(transitions.items())),
        "regressions": len(regressions),
        "regression_paths": regressions,
        "pass_to_skip": len(reclassifications),
        "pass_to_skip_paths": reclassifications,
        "activated_failures": len(activated_failures),
        "activated_failure_paths": activated_failures,
        "new_passes": len(improvements),
        "new_pass_paths": improvements,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--current", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--regression-manifest", type=Path)
    args = parser.parse_args()

    try:
        delta = compare(load_results(args.baseline), load_results(args.current))
        delta.update(
            {
                "status": "pass" if delta["regressions"] == 0 else "fail",
                "baseline": args.baseline.as_posix(),
                "current": args.current.as_posix(),
            }
        )
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(json.dumps(delta, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        if args.regression_manifest is not None:
            args.regression_manifest.parent.mkdir(parents=True, exist_ok=True)
            args.regression_manifest.write_text(
                "".join(
                    json.dumps({"path": path}, separators=(",", ":")) + "\n"
                    for path in delta["regression_paths"]
                ),
                encoding="utf-8",
            )
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"PHPT result delta error: {error}", file=sys.stderr)
        return 2

    print(
        f"[{'ok' if delta['regressions'] == 0 else 'fail'}] PHPT delta "
        f"regressions={delta['regressions']} pass_to_skip={delta['pass_to_skip']} "
        f"activated_failures={delta['activated_failures']} new_passes={delta['new_passes']} "
        f"out={args.out}"
    )
    return 0 if delta["regressions"] == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
