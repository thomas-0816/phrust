#!/usr/bin/env python3
"""Compare Rust parser acceptance with the pinned PHP lint oracle."""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tomllib
from pathlib import Path

import run_parser_fixtures


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_ALLOWLIST = ROOT / "fixtures" / "parser" / "known_gaps.toml"


def rust_parse_result(fixture: Path) -> dict[str, object]:
    process = subprocess.run(
        ["cargo", "run", "--quiet", "-p", "php_parser_cli", "--", "--json", str(fixture)],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if process.returncode != 0:
        return {
            "file": str(fixture),
            "ok": False,
            "roundtrip_ok": False,
            "diagnostics": 1,
            "diagnostic_ids": [],
            "stderr": process.stderr,
        }
    parsed = json.loads(process.stdout)
    return {
        "file": str(fixture),
        "ok": bool(parsed["ok"]),
        "roundtrip_ok": bool(parsed["roundtrip_ok"]),
        "diagnostics": len(parsed["diagnostics"]),
        "diagnostic_ids": [diagnostic["id"] for diagnostic in parsed["diagnostics"]],
    }


def load_allowlist(path: Path) -> dict[str, dict[str, object]]:
    if not path.exists():
        raise RuntimeError(f"known gap allowlist is missing: {path}")
    with path.open("rb") as handle:
        data = tomllib.load(handle)

    gaps: dict[str, dict[str, object]] = {}
    for index, gap in enumerate(data.get("gap", []), start=1):
        fixture = gap.get("fixture")
        if not isinstance(fixture, str) or not fixture:
            raise RuntimeError(f"{path}: gap #{index} is missing a fixture")
        if fixture in gaps:
            raise RuntimeError(f"{path}: duplicate gap fixture: {fixture}")
        gaps[fixture] = gap
    return gaps


def relative_fixture(path: Path) -> str:
    return path.relative_to(ROOT).as_posix()


def is_allowed(
    fixture: str,
    reference: dict[str, object],
    rust: dict[str, object],
    allowlist: dict[str, dict[str, object]],
) -> bool:
    gap = allowlist.get(fixture)
    if gap is None:
        return False

    if "reference_ok" in gap and bool(gap["reference_ok"]) != bool(reference["ok"]):
        return False
    if "rust_ok" in gap and bool(gap["rust_ok"]) != bool(rust["ok"]):
        return False
    if "roundtrip_ok" in gap and bool(gap["roundtrip_ok"]) != bool(rust["roundtrip_ok"]):
        return False
    return True


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--fixture-root", default=str(run_parser_fixtures.DEFAULT_FIXTURE_ROOT))
    parser.add_argument("--allowlist", default=str(DEFAULT_ALLOWLIST))
    parser.add_argument(
        "--strict",
        action="store_true",
        help="retained for compatibility; parser-diff is strict by default",
    )
    args = parser.parse_args()

    php, warning = run_parser_fixtures.find_reference_php()
    if php is None:
        if os.environ.get("REFERENCE_PHP"):
            print(f"[fail] {warning}", file=sys.stderr)
            return 1
        print(f"[skip] {warning}")
        return 0
    if warning:
        print(f"[warn] {warning}")
    version = run_parser_fixtures.php_version(php)
    if version != run_parser_fixtures.EXPECTED_PHP_VERSION:
        message = (
            "parser acceptance comparison requires "
            f"PHP {run_parser_fixtures.EXPECTED_PHP_VERSION}; "
            f"{php} reports {version or 'unknown'}"
        )
        if os.environ.get("REFERENCE_PHP"):
            print(f"[fail] {message}", file=sys.stderr)
            return 1
        print(
            f"[skip] {message}"
        )
        return 0

    try:
        allowlist = load_allowlist(Path(args.allowlist))
    except RuntimeError as error:
        print(f"[fail] {error}", file=sys.stderr)
        return 1

    mismatches = []
    allowed = []
    fixtures = run_parser_fixtures.iter_fixtures(Path(args.fixture_root))
    for fixture in fixtures:
        reference = run_parser_fixtures.run_oracle(php, fixture)
        rust = rust_parse_result(fixture)
        rel_fixture = relative_fixture(fixture)
        matches = bool(reference["ok"]) == bool(rust["ok"]) and bool(rust["roundtrip_ok"])
        allowed_gap = not matches and is_allowed(rel_fixture, reference, rust, allowlist)
        status = "ok" if matches else "allowed" if allowed_gap else "mismatch"
        print(
            f"[{status}] {rel_fixture}: "
            f"reference_ok={reference['ok']} rust_ok={rust['ok']} "
            f"rust_diagnostics={rust['diagnostics']} "
            f"roundtrip_ok={rust['roundtrip_ok']}"
        )
        if allowed_gap:
            allowed.append(rel_fixture)
        elif not matches:
            mismatches.append(rel_fixture)

    stale_allowlist = sorted(set(allowlist) - set(allowed))
    if stale_allowlist:
        print("[fail] stale parser known-gap allowlist entrie(s):")
        for fixture in stale_allowlist:
            print(f"  {fixture}")
        return 1

    if mismatches:
        print(f"[fail] {len(mismatches)} parser acceptance mismatch(es):")
        for fixture in mismatches:
            print(f"  {fixture}")
        return 1

    print(
        f"[info] compared {len(fixtures)} parser fixture(s); "
        f"allowed gaps={len(allowed)}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
