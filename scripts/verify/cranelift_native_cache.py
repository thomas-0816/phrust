#!/usr/bin/env python3
"""Verify restart-persistent PNA2 machine-code cache behavior."""

from __future__ import annotations

import json
import pathlib
import re
import subprocess
import tempfile
import tomllib


ROOT = pathlib.Path(__file__).resolve().parents[2]


def run_probe(cache_dir: pathlib.Path) -> dict[str, object]:
    completed = subprocess.run(
        [
            "cargo",
            "run",
            "--quiet",
            "-p",
            "php_jit",
            "--example",
            "native_cache_probe",
            "--",
            str(cache_dir),
        ],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    lines = [line for line in completed.stdout.splitlines() if line.strip()]
    if not lines:
        raise SystemExit("[fail] native cache probe produced no JSON")
    return json.loads(lines[-1])


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(f"[fail] {message}")


def main() -> None:
    with tempfile.TemporaryDirectory(prefix="phrust-pna-gate-") as temporary:
        cache_dir = pathlib.Path(temporary)
        first = run_probe(cache_dir)
        artifacts = list(cache_dir.glob("*.pna"))
        require(first.get("compiled") is True, "first process did not compile")
        require(first.get("value") == 42, "first native execution returned wrong value")
        require(first.get("event") == "written", "first process did not atomically write PNA2")
        require(len(artifacts) == 1, "first process did not create exactly one artifact")
        require(artifacts[0].read_bytes().startswith(b"PNA2"), "artifact is not PNA2")

        second = run_probe(cache_dir)
        require(second.get("compiled") is False, "fresh second process recompiled unchanged IR")
        require(second.get("event") == "hit", "fresh second process missed native cache")
        require(second.get("value") == 42, "reloaded native code returned wrong value")

        corrupted = bytearray(artifacts[0].read_bytes())
        corrupted[-1] ^= 0x5A
        artifacts[0].write_bytes(corrupted)
        rebuilt = run_probe(cache_dir)
        require(rebuilt.get("compiled") is True, "corrupt artifact was not rebuilt")
        require(rebuilt.get("event") == "rebuilt", "corrupt artifact lacked rebuild event")
        require(rebuilt.get("value") == 42, "rebuilt native code returned wrong value")
        require(
            int(rebuilt.get("invalid_artifacts", 0)) >= 1,
            "corrupt artifact rejection was not counted",
        )

    commands = (ROOT / "crates/php_vm_cli/src/commands.rs").read_text()
    cache_source = (ROOT / "crates/php_jit/src/native_cache.rs").read_text()
    for control in (
        "--native-cache",
        "--native-cache-dir",
        "--clear-native-cache",
        "--native-cache-stats",
    ):
        require(control in commands, f"missing product control {control}")
    for variable in ("PHRUST_NATIVE_CACHE", "PHRUST_NATIVE_CACHE_DIR"):
        require(variable in cache_source, f"missing environment control {variable}")

    lock = tomllib.loads((ROOT / "Cargo.lock").read_text())
    locked_cranelift = next(
        package["version"]
        for package in lock["package"]
        if package["name"] == "cranelift-codegen"
    )
    jit_source = (ROOT / "crates/php_jit/src/lib.rs").read_text()
    embedded = re.search(r'CRANELIFT_VERSION: &str = "([^"]+)"', jit_source)
    require(embedded is not None, "missing embedded Cranelift cache identity")
    require(
        embedded.group(1) == locked_cranelift,
        "embedded Cranelift version does not match Cargo.lock",
    )

    print(
        json.dumps(
            {
                "schema_version": 1,
                "cache": "PNA2",
                "second_process_hit": True,
                "unchanged_recompiled": False,
                "corruption_rebuilt": True,
                "w_x": "write-then-rx",
            },
            sort_keys=True,
        )
    )


if __name__ == "__main__":
    main()
