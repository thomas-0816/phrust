#!/usr/bin/env python3
"""Run the explicit mysqli live integration gate when a DSN is configured."""

from __future__ import annotations

import json
import os
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "target" / "mysqli-integration" / "report.json"
DSN_ENV = "PHRUST_MYSQL_TEST_DSN"


def write_report(payload: dict) -> None:
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")


def main() -> int:
    dsn = os.environ.get(DSN_ENV, "").strip()
    started_at = datetime.now(timezone.utc).isoformat()
    if not dsn:
        write_report(
            {
                "schema_version": 1,
                "status": "environment_blocker",
                "wordpress_error_class": "environment_blocker",
                "owner_layer": "database_mysqli",
                "diagnostic_id": "E_PHP_MYSQLI_INTEGRATION_DSN_MISSING",
                "database_context_summary": {
                    "required_env": DSN_ENV,
                    "connection_state": "not_configured",
                },
                "started_at": started_at,
            }
        )
        print(f"[skip] {DSN_ENV} is not set; wrote {OUT.relative_to(ROOT)}")
        return 0

    command = [
        "cargo",
        "test",
        "-p",
        "php_runtime",
        "db::mysql::tests::live_",
        "--",
        "--nocapture",
    ]
    completed = subprocess.run(command, cwd=ROOT, text=True)
    status = "pass" if completed.returncode == 0 else "fail"
    write_report(
        {
            "schema_version": 1,
            "status": status,
            "wordpress_error_class": "database_mysqli"
            if completed.returncode != 0
            else None,
            "owner_layer": "php_runtime::db::mysql",
            "diagnostic_id": None
            if completed.returncode == 0
            else "E_PHP_MYSQLI_INTEGRATION_FAILED",
            "database_context_summary": {
                "required_env": DSN_ENV,
                "connection_state": "configured",
                "dsn_redacted": redact_dsn(dsn),
            },
            "command": command,
            "started_at": started_at,
            "completed_at": datetime.now(timezone.utc).isoformat(),
        }
    )
    print(f"[{status}] mysqli integration report: {OUT.relative_to(ROOT)}")
    return completed.returncode


def redact_dsn(dsn: str) -> str:
    if "@" not in dsn:
        return dsn
    scheme, rest = dsn.split("://", 1) if "://" in dsn else ("", dsn)
    _, host = rest.rsplit("@", 1)
    prefix = f"{scheme}://" if scheme else ""
    return f"{prefix}<redacted>@{host}"


if __name__ == "__main__":
    sys.exit(main())
