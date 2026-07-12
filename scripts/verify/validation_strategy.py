#!/usr/bin/env python3
"""Enforce one owner for broad Rust validation in aggregate gates."""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
RECIPE_RE = re.compile(r"^([A-Za-z0-9_-]+)(?:\s+[^:]*)?:(?:\s.*)?$")
JUST_CALL_RE = re.compile(r"(?:^|\s)@?just\s+([A-Za-z0-9_-]+)")

BROAD_RECIPE_CALLS = {"test", "check", "ci-rust"}

# These recipes are composed after ci-rust by ci-local. They must remain
# domain-focused so aggregate validation does not replay the Rust baseline.
FOCUSED_RECIPES = (
    "verify-frontend",
    "verify-runtime",
    "verify-stdlib",
    "verify-server",
    "verify-performance",
)


def parse_recipes(source: str) -> dict[str, list[str]]:
    recipes: dict[str, list[str]] = {}
    current: str | None = None
    for raw_line in source.splitlines():
        if raw_line and not raw_line[0].isspace():
            match = RECIPE_RE.match(raw_line)
            current = match.group(1) if match else None
            if current is not None:
                recipes[current] = []
            continue
        if current is not None and raw_line.strip():
            recipes[current].append(raw_line.strip())
    return recipes


def reachable_recipes(
    recipes: dict[str, list[str]], root: str
) -> set[str]:
    reachable: set[str] = set()
    pending = [root]
    while pending:
        recipe = pending.pop()
        if recipe in reachable:
            continue
        reachable.add(recipe)
        for line in recipes.get(recipe, []):
            for called in JUST_CALL_RE.findall(line):
                if called in recipes and called not in reachable:
                    pending.append(called)
    return reachable


def validation_errors(source: str) -> list[str]:
    recipes = parse_recipes(source)
    errors: list[str] = []

    missing = [
        name for name in ("test", "ci-local", *FOCUSED_RECIPES) if name not in recipes
    ]
    if missing:
        errors.append(f"missing required validation recipes: {', '.join(missing)}")
        return errors

    workspace_test_count = sum(
        "cargo test --workspace" in line for line in recipes["test"]
    )
    if workspace_test_count != 1:
        errors.append(
            "recipe 'test' must own exactly one 'cargo test --workspace' command"
        )

    ci_local = "\n".join(recipes["ci-local"])
    if ci_local.count("just ci-rust") != 1:
        errors.append("recipe 'ci-local' must invoke 'ci-rust' exactly once")
    if ci_local.count("just ci-domain-gates") != 1:
        errors.append("recipe 'ci-local' must invoke 'ci-domain-gates' exactly once")

    checked: set[str] = set()
    for root in FOCUSED_RECIPES:
        for recipe in sorted(reachable_recipes(recipes, root)):
            if recipe in checked:
                continue
            checked.add(recipe)
            for line in recipes[recipe]:
                if "cargo test --workspace" in line:
                    errors.append(
                        f"domain recipe '{root}' reaches '{recipe}', which replays "
                        "broad validation via 'cargo test --workspace'"
                    )
                for called in JUST_CALL_RE.findall(line):
                    if called in BROAD_RECIPE_CALLS:
                        errors.append(
                            f"domain recipe '{root}' reaches '{recipe}', which replays "
                            f"broad validation via 'just {called}'"
                        )

    return errors


def self_test() -> None:
    valid = """\
test:
    cargo test --workspace
ci-local:
    @just ci-rust
    @just ci-domain-gates
verify-frontend:
    @just frontend-fixtures
verify-runtime:
    @just runtime-fixtures
verify-stdlib:
    @just stdlib-fixtures
verify-server:
    cargo test -p php_server
verify-performance:
    @just performance-tests
performance-tests:
    scripts/performance/tool.py --self-test
"""
    assert validation_errors(valid) == []

    duplicate = valid.replace(
        "performance-tests:\n",
        "performance-tests:\n    @just nested-performance-tests\n",
    )
    duplicate += "nested-performance-tests:\n    cargo test --workspace\n"
    errors = validation_errors(duplicate)
    assert any("nested-performance-tests" in error for error in errors)

    missing_baseline = valid.replace("    @just ci-rust\n", "")
    errors = validation_errors(missing_baseline)
    assert any("ci-rust" in error for error in errors)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        self_test()
        print("[pass] validation strategy self-test")
        return 0

    errors = validation_errors((ROOT / "justfile").read_text(encoding="utf-8"))
    if errors:
        for error in errors:
            print(f"[fail] {error}", file=sys.stderr)
        return 1

    print("[pass] validation strategy keeps broad workspace tests single-owner")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
