#!/usr/bin/env python3
"""Classify the environment for real WordPress smoke runs."""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Any

SCRIPT_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPT_DIR))

from common import (  # noqa: E402
    REPO_ROOT,
    binary_is_stale,
    canonical_path,
    docker_available,
    executable,
    is_port_available,
    json_dump,
    mysql_credentials_valid,
    parse_mysql_dsn,
    repo_path,
    tcp_reachable,
    wordpress_shape_blockers,
)


PHP_VM_BINARY_SOURCE_ROOTS = (
    "Cargo.lock",
    "Cargo.toml",
    "crates/php_ast",
    "crates/php_diagnostics",
    "crates/php_executor",
    "crates/php_ir",
    "crates/php_jit",
    "crates/php_lexer",
    "crates/php_optimizer",
    "crates/php_runtime",
    "crates/php_semantics",
    "crates/php_source",
    "crates/php_std",
    "crates/php_syntax",
    "crates/php_vm",
    "crates/php_vm_cli",
)

PHRUST_SERVER_BINARY_SOURCE_ROOTS = (
    "Cargo.lock",
    "Cargo.toml",
    "crates/php_ast",
    "crates/php_diagnostics",
    "crates/php_executor",
    "crates/php_ir",
    "crates/php_lexer",
    "crates/php_optimizer",
    "crates/php_runtime",
    "crates/php_semantics",
    "crates/php_server",
    "crates/php_source",
    "crates/php_std",
    "crates/php_syntax",
    "crates/php_vm",
)


def main() -> int:
    args = parse_args()
    report = build_report(args)
    if args.out:
        out = repo_path(args.out)
        assert out is not None
        if out.suffix:
            json_dump(report, out)
        else:
            json_dump(report, out / "preflight.json")
    print(json.dumps(report, indent=2, sort_keys=True))
    return 1 if report["status"] == "fail" else 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--wordpress-dir", default=os.environ.get("PHRUST_WORDPRESS_DIR", ""))
    parser.add_argument("--docroot", default=os.environ.get("PHRUST_WORDPRESS_DOCROOT", ""))
    parser.add_argument("--reference-php", default=os.environ.get("REFERENCE_PHP", ""))
    parser.add_argument("--require-reference", action="store_true")
    parser.add_argument("--phrust-binary", default=os.environ.get("PHP_VM_CLI", "target/debug/php-vm"))
    parser.add_argument("--phrust-server", default=os.environ.get("PHRUST_SERVER", "target/debug/phrust-server"))
    parser.add_argument("--db-enabled", action="store_true", default=os.environ.get("PHRUST_WORDPRESS_DB", "0") == "1")
    parser.add_argument("--db-dsn-env", default="PHRUST_MYSQL_TEST_DSN")
    parser.add_argument("--listen", default=os.environ.get("PHRUST_WORDPRESS_LISTEN", "127.0.0.1:18080"))
    parser.add_argument("--out", default="")
    return parser.parse_args()


def build_report(args: argparse.Namespace) -> dict[str, Any]:
    blockers: list[str] = []
    warnings: list[str] = []

    wordpress_dir = repo_path(args.wordpress_dir)
    docroot = repo_path(args.docroot) or wordpress_dir
    reference_php = repo_path(args.reference_php)
    phrust_binary = repo_path(args.phrust_binary)
    phrust_server = repo_path(args.phrust_server)

    blockers.extend(wordpress_shape_blockers(wordpress_dir))
    wordpress_missing = "missing_wordpress_checkout" in blockers
    wordpress_canonical = canonical_path(wordpress_dir) if wordpress_dir else None
    docroot_canonical = canonical_path(docroot) if docroot else None
    if wordpress_missing and args.docroot:
        if docroot is None or docroot_canonical is None or not docroot_canonical.is_dir():
            blockers.append("document_root_invalid")
    elif not wordpress_missing and (docroot is None or docroot_canonical is None or not docroot_canonical.is_dir()):
        blockers.append("document_root_invalid")
    elif (
        not wordpress_missing
        and wordpress_canonical is not None
        and not docroot_canonical.is_relative_to(wordpress_canonical)
    ):
        blockers.append("document_root_invalid")

    if args.require_reference or args.reference_php:
        if reference_php is None or not reference_php.exists():
            blockers.append("missing_reference_php")
        elif not executable(reference_php):
            blockers.append("invalid_reference_php")
        else:
            version = reference_version(reference_php)
            if version is None:
                blockers.append("invalid_reference_php")

    if not executable(phrust_binary) or binary_is_stale(phrust_binary, PHP_VM_BINARY_SOURCE_ROOTS):
        blockers.append("missing_php_vm_binary_or_stale_binary")
    if not executable(phrust_server) or binary_is_stale(
        phrust_server, PHRUST_SERVER_BINARY_SOURCE_ROOTS
    ):
        blockers.append("phrust_server_unavailable")

    host, port = parse_listen(args.listen)
    if port is not None and not is_port_available(host, port):
        blockers.append("port_unavailable")

    dsn = os.environ.get(args.db_dsn_env, "")
    if args.db_enabled:
        docker_ok = docker_available()
        if not dsn:
            blockers.append("missing_mysql_dsn")
            if not docker_ok:
                blockers.append("docker_unavailable")
        if dsn:
            mysql = parse_mysql_dsn(dsn)
            if not tcp_reachable(mysql["host"], mysql["port"]):
                blockers.append("mariadb_unavailable")
                if not docker_ok:
                    blockers.append("docker_unavailable")
            else:
                if not docker_ok:
                    warnings.append("docker_unavailable_dsn_tcp_reachable")
                credentials_valid = mysql_credentials_valid(dsn)
                if credentials_valid is False:
                    blockers.append("mariadb_credentials_invalid")
                elif credentials_valid is None:
                    warnings.append("mysql_client_unavailable_credentials_not_validated")

    status = "ok"
    if blockers:
        status = "skip"
        hard_failures = {"invalid_reference_php", "document_root_invalid", "mariadb_credentials_invalid"}
        if "missing_wordpress_checkout" not in blockers and any(
            blocker in hard_failures for blocker in blockers
        ):
            status = "fail"

    return {
        "status": status,
        "environment_blockers": sorted(set(blockers)),
        "warnings": warnings,
        "inputs": {
            "wordpress_dir": str(wordpress_canonical or wordpress_dir or ""),
            "reference_php": str(reference_php or ""),
            "phrust_binary": str(phrust_binary or ""),
            "phrust_server": str(phrust_server or ""),
            "docroot": str(docroot_canonical or docroot or ""),
            "db_enabled": bool(args.db_enabled),
            "db_dsn_env": args.db_dsn_env,
            "db_dsn_present": bool(dsn),
            "listen": args.listen,
            "repo_root": str(REPO_ROOT),
        },
    }


def reference_version(reference_php: Path) -> str | None:
    try:
        result = subprocess.run(
            [str(reference_php), "-r", "echo PHP_VERSION;"],
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            timeout=5,
            check=True,
        )
    except (OSError, subprocess.CalledProcessError, subprocess.TimeoutExpired):
        return None
    return result.stdout.strip() or None


def parse_listen(value: str) -> tuple[str, int | None]:
    if ":" not in value:
        return ("127.0.0.1", None)
    host, port_text = value.rsplit(":", 1)
    try:
        return (host, int(port_text))
    except ValueError:
        return (host, None)


if __name__ == "__main__":
    raise SystemExit(main())
