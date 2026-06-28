#!/usr/bin/env python3
"""Run php-vm runtime fixtures from a JSONL manifest."""

from __future__ import annotations

import argparse
import difflib
import json
import os
import re
import subprocess
import sys
import tempfile
import unittest
from dataclasses import dataclass
from pathlib import Path
from typing import Any


LINE_NUMBER_RE = re.compile(r"on line \d+")


@dataclass(frozen=True)
class FixtureResult:
    fixture_id: str
    ok: bool
    detail: str


def load_manifest(path: Path) -> list[dict[str, Any]]:
    cases: list[dict[str, Any]] = []
    seen: set[str] = set()
    with path.open("r", encoding="utf-8") as handle:
        for line_number, line in enumerate(handle, start=1):
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            try:
                case = json.loads(line)
            except json.JSONDecodeError as error:
                raise SystemExit(f"{path}:{line_number}: invalid JSON: {error}") from error
            fixture_id = case.get("id")
            if not isinstance(fixture_id, str) or not fixture_id:
                raise SystemExit(f"{path}:{line_number}: case id must be a non-empty string")
            if fixture_id in seen:
                raise SystemExit(f"{path}:{line_number}: duplicate case id {fixture_id}")
            fixture_path = case.get("path")
            if not isinstance(fixture_path, str) or not fixture_path:
                raise SystemExit(f"{path}:{line_number}: path must be a non-empty string")
            expect = case.get("expect")
            if not isinstance(expect, dict):
                raise SystemExit(f"{path}:{line_number}: expect must be an object")
            seen.add(fixture_id)
            cases.append(case)
    return cases


def normalize(text: str, normalizers: list[str]) -> str:
    normalized = text
    for normalizer in normalizers:
        if normalizer == "line_numbers":
            normalized = LINE_NUMBER_RE.sub("on line <line>", normalized)
        else:
            raise ValueError(f"unknown normalizer: {normalizer}")
    return normalized


def expand(value: str, repo: Path) -> str:
    return value.replace("{repo}", str(repo))


def unified_diff(expected: str, actual: str, label: str) -> str:
    return "".join(
        difflib.unified_diff(
            expected.splitlines(keepends=True),
            actual.splitlines(keepends=True),
            fromfile=f"expected {label}",
            tofile=f"actual {label}",
        )
    )


def check_stream(
    *,
    stream_name: str,
    actual: str,
    expect: dict[str, Any],
    repo: Path,
) -> list[str]:
    failures: list[str] = []
    normalizers = expect.get(f"normalize_{stream_name}", [])
    if not isinstance(normalizers, list) or not all(isinstance(item, str) for item in normalizers):
        failures.append(f"normalize_{stream_name} must be a string list")
        normalizers = []
    actual = normalize(actual, normalizers)

    exact = expect.get(stream_name)
    if isinstance(exact, str):
        expected = expand(exact, repo)
        if actual != expected:
            failures.append(unified_diff(expected, actual, stream_name))

    contains_key = f"{stream_name}_contains"
    contains = expect.get(contains_key, [])
    if isinstance(contains, str):
        contains = [contains]
    if not isinstance(contains, list) or not all(isinstance(item, str) for item in contains):
        failures.append(f"{contains_key} must be a string or string list")
    else:
        for needle in contains:
            expanded = expand(needle, repo)
            if expanded not in actual:
                failures.append(f"{stream_name} missing substring: {expanded!r}")

    line_key = f"{stream_name}_lines_contains"
    lines_contains = expect.get(line_key, [])
    if isinstance(lines_contains, str):
        lines_contains = [lines_contains]
    if not isinstance(lines_contains, list) or not all(
        isinstance(item, str) for item in lines_contains
    ):
        failures.append(f"{line_key} must be a string or string list")
    elif lines_contains:
        lines = actual.splitlines()
        for expected_line in lines_contains:
            expanded = expand(expected_line, repo)
            if expanded not in lines:
                failures.append(f"{stream_name} missing line: {expanded!r}")

    return failures


