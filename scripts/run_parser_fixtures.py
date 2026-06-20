#!/usr/bin/env python3
"""Run parser fixtures through the PHP lint oracle."""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_FIXTURE_ROOT = ROOT / "fixtures" / "parser"
EXPECTED_PHP_VERSION = "8.5.7"


def find_reference_php() -> tuple[Path | None, str | None]:
    configured = os.environ.get("REFERENCE_PHP")
    if configured:
        path = Path(configured)
        if path.exists() and os.access(path, os.X_OK):
            return path, None
        return None, f"REFERENCE_PHP is set but not executable: {path}"

    local = ROOT / "third_party" / "php-src" / "sapi" / "cli" / "php"
    if local.exists() and os.access(local, os.X_OK):
        return local, None

    system = shutil.which("php")
    if system:
        return Path(system), "using php from PATH; this may not be PHP 8.5.7"

    return None, "no PHP binary found; set REFERENCE_PHP or build the local reference"


def php_version(php: Path) -> str | None:
    process = subprocess.run(
        [str(php), "-r", "echo PHP_VERSION;"],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if process.returncode != 0:
        return None
    return process.stdout.strip()


def is_expected_reference(php: Path) -> bool:
    return php_version(php) == EXPECTED_PHP_VERSION


def iter_fixtures(root: Path) -> list[Path]:
    if not root.exists():
        return []
    return sorted(path for path in root.rglob("*.php") if path.is_file())


def run_oracle(php: Path, fixture: Path) -> dict[str, object]:
    script = ROOT / "scripts" / "reference_php_lint_json.php"
    process = subprocess.run(
        [str(php), str(script), "--file", str(fixture)],
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
            "exit_code": process.returncode,
            "stdout": process.stdout,
            "stderr": process.stderr,
            "php_version": "unknown",
            "harness_error": True,
        }
    return json.loads(process.stdout)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--fixture-root", default=str(DEFAULT_FIXTURE_ROOT))
    parser.add_argument("--json", action="store_true", help="emit JSON array")
    args = parser.parse_args()

    php, warning = find_reference_php()
    if php is None:
        if os.environ.get("REFERENCE_PHP"):
            print(f"[fail] {warning}", file=sys.stderr)
            return 1
        print(f"[skip] {warning}")
        return 0
    if warning:
        print(f"[warn] {warning}", file=sys.stderr)

    fixtures = iter_fixtures(Path(args.fixture_root))
    results = [run_oracle(php, fixture) for fixture in fixtures]

    if args.json:
        print(json.dumps(results, indent=2, sort_keys=True))
    else:
        print(f"[info] php lint oracle: {php}")
        for result in results:
            status = "ok" if result["ok"] else "not ok"
            print(f"[{status}] {result['file']}")
        print(f"[info] checked {len(results)} parser fixture(s)")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
