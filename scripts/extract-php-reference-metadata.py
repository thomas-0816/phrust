#!/usr/bin/env python3
"""Extract deterministic metadata from the pinned PHP reference checkout."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path


REFERENCE_PATHS = [
    "Zend/zend_language_scanner.l",
    "Zend/zend_language_parser.y",
    "Zend/zend_vm_def.h",
    "Zend/zend_ast.h",
    "Zend/zend_compile.h",
    "Zend/zend_types.h",
    "Zend/zend_exceptions.h",
    "Zend/zend_interfaces.c",
    "Zend/zend_builtin_functions.c",
    "Zend/tests",
    "tests",
]


def fail(message: str) -> None:
    print(f"error: {message}", file=sys.stderr)
    print(
        "hint: run `nix develop -c just bootstrap-ref` before extracting metadata",
        file=sys.stderr,
    )
    raise SystemExit(1)


def run_git(php_src: Path, args: list[str], default: str = "") -> str:
    try:
        result = subprocess.run(
            ["git", "-C", str(php_src), *args],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
        )
    except subprocess.CalledProcessError:
        return default
    return result.stdout.strip()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def file_metadata(root: Path, relative_path: str) -> dict[str, object]:
    path = root / relative_path
    if not path.is_file():
        fail(f"missing required reference file: {relative_path}")
    data = path.read_bytes()
    return {
        "kind": "file",
        "path": relative_path,
        "sha256": hashlib.sha256(data).hexdigest(),
        "bytes": len(data),
        "lines": data.count(b"\n") + (0 if data.endswith(b"\n") or not data else 1),
    }


def directory_metadata(root: Path, relative_path: str) -> dict[str, object]:
    path = root / relative_path
    if not path.is_dir():
        fail(f"missing required reference directory: {relative_path}")

    files: list[dict[str, object]] = []
    total_bytes = 0
    manifest_digest = hashlib.sha256()

    for child in sorted(path.rglob("*")):
        if not child.is_file():
            continue
        rel = child.relative_to(root).as_posix()
        size = child.stat().st_size
        digest = sha256_file(child)
        total_bytes += size
        files.append({"path": rel, "bytes": size, "sha256": digest})
        manifest_digest.update(rel.encode("utf-8"))
        manifest_digest.update(b"\0")
        manifest_digest.update(digest.encode("ascii"))
        manifest_digest.update(b"\0")

    return {
        "kind": "directory",
        "path": relative_path,
        "file_count": len(files),
        "total_bytes": total_bytes,
        "manifest_sha256": manifest_digest.hexdigest(),
    }


def extract(php_src: Path) -> dict[str, object]:
    if not php_src.exists():
        fail(f"PHP source checkout does not exist: {php_src}")
    if not (php_src / ".git").is_dir():
        fail(f"PHP source checkout is not a Git repository: {php_src}")

    paths: list[dict[str, object]] = []
    for relative_path in REFERENCE_PATHS:
        full_path = php_src / relative_path
        if full_path.is_dir():
            paths.append(directory_metadata(php_src, relative_path))
        else:
            paths.append(file_metadata(php_src, relative_path))

    branch = run_git(php_src, ["branch", "--show-current"])
    detached = branch == ""
    commit = run_git(php_src, ["rev-parse", "HEAD"])
    tag = run_git(php_src, ["describe", "--tags", "--exact-match", "HEAD"])

    return {
        "generated_at_utc": datetime.now(timezone.utc)
        .replace(microsecond=0)
        .isoformat()
        .replace("+00:00", "Z"),
        "git": {
            "repository": run_git(php_src, ["config", "--get", "remote.origin.url"]),
            "commit": commit,
            "tag": tag,
            "branch": branch,
            "detached": detached,
        },
        "paths": paths,
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--php-src", required=True, help="Path to php-src checkout")
    parser.add_argument("--out", required=True, help="Output JSON path")
    args = parser.parse_args()

    php_src = Path(args.php_src)
    out = Path(args.out)
    metadata = extract(php_src)

    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(metadata, indent=2, sort_keys=True) + "\n")
    print(f"wrote {out}")


if __name__ == "__main__":
    main()
