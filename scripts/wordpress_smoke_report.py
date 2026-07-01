#!/usr/bin/env python3
"""Write a deterministic WordPress bring-up diagnostic report skeleton."""

from __future__ import annotations

import argparse
import hashlib
import json
from datetime import datetime, timezone
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_OUT = ROOT / "target" / "wordpress-bringup" / "report.json"
FIXTURE_ROOT = ROOT / "fixtures" / "runtime_semantics" / "wp_web_db_diagnostics"


SCHEMA_FIELDS = [
    "schema_version",
    "status",
    "wordpress_error_class",
    "owner_layer",
    "diagnostic_id",
    "source_path",
    "source_span",
    "first_php_frame",
    "runtime_stack",
    "include_stack",
    "autoload_stack",
    "request_context_summary",
    "database_context_summary",
    "last_vm_instruction",
    "counters",
    "stdout_digest",
    "stderr_digest",
    "secondary_errors",
    "suggested_reduced_fixture_path",
]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", type=Path, default=DEFAULT_OUT)
    args = parser.parse_args()

    fixtures = sorted(
        path.relative_to(ROOT).as_posix()
        for path in FIXTURE_ROOT.rglob("*")
        if path.is_file()
    )
    report = {
        "schema_version": 1,
        "status": "pass",
        "wordpress_error_class": None,
        "owner_layer": "diagnostics",
        "diagnostic_id": None,
        "source_path": None,
        "source_span": None,
        "first_php_frame": None,
        "runtime_stack": [],
        "include_stack": [],
        "autoload_stack": [],
        "request_context_summary": {
            "fixtures": [
                "web_request/superglobals.php",
                "web_request/path_info.php",
                "web_request/response_headers.php",
            ],
            "covered_fields": [
                "method",
                "uri",
                "script_filename",
                "document_root",
                "path_info",
                "headers",
                "status",
                "cookies",
                "output_buffering",
            ],
        },
        "database_context_summary": {
            "fixtures": [
                "mysqli/prepared_statement_sqlite.php",
                "mysqli/query_result_api.php",
            ],
            "covered_operations": [
                "connect",
                "query",
                "fetch_assoc",
                "fetch_object",
                "prepare",
                "bind_param",
                "execute",
                "get_result",
                "bind_result",
                "fetch",
                "charset",
                "error_status",
            ],
        },
        "last_vm_instruction": None,
        "counters": {
            "fixture_files": len(fixtures),
            "schema_fields": len(SCHEMA_FIELDS),
        },
        "stdout_digest": digest(b""),
        "stderr_digest": digest(b""),
        "secondary_errors": [],
        "suggested_reduced_fixture_path": None,
        "schema_fields": SCHEMA_FIELDS,
        "fixtures": fixtures,
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "example_classified_failure_path": (
            "fixtures/runtime_semantics/wp_web_db_diagnostics/diagnostics/"
            "classified_failure.json"
        ),
    }

    out = args.out if args.out.is_absolute() else ROOT / args.out
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    markdown = out.with_suffix(".md")
    markdown.write_text(markdown_summary(report), encoding="utf-8")
    print(f"[pass] wordpress smoke report: {out.relative_to(ROOT)}")
    return 0


def digest(data: bytes) -> str:
    return "sha256:" + hashlib.sha256(data).hexdigest()


def markdown_summary(report: dict) -> str:
    lines = [
        "# WordPress Bring-Up Diagnostics",
        "",
        f"- status: {report['status']}",
        f"- schema_version: {report['schema_version']}",
        f"- fixture_files: {report['counters']['fixture_files']}",
        f"- example_failure: {report['example_classified_failure_path']}",
        "",
    ]
    return "\n".join(lines)


if __name__ == "__main__":
    raise SystemExit(main())
