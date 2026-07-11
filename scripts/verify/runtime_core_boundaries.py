#!/usr/bin/env python3
"""Enforce the backend-free runtime core and inward extension direction."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
FORBIDDEN = {
    "curl",
    "ed25519-dalek",
    "exif",
    "gost94",
    "image",
    "imap",
    "ldap3",
    "libsodium-sys-stable",
    "libxml",
    "mysql",
    "openssl",
    "pcre2",
    "php_lexer",
    "php_syntax",
    "postgres",
    "rusqlite",
    "sha1",
    "sha2",
    "sha3",
    "ssh2",
    "suppaftp",
    "tiger",
    "whirlpool",
    "zip",
}
PILOTS = {"apcu", "ctype"}


def run(*args: str) -> str:
    result = subprocess.run(
        args,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode != 0:
        raise RuntimeError(result.stderr.strip() or f"{' '.join(args)} failed")
    return result.stdout


def main() -> int:
    failures: list[str] = []
    metadata = json.loads(run("cargo", "metadata", "--format-version=1", "--no-deps"))
    packages = {package["name"]: package for package in metadata["packages"]}
    runtime = packages["php_runtime"]
    dependencies = {dependency["name"]: dependency for dependency in runtime["dependencies"]}
    for name in sorted(FORBIDDEN & dependencies.keys()):
        if not dependencies[name]["optional"]:
            failures.append(f"php_runtime dependency {name} must be optional")

    tree = run(
        "cargo",
        "tree",
        "-p",
        "php_runtime",
        "--no-default-features",
        "--prefix",
        "none",
        "--format",
        "{p}",
    )
    core_packages = {line.split()[0] for line in tree.splitlines() if line.strip()}
    for name in sorted(FORBIDDEN & core_packages):
        failures.append(f"minimal runtime graph contains forbidden package {name}")

    runtime_dependencies = {item["name"] for item in dependencies.values()}
    if "php_extensions" in runtime_dependencies:
        failures.append("php_runtime must not depend on php_extensions")
    extension_dependencies = {
        dependency["name"] for dependency in packages["php_extensions"]["dependencies"]
    }
    if "php_runtime" not in extension_dependencies:
        failures.append("php_extensions must depend inward on php_runtime")

    registry = (ROOT / "crates/php_runtime/src/builtins/registry.rs").read_text()
    module_index = (ROOT / "crates/php_runtime/src/builtins/modules/mod.rs").read_text()
    for pilot in sorted(PILOTS):
        old_path = ROOT / f"crates/php_runtime/src/builtins/modules/{pilot}.rs"
        new_path = ROOT / f"crates/php_extensions/src/{pilot}.rs"
        if old_path.exists() or not new_path.exists():
            failures.append(f"pilot {pilot} must be owned only by php_extensions")
        if f"modules::{pilot}::ENTRIES" in registry or f"mod {pilot};" in module_index:
            failures.append(f"pilot {pilot} remains in the legacy runtime registry")

    if failures:
        print("[fail] runtime core boundaries:", file=sys.stderr)
        for failure in failures:
            print(f"  - {failure}", file=sys.stderr)
        return 1
    print(
        "[ok] runtime core excludes backend/frontend packages; "
        "extension pilots depend inward"
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, json.JSONDecodeError) as error:
        print(f"[fail] runtime core boundaries: {error}", file=sys.stderr)
        raise SystemExit(1) from error
