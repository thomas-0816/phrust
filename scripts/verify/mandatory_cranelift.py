#!/usr/bin/env python3
"""Verify that every product binary has one mandatory Cranelift compiler graph."""

from __future__ import annotations

import json
import re
import subprocess
import sys
from collections import deque
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
PRODUCTS = ("php_server", "php_vm_cli")
PRODUCT_GRAPH = {"php_server", "php_vm_cli", "php_executor", "php_vm", "php_jit"}
CRANELIFT_CRATES = {
    "cranelift-codegen",
    "cranelift-frontend",
    "cranelift-jit",
    "cranelift-module",
    "cranelift-native",
}
FORBIDDEN_SOURCE = re.compile(
    r"\bJitMode\b|\bNoopJitBackend\b|\bCurrentJitBackend\b|"
    r"\bCraneliftExperiment\b|\bNativeExecutionDisabled\b|"
    r"\bBackendUnavailable\b|\ballow_native_execution\b|"
    r"experimental-jit|--jit(?:=|\s)",
)


def metadata() -> dict[str, object]:
    result = subprocess.run(
        ["cargo", "metadata", "--locked", "--format-version", "1", "--no-deps"],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=True,
    )
    return json.loads(result.stdout)


def product_reaches_jit(packages: dict[str, dict[str, object]], product: str) -> bool:
    queue = deque([product])
    seen: set[str] = set()
    while queue:
        current = queue.popleft()
        if current in seen:
            continue
        seen.add(current)
        if current == "php_jit":
            return True
        package = packages.get(current)
        if package is None:
            continue
        for dependency in package["dependencies"]:
            name = dependency.get("rename") or dependency["name"]
            if name in packages:
                queue.append(name)
    return False


def main() -> int:
    document = metadata()
    packages = {package["name"]: package for package in document["packages"]}
    failures: list[str] = []

    jit = packages.get("php_jit")
    if jit is None:
        failures.append("workspace metadata does not contain php_jit")
    else:
        dependencies = {dependency["name"]: dependency for dependency in jit["dependencies"]}
        missing = sorted(CRANELIFT_CRATES - dependencies.keys())
        optional = sorted(
            name for name in CRANELIFT_CRATES if dependencies.get(name, {}).get("optional")
        )
        if missing:
            failures.append("php_jit is missing Cranelift crates: " + ", ".join(missing))
        if optional:
            failures.append("Cranelift dependencies remain optional: " + ", ".join(optional))

    for product in PRODUCTS:
        package = packages.get(product)
        if package is None:
            failures.append(f"workspace metadata does not contain {product}")
            continue
        if not any(target["kind"] == ["bin"] for target in package["targets"]):
            failures.append(f"{product} has no product binary target")
        if not product_reaches_jit(packages, product):
            failures.append(f"{product} does not transitively depend on php_jit")

    for name in PRODUCT_GRAPH:
        package = packages.get(name)
        if package is None:
            continue
        if "jit-cranelift" in package["features"]:
            failures.append(f"{name} still exposes the jit-cranelift feature")
        manifest = Path(package["manifest_path"])
        source_root = manifest.parent / "src"
        for path in sorted(source_root.rglob("*.rs")):
            for line_number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
                if FORBIDDEN_SOURCE.search(line):
                    failures.append(
                        f"{path.relative_to(ROOT)}:{line_number}: retired product selector: {line.strip()}"
                    )

    if failures:
        print("mandatory Cranelift graph gate failed:", file=sys.stderr)
        print("\n".join(f"- {failure}" for failure in failures), file=sys.stderr)
        return 1
    print("mandatory Cranelift graph gate passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
