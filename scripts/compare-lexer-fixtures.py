#!/usr/bin/env python3
"""Differential fixture harness for Lexer lexer work."""

from __future__ import annotations

import argparse
import fnmatch
import json
import os
import shutil
import subprocess
import sys
import tomllib
from dataclasses import dataclass
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
FIXTURE_DIR = ROOT / "tests" / "fixtures" / "lexer"
TOKENIZE_SCRIPT = ROOT / "scripts" / "tokenize-reference.php"
PINNED_PHP = ROOT / "third_party" / "php-src" / "sapi" / "cli" / "php"
DEFAULT_ALLOWLIST = ROOT / "tests" / "fixtures" / "lexer" / "allowlist.toml"
EXPECTED_PHP_VERSION = "8.5.7"


@dataclass(frozen=True)
class TokenDiff:
    fixture: str
    index: int
    field: str
    reference: Any
    rust: Any
    reference_token: dict[str, Any] | None
    rust_token: dict[str, Any] | None


@dataclass(frozen=True)
class AllowRule:
    fixture: str
    reason: str

    def matches(self, fixture: str) -> bool:
        return fnmatch.fnmatch(fixture, self.fixture)


def find_php() -> str | None:
    env_php = os.environ.get("REFERENCE_PHP")
    if env_php:
        return env_php
    if PINNED_PHP.exists() and os.access(PINNED_PHP, os.X_OK):
        return str(PINNED_PHP)
    return shutil.which("php")