def run_case(case: dict[str, Any], *, php_vm: Path, repo: Path, out_dir: Path) -> FixtureResult:
    fixture_id = case["id"]
    fixture_path = repo / case["path"]
    args = case.get("args", [])
    if not isinstance(args, list) or not all(isinstance(item, str) for item in args):
        return FixtureResult(fixture_id, False, "args must be a string list")
    command = [str(php_vm), "run", str(fixture_path), *args]
    completed = subprocess.run(
        command,
        cwd=repo,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    stdout = completed.stdout.decode("utf-8", errors="replace")
    stderr = completed.stderr.decode("utf-8", errors="replace")

    safe_id = re.sub(r"[^A-Za-z0-9_.-]+", "-", fixture_id)
    (out_dir / f"{safe_id}.stdout").write_text(stdout, encoding="utf-8")
    (out_dir / f"{safe_id}.stderr").write_text(stderr, encoding="utf-8")

    expect = case["expect"]
    failures: list[str] = []
    expected_exit = expect.get("exit", 0)
    if completed.returncode != expected_exit:
        failures.append(f"exit code: expected {expected_exit}, got {completed.returncode}")
    failures.extend(check_stream(stream_name="stdout", actual=stdout, expect=expect, repo=repo))
    failures.extend(check_stream(stream_name="stderr", actual=stderr, expect=expect, repo=repo))

    if failures:
        detail = "\n".join(failure for failure in failures if failure)
        return FixtureResult(fixture_id, False, detail)
    return FixtureResult(fixture_id, True, "ok")


def write_reports(results: list[FixtureResult], out_dir: Path) -> None:
    payload = [
        {"id": result.fixture_id, "ok": result.ok, "detail": result.detail}
        for result in results
    ]
    (out_dir / "report.json").write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    lines = ["# Runtime Fixture Report", ""]
    for result in results:
        status = "PASS" if result.ok else "FAIL"
        lines.append(f"- {status} `{result.fixture_id}`")
        if not result.ok:
            lines.append("")
            lines.append("```text")
            lines.append(result.detail)
            lines.append("```")
            lines.append("")
    (out_dir / "report.md").write_text("\n".join(lines) + "\n", encoding="utf-8")


def run_manifest(manifest: Path, php_vm: Path, repo: Path, out_dir: Path) -> int:
    cases = load_manifest(manifest)
    out_dir.mkdir(parents=True, exist_ok=True)
    results = [run_case(case, php_vm=php_vm, repo=repo, out_dir=out_dir) for case in cases]
    write_reports(results, out_dir)
    failures = [result for result in results if not result.ok]
    if failures:
        print(f"[fail] {len(failures)} of {len(results)} runtime fixtures failed", file=sys.stderr)
        for failure in failures[:10]:
            print(f"\n[{failure.fixture_id}]\n{failure.detail}", file=sys.stderr)
        print(f"\nFull report: {out_dir / 'report.md'}", file=sys.stderr)
        return 1
    print(f"[ok] runtime fixtures passed ({len(results)} cases).")
    return 0


class RunnerTests(unittest.TestCase):
    def test_line_number_normalizer(self) -> None:
        self.assertEqual(
            normalize("Warning on line 42\nFatal on line 7\n", ["line_numbers"]),
            "Warning on line <line>\nFatal on line <line>\n",
        )

    def test_manifest_validation_rejects_duplicate_ids(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "manifest.jsonl"
            path.write_text(
                '{"id":"same","path":"a.php","expect":{}}\n'
                '{"id":"same","path":"b.php","expect":{}}\n',
                encoding="utf-8",
            )
            with self.assertRaises(SystemExit):
                load_manifest(path)

    def test_stream_contains_and_line_checks(self) -> None:
        failures = check_stream(
            stream_name="stdout",
            actual="alpha\nbeta\n",
            expect={"stdout_contains": "alp", "stdout_lines_contains": ["beta"]},
            repo=Path("/repo"),
        )
        self.assertEqual(failures, [])


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", default="tests/runtime/manifests/runtime-fixtures.jsonl")
    parser.add_argument("--php-vm", default=None)
    parser.add_argument("--out", default="target/runtime-fixtures")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        suite = unittest.defaultTestLoader.loadTestsFromTestCase(RunnerTests)
        result = unittest.TextTestRunner(verbosity=2).run(suite)
        return 0 if result.wasSuccessful() else 1

    repo = Path.cwd().resolve()
    target_dir = Path(os.environ.get("CARGO_TARGET_DIR", "target"))
    php_vm = Path(args.php_vm) if args.php_vm else target_dir / "debug" / "php-vm"
    return run_manifest(Path(args.manifest), php_vm, repo, Path(args.out))


if __name__ == "__main__":
    raise SystemExit(main())
