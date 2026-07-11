#!/usr/bin/env python3
"""Report workspace dependency edges that cross PHP layer boundaries."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
ALLOWLIST = ROOT / "scripts/verify/dependency_boundary_allowlist.json"
REPORT_DIR = ROOT / "target/architecture"
REPORT_JSON = REPORT_DIR / "dependency-boundaries.json"
REPORT_MD = REPORT_DIR / "dependency-boundaries.md"

LAYER_INDEX = {
    "php_diagnostics": 0,
    "php_source": 0,
    "php_lexer": 1,
    "php_syntax": 2,
    "php_ast": 3,
    "php_semantics": 4,
    "php_ir": 5,
    "php_optimizer": 6,
    "php_runtime": 7,
    "php_extensions": 8,
    "php_std": 8,
    "php_vm": 9,
    "php_executor": 10,
    "php_server": 11,
}


class BoundaryError(Exception):
    pass


def load_metadata() -> dict:
    result = subprocess.run(
        ["cargo", "metadata", "--format-version=1", "--no-deps"],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode != 0:
        raise BoundaryError(result.stderr.strip() or "cargo metadata failed")
    return json.loads(result.stdout)


def load_allowlist() -> dict[tuple[str, str], dict]:
    try:
        data = json.loads(ALLOWLIST.read_text(encoding="utf-8"))
    except OSError as error:
        raise BoundaryError(f"could not read allowlist: {error}") from error
    entries: dict[tuple[str, str], dict] = {}
    for index, entry in enumerate(data.get("allowed_edges", []), start=1):
        missing = {"from", "to", "category", "reason"} - set(entry)
        if missing:
            raise BoundaryError(
                f"allowlist entry {index} missing fields: {', '.join(sorted(missing))}"
            )
        if not entry["reason"].strip():
            raise BoundaryError(f"allowlist entry {index} needs a non-empty reason")
        entries[(entry["from"], entry["to"])] = entry
    return entries


def workspace_edges(metadata: dict) -> list[tuple[str, str]]:
    packages = {
        package["name"]
        for package in metadata["packages"]
        if package["name"].startswith("php_")
    }
    edges: set[tuple[str, str]] = set()
    for package in metadata["packages"]:
        source = package["name"]
        if source not in packages:
            continue
        for dependency in package.get("dependencies", []):
            target = dependency["name"]
            if target in packages:
                edges.add((source, target))
    return sorted(edges)


def edge_kind(source: str, target: str) -> str:
    if source not in LAYER_INDEX or target not in LAYER_INDEX:
        return "workspace"
    if LAYER_INDEX[target] <= LAYER_INDEX[source] + 1:
        return "expected"
    return "boundary-exception"


def write_reports(rows: list[dict]) -> None:
    REPORT_DIR.mkdir(parents=True, exist_ok=True)
    REPORT_JSON.write_text(json.dumps({"edges": rows}, indent=2) + "\n", encoding="utf-8")
    lines = ["# Dependency Boundary Report", "", "| Edge | Kind | Category | Reason |", "| --- | --- | --- | --- |"]
    for row in rows:
        lines.append(
            f"| `{row['from']} -> {row['to']}` | {row['kind']} | {row.get('category', '')} | {row.get('reason', '')} |"
        )
    REPORT_MD.write_text("\n".join(lines) + "\n", encoding="utf-8")


def main() -> int:
    try:
        allowlist = load_allowlist()
        metadata = load_metadata()
        rows = []
        violations = []
        for source, target in workspace_edges(metadata):
            allowed = allowlist.get((source, target))
            kind = edge_kind(source, target)
            row = {"from": source, "to": target, "kind": kind}
            if allowed is not None:
                row.update({"category": allowed["category"], "reason": allowed["reason"]})
            elif kind == "boundary-exception":
                violations.append(f"{source} -> {target}")
            rows.append(row)
        write_reports(rows)
    except BoundaryError as error:
        print(f"[fail] dependency boundaries: {error}", file=sys.stderr)
        return 1
    if violations:
        print("[fail] dependency boundaries: undocumented cross-layer edges:", file=sys.stderr)
        for violation in violations:
            print(f"  - {violation}", file=sys.stderr)
        print(f"Report: {REPORT_MD.relative_to(ROOT)}", file=sys.stderr)
        return 1
    print(f"[ok] dependency boundaries report written to {REPORT_MD.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