def php_version(php: str) -> str | None:
    process = subprocess.run(
        [php, "-r", "echo PHP_VERSION;"],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    if process.returncode != 0:
        return None
    return process.stdout.strip()


def tokenize_reference(php: str, fixture: Path) -> dict[str, Any]:
    completed = subprocess.run(
        [php, str(TOKENIZE_SCRIPT), "--file", str(fixture)],
        cwd=ROOT,
        check=True,
        text=True,
        capture_output=True,
    )
    return json.loads(completed.stdout)


def tokenize_rust(fixture: Path) -> dict[str, Any]:
    completed = subprocess.run(
        ["cargo", "run", "--quiet", "-p", "php_lexer_cli", "--", "--file", str(fixture)],
        cwd=ROOT,
        check=True,
        text=True,
        capture_output=True,
    )
    return json.loads(completed.stdout)


def compare_tokens(fixture: str, reference: dict[str, Any], rust: dict[str, Any]) -> list[TokenDiff]:
    reference_tokens = reference.get("tokens", [])
    rust_tokens = rust.get("tokens", [])
    diffs: list[TokenDiff] = []

    if len(reference_tokens) != len(rust_tokens):
        diffs.append(
            TokenDiff(
                fixture=fixture,
                index=min(len(reference_tokens), len(rust_tokens)),
                field="token_count",
                reference=len(reference_tokens),
                rust=len(rust_tokens),
                reference_token=None,
                rust_token=None,
            )
        )

    for index, (ref_token, rust_token) in enumerate(zip(reference_tokens, rust_tokens)):
        for field in ("kind", "text", "line", "start", "end"):
            if ref_token.get(field) != rust_token.get(field):
                diffs.append(
                    TokenDiff(
                        fixture=fixture,
                        index=index,
                        field=field,
                        reference=ref_token.get(field),
                        rust=rust_token.get(field),
                        reference_token=ref_token,
                        rust_token=rust_token,
                    )
                )
                break

    return diffs


def load_allowlist(path: Path | None) -> list[AllowRule]:
    if path is None:
        return []
    if not path.exists():
        raise FileNotFoundError(f"allowlist does not exist: {path}")

    data = tomllib.loads(path.read_text(encoding="utf-8"))
    rules = []
    for entry in data.get("rule", []):
        fixture = entry.get("fixture")
        reason = entry.get("reason")
        if not fixture or not reason:
            raise ValueError("each allowlist rule needs fixture and reason")
        rules.append(AllowRule(fixture=fixture, reason=reason))
    return rules


def allowance_for(fixture: str, rules: list[AllowRule]) -> AllowRule | None:
    for rule in rules:
        if rule.matches(fixture):
            return rule
    return None


def print_diff(diff: TokenDiff) -> None:
    print(
        f"[diff] {diff.fixture}: token {diff.index} {diff.field}: "
        f"reference={diff.reference!r} rust={diff.rust!r}",
        file=sys.stderr,
    )
    if diff.reference_token is not None:
        print(
            "       reference token: "
            f"kind={diff.reference_token.get('kind')!r} "
            f"text={diff.reference_token.get('text')!r} "
            f"line={diff.reference_token.get('line')!r}",
            file=sys.stderr,
        )
    if diff.rust_token is not None:
        print(
            "       rust token:      "
            f"kind={diff.rust_token.get('kind')!r} "
            f"text={diff.rust_token.get('text')!r} "
            f"line={diff.rust_token.get('line')!r}",
            file=sys.stderr,
        )


def report_payload(results: list[dict[str, Any]]) -> dict[str, Any]:
    return {
        "fixtures": results,
        "summary": {
            "total": len(results),
            "matched": sum(1 for result in results if result["status"] == "matched"),
            "allowed": sum(1 for result in results if result["status"] == "allowed"),
            "failed": sum(1 for result in results if result["status"] == "failed"),
        },
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--all-diffs", action="store_true", help="print all diffs")
    parser.add_argument("--json-report", type=Path, help="write machine-readable report")
    parser.add_argument(
        "--allowlist",
        nargs="?",
        const=DEFAULT_ALLOWLIST,
        type=Path,
        help="allow documented fixture-level differences",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    php = find_php()
    if php is None:
        print(
            "[skip] no PHP binary found; set REFERENCE_PHP or build the pinned reference",
            file=sys.stderr,
        )
        return 0

    version = php_version(php)
    if version != EXPECTED_PHP_VERSION:
        print(
            f"[skip] lexer fixture comparison requires PHP {EXPECTED_PHP_VERSION}; "
            f"{php} reports {version or 'unknown'}",
            file=sys.stderr,
        )
        return 0

    try:
        allow_rules = load_allowlist(args.allowlist)
    except (FileNotFoundError, ValueError, tomllib.TOMLDecodeError) as error:
        print(f"[fail] invalid allowlist: {error}", file=sys.stderr)
        return 1

    fixtures = sorted(FIXTURE_DIR.glob("*.php"))
    if not fixtures:
        print(f"[fail] no lexer fixtures found in {FIXTURE_DIR}", file=sys.stderr)
        return 1

    results: list[dict[str, Any]] = []
    failed = False

    for fixture in fixtures:
        rel = str(fixture.relative_to(ROOT))
        try:
            reference_tokens = tokenize_reference(php, fixture)
            rust_tokens = tokenize_rust(fixture)
        except subprocess.CalledProcessError as error:
            sys.stderr.write(error.stderr)
            print(f"[fail] tokenization failed: {rel}", file=sys.stderr)
            return error.returncode or 1
        except json.JSONDecodeError as error:
            print(f"[fail] invalid JSON for {rel}: {error}", file=sys.stderr)
            return 1

        diffs = compare_tokens(rel, reference_tokens, rust_tokens)
        rule = allowance_for(rel, allow_rules)

        if not diffs:
            print(f"[ok] {rel}: {len(reference_tokens.get('tokens', []))} tokens match")
            results.append({"fixture": rel, "status": "matched", "diff_count": 0})
            continue

        if rule is not None:
            print(f"[allow] {rel}: {len(diffs)} known diffs ({rule.reason})")
            results.append(
                {
                    "fixture": rel,
                    "status": "allowed",
                    "diff_count": len(diffs),
                    "reason": rule.reason,
                }
            )
            continue

        failed = True
        print_diff(diffs[0])
        if args.all_diffs:
            for diff in diffs[1:]:
                print_diff(diff)
        results.append({"fixture": rel, "status": "failed", "diff_count": len(diffs)})
        if not args.all_diffs:
            break

    if args.json_report:
        args.json_report.parent.mkdir(parents=True, exist_ok=True)
        args.json_report.write_text(
            json.dumps(report_payload(results), indent=2, sort_keys=True),
            encoding="utf-8",
        )

    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
