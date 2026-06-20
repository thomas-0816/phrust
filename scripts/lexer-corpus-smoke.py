#!/usr/bin/env python3
"""Extract --FILE-- sections from php-src .phpt tests and smoke-test the lexer."""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PHP_SRC = ROOT / "third_party" / "php-src"
TARGET = ROOT / "target" / "php-src-lexer-corpus"
FILE_SECTION = re.compile(r"^--FILE--\r?\n(?P<body>.*?)(?=^--[A-Z_]+--\r?$)", re.M | re.S)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--limit", type=int, default=25, help="maximum extracted files to test")
    return parser.parse_args()


def extract_file_section(path: Path) -> str | None:
    text = path.read_text(encoding="utf-8", errors="replace")
    match = FILE_SECTION.search(text)
    if match is None:
        return None
    return match.group("body")


def main() -> int:
    args = parse_args()
    if not PHP_SRC.exists():
        print("[skip] third_party/php-src is absent; corpus smoke skipped")
        return 0

    TARGET.mkdir(parents=True, exist_ok=True)
    found = 0
    extracted = 0
    tested = 0

    for phpt in sorted(PHP_SRC.rglob("*.phpt")):
        found += 1
        body = extract_file_section(phpt)
        if body is None:
            continue

        rel_name = phpt.relative_to(PHP_SRC).as_posix().replace("/", "__")
        out = TARGET / f"{extracted:04d}-{rel_name}.php"
        out.write_text(body, encoding="utf-8")
        extracted += 1

        completed = subprocess.run(
            ["cargo", "run", "--quiet", "-p", "php_lexer_cli", "--", "--file", str(out)],
            cwd=ROOT,
            text=True,
            capture_output=True,
        )
        if completed.returncode != 0:
            sys.stderr.write(completed.stderr)
            print(f"[fail] Rust lexer failed on extracted corpus file: {out}", file=sys.stderr)
            return completed.returncode

        tested += 1
        if tested >= args.limit:
            break

    print(
        f"[ok] php-src corpus smoke: found={found} extracted={extracted} tested={tested} target={TARGET}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
