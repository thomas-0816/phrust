#!/usr/bin/env python3
"""Gate warmed dense-ID inline-cache lookup against its coordinate control."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
BASELINE = ROOT / "scripts/performance/inline_cache_lookup_baseline.json"
CRITERION = ROOT / "target/criterion"


def median(path: Path) -> float:
    data = json.loads(path.read_text(encoding="utf-8"))
    value = data["median"]["point_estimate"]
    if not isinstance(value, (int, float)) or value <= 0:
        raise ValueError(f"invalid median point estimate in {path}")
    return float(value)


def check(dense: float, coordinate: float, maximum_ratio: float) -> str | None:
    ratio = dense / coordinate
    if ratio > maximum_ratio:
        return (
            f"dense-ID median {dense:.3f} ns / coordinate median "
            f"{coordinate:.3f} ns = {ratio:.3f}; baseline limit is "
            f"{maximum_ratio:.3f}"
        )
    return None


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    baseline = json.loads(BASELINE.read_text(encoding="utf-8"))
    maximum_ratio = float(baseline["maximum_dense_to_coordinate_ratio"])
    if args.self_test:
        if check(96.0, 100.0, maximum_ratio) is None:
            raise ValueError("regression fixture was not rejected")
        if check(80.0, 100.0, maximum_ratio) is not None:
            raise ValueError("healthy fixture was rejected")
        print("[ok] inline-cache lookup gate self-test")
        return 0
    dense = median(
        CRITERION / "performance_inline_cache_function_hit_dense_id/new/estimates.json"
    )
    coordinate = median(
        CRITERION
        / "performance_inline_cache_function_hit_coordinate/new/estimates.json"
    )
    failure = check(dense, coordinate, maximum_ratio)
    if failure:
        print(
            "[fail] rule 10 (performance-contract) | warmed inline-cache lookup | "
            f"current: {failure} | remediation: profile dense-ID lookup and "
            "restore its advantage over coordinate lookup",
            file=sys.stderr,
        )
        return 1
    print(
        "[ok] warmed inline-cache lookup "
        f"dense={dense:.3f} ns coordinate={coordinate:.3f} ns "
        f"ratio={dense / coordinate:.3f} limit={maximum_ratio:.3f}"
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, KeyError, json.JSONDecodeError) as error:
        print(f"[fail] inline-cache lookup gate: {error}", file=sys.stderr)
        raise SystemExit(1) from error
