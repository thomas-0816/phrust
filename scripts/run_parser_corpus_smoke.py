#!/usr/bin/env python3
"""Run an optional parser smoke test over an extracted php-src corpus."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

import extract_parser_corpus
import run_parser_fixtures


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_OUTPUT = ROOT / "target" / "parser-corpus-smoke" / "extracted"


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
            "diagnostic_ids": ["harness-error"],
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


def summarize_deviation(
    entry: dict[str, str],
    reference: dict[str, object],
    rust: dict[str, object],
) -> dict[str, object]:
    return {
        "source": entry["source"],
        "extracted": entry["extracted"],
        "reference_ok": bool(reference["ok"]),
        "rust_ok": bool(rust["ok"]),
        "roundtrip_ok": bool(rust["roundtrip_ok"]),
        "rust_diagnostics": int(rust["diagnostics"]),
        "diagnostic_ids": rust["diagnostic_ids"],
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--php-src", help="php-src checkout; defaults to PHP_SRC_DIR or third_party/php-src")
    parser.add_argument("--out", default=str(DEFAULT_OUTPUT))
    parser.add_argument("--max-files", type=int, default=extract_parser_corpus.DEFAULT_MAX_FILES)
    parser.add_argument("--top", type=int, default=10, help="number of deviations to print")
    parser.add_argument(
        "--fail-on-mismatch",
        action="store_true",
        help="exit non-zero when corpus acceptance or roundtrip deviations are found",
    )
    args = parser.parse_args()

    php_src = extract_parser_corpus.find_php_src_dir(args.php_src)
    if php_src is None:
        print("[skip] no php-src checkout found; set PHP_SRC_DIR or --php-src")
        return 0

    php, warning = run_parser_fixtures.find_reference_php()
    if php is None:
        print(f"[skip] {warning}")
        return 0
    if warning:
        print(f"[warn] {warning}")

    version = run_parser_fixtures.php_version(php)
    if version != run_parser_fixtures.EXPECTED_PHP_VERSION:
        print(
            "[skip] parser corpus smoke requires "
            f"PHP {run_parser_fixtures.EXPECTED_PHP_VERSION}; "
            f"{php} reports {version or 'unknown'}"
        )
        return 0

    output = Path(args.out)
    manifest = extract_parser_corpus.extract_corpus(
        php_src,
        output,
        max_files=max(args.max_files, 0),
    )
    if not manifest:
        print(f"[skip] no parser corpus files extracted from {php_src}")
        return 0

    deviations: list[dict[str, object]] = []
    for entry in manifest:
        extracted = ROOT / entry["extracted"]
        reference = run_parser_fixtures.run_oracle(php, extracted)
        rust = rust_parse_result(extracted)
        matches = bool(reference["ok"]) == bool(rust["ok"]) and bool(rust["roundtrip_ok"])
        if not matches:
            deviations.append(summarize_deviation(entry, reference, rust))

    print(f"[info] php-src: {php_src}")
    print(f"[info] extracted corpus: {output}")
    print(f"[info] checked {len(manifest)} parser corpus file(s)")
    print(f"[info] deviations: {len(deviations)}")

    if deviations:
        print(f"[info] top {min(args.top, len(deviations))} deviation(s):")
        for deviation in deviations[: max(args.top, 0)]:
            print(
                "  "
                f"{deviation['source']}: "
                f"reference_ok={deviation['reference_ok']} "
                f"rust_ok={deviation['rust_ok']} "
                f"roundtrip_ok={deviation['roundtrip_ok']} "
                f"rust_diagnostics={deviation['rust_diagnostics']} "
                f"ids={','.join(deviation['diagnostic_ids'])}"
            )

    report = {
        "php_src": str(php_src),
        "checked": len(manifest),
        "deviations": deviations,
    }
    report_path = output.parent / "parser-corpus-smoke-report.json"
    report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"[info] report: {report_path.relative_to(ROOT)}")

    if deviations and args.fail_on_mismatch:
        print("[fail] parser corpus deviations found", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
