#!/usr/bin/env python3
"""Final no-exceptions ratchet for the mandatory Cranelift architecture."""

from __future__ import annotations

import argparse
import json
import os
import re
import socket
import subprocess
import sys
import tempfile
import time
import urllib.request
from collections import deque
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
SELF = Path(__file__).resolve()
CRANELIFT_CRATES = {
    "cranelift-codegen",
    "cranelift-frontend",
    "cranelift-jit",
    "cranelift-module",
    "cranelift-native",
}
PRODUCTS = ("php_server", "php_vm_cli")
PRODUCT_GRAPH = {"php_server", "php_vm_cli", "php_executor", "php_vm", "php_jit"}
FORBIDDEN_PATHS = (
    "crates/php_vm/src/bytecode",
    "crates/php_vm/src/deopt.rs",
    "crates/php_vm/src/fallback.rs",
    "crates/php_vm/src/osr.rs",
    "crates/php_vm/src/quickening.rs",
    "crates/php_vm/src/vm/dense_" + "dispatch.rs",
    "crates/php_vm/src/vm/rich_" + "dispatch.rs",
    "crates/php_jit/src/region_ir/interpreter.rs",
    "crates/php_jit/src/copy_" + "patch",
)
FORBIDDEN_SOURCE = (
    re.compile(r"\bJitMode\b|\bNoopJitBackend\b|\bCurrentJitBackend\b"),
    re.compile(r"\bExecutionFormat\b|\bQuickeningMode\b|\bSuperinstructionMode\b"),
    re.compile(
        r"\bDense(?:Opcode|BytecodeUnit|ExecutionPlan|IncludeMode|JumpThreadingMode)\b|"
        r"\bBytecodeLayoutMode\b"
    ),
    re.compile(
        r"execute_bytecode_function|execute_dense_activation|"
        r"execute_function_with_dense_plan|execute_ir_function|execute_instruction|"
        r"dense_dispatch|rich_dispatch|deopt_to_dense|resume_to_interpreter"
    ),
    re.compile(r"\bNativeLeaf\b|leaf[_ -]recognizer|stencil backend", re.IGNORECASE),
    re.compile(
        r"\bJitNative" + r"Specialization\b|\bJitProperty(?:Load|Store)Metadata\b"
    ),
    re.compile(
        r"\bValue(?:I64|Metadata|Value)?StatusOut\b|"
        r"\bvalue_(?:i64_|metadata_|value_)?status_out_native\b"
    ),
    re.compile(
        r"php_jit_array_(?:is_packed_ints|len|fetch_int_slow)|"
        r"php_jit_property_load_monomorphic_fast"
    ),
    re.compile(
        r"\bJitRuntimeHelperTable\b|\bJitHelperDispatch\b|"
        r"jit_default_helper_" + r"dispatch"
    ),
    re.compile(r"pub\s+resume_(?:block|instruction)\b"),
    re.compile(r"interpreter\s+(?:side[ -]exit|fallback|resume)", re.IGNORECASE),
    re.compile(r"InstructionKind::" + "Unsupported"),
    re.compile(r"jit" + r"-(?:copy-patch|cranelift)"),
    re.compile(r"experimental" + r"-jit"),
    re.compile(r"--" + r"(?:jit|exec-format|quickening|superinstructions|dense-[a-z0-9-]+)\b"),
    re.compile(r"copy" + r"(?:[_ -]and)?[_ -]?patch", re.IGNORECASE),
)
FORBIDDEN_BINARY = (
    "execute_" + "bytecode_function",
    "execute_" + "dense_activation",
    "execute_function_with_" + "dense_plan",
    "execute_" + "ir_function",
    "rich_" + "dispatch",
    "copy_" + "and_patch",
    "copy-" + "patch",
    "JIT " + "off",
    "--" + "jit",
    "backend " + "selection",
    "interpreter " + "resume",
)


def command(
    args: list[str], *, check: bool = True, capture: bool = True
) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        args,
        cwd=ROOT,
        text=True,
        capture_output=capture,
        check=check,
        env={**os.environ, "LC_ALL": "C", "TZ": "UTC"},
    )


def repository_files() -> list[Path]:
    tracked = command(["git", "ls-files"]).stdout.splitlines()
    untracked = command(
        ["git", "ls-files", "--others", "--exclude-standard"]
    ).stdout.splitlines()
    return [ROOT / relative for relative in sorted(set(tracked + untracked))]


def source_contract_file(path: Path) -> bool:
    relative = path.relative_to(ROOT).as_posix()
    if path.resolve() == SELF or not path.is_file():
        return False
    if relative.startswith(("target/", ".git/", "third_party/php-src/")):
        return False
    if path.suffix == ".rs" or path.name in {"Cargo.toml", "Cargo.lock", "justfile"}:
        return True
    if relative.startswith("scripts/"):
        return True
    if relative in {"README.md", "AGENTS.md"} or (
        relative.startswith("docs/") and path.suffix == ".md"
    ):
        return True
    if path.suffix in {".json", ".jsonl"} and (
        "schema" in relative or relative.startswith("docs/")
    ):
        return True
    return relative in {
        "tests/snapshots/php-vm-help.txt",
        "tests/snapshots/phrust-server-help.txt",
    }


def verify_source(failures: list[str]) -> int:
    scanned = 0
    for relative in FORBIDDEN_PATHS:
        if (ROOT / relative).exists():
            failures.append(f"retired path exists: {relative}")
    for path in repository_files():
        if not source_contract_file(path):
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            continue
        scanned += 1
        for pattern in FORBIDDEN_SOURCE:
            match = pattern.search(text)
            if match:
                line = text.count("\n", 0, match.start()) + 1
                failures.append(
                    f"{path.relative_to(ROOT)}:{line}: forbidden source contract {match.group(0)!r}"
                )
    return scanned


def metadata() -> dict[str, Any]:
    return json.loads(
        command(["cargo", "metadata", "--locked", "--format-version", "1"]).stdout
    )


def workspace_closure(packages: dict[str, dict[str, Any]], start: str) -> set[str]:
    closure: set[str] = set()
    queue = deque([start])
    while queue:
        name = queue.popleft()
        if name in closure:
            continue
        closure.add(name)
        package = packages.get(name)
        if package is None:
            continue
        for dependency in package["dependencies"]:
            dependency_name = dependency.get("rename") or dependency["name"]
            if dependency_name in packages:
                queue.append(dependency_name)
    return closure


def verify_cargo_graph(failures: list[str]) -> None:
    document = metadata()
    packages = {package["name"]: package for package in document["packages"]}
    compiler = packages.get("php_jit")
    if compiler is None:
        failures.append("workspace has no php_jit native compiler package")
        return
    dependencies = {dependency["name"]: dependency for dependency in compiler["dependencies"]}
    for dependency in sorted(CRANELIFT_CRATES):
        record = dependencies.get(dependency)
        if record is None:
            failures.append(f"php_jit lacks mandatory dependency {dependency}")
        elif record.get("optional"):
            failures.append(f"Cranelift dependency is optional: {dependency}")

    retired_features = {"jit-" + "copy-patch", "jit-" + "cranelift", "interpreter"}
    for name in PRODUCT_GRAPH:
        package = packages.get(name)
        if package is None:
            failures.append(f"product graph lacks package {name}")
            continue
        forbidden = sorted(
            feature
            for feature in package["features"]
            if feature in retired_features or "interpreter" in feature
        )
        if forbidden:
            failures.append(f"{name} exposes retired features: {', '.join(forbidden)}")

    for product in PRODUCTS:
        closure = workspace_closure(packages, product)
        if "php_jit" not in closure:
            failures.append(f"{product} can build without the native compiler")
        oracle = sorted(name for name in closure if "oracle" in name.lower())
        if oracle:
            failures.append(f"{product} depends on oracle packages: {', '.join(oracle)}")


def verify_lowering(failures: list[str]) -> None:
    for script in (
        "scripts/verify/cranelift_exhaustive_lowering.py",
        "scripts/verify/cranelift_typed_runtime_ops.py",
        "scripts/verify/cranelift_typed_semantic_ops.py",
    ):
        result = command([str(ROOT / script)], check=False)
        if result.returncode != 0:
            failures.append(f"{script} failed: {(result.stderr or result.stdout).strip()}")
    coverage_path = ROOT / "target/cranelift-only/instruction-coverage.json"
    if not coverage_path.is_file():
        failures.append("generated lowering manifest is missing")
        return
    coverage = json.loads(coverage_path.read_text(encoding="utf-8"))
    instructions = [entry for entry in coverage.get("entries", []) if entry.get("kind") == "instruction"]
    terminators = [entry for entry in coverage.get("entries", []) if entry.get("kind") == "terminator"]
    if len(instructions) != 101 or len(terminators) != 6:
        failures.append(
            f"lowering manifest has {len(instructions)} instructions/{len(terminators)} terminators"
        )


def binary_text(binary: Path) -> tuple[str, str]:
    symbols = command(["nm", "-C", str(binary)], check=False)
    strings = command(["strings", "-a", str(binary)], check=False)
    if symbols.returncode != 0:
        raise RuntimeError(f"nm failed for {binary}: {symbols.stderr.strip()}")
    if strings.returncode != 0:
        raise RuntimeError(f"strings failed for {binary}: {strings.stderr.strip()}")
    return symbols.stdout, strings.stdout


def verify_binary(binary: Path, failures: list[str]) -> None:
    if not binary.is_file():
        failures.append(f"release binary is missing: {binary}")
        return
    try:
        symbols, strings = binary_text(binary)
    except RuntimeError as error:
        failures.append(str(error))
        return
    if not any(marker in symbols for marker in ("CraneliftNativeCompiler", "JITModule")):
        failures.append(f"{binary.name} lacks a native compiler symbol")
    if not any(
        marker in symbols
        for marker in ("compile_unit_with_runtime_helpers", "native_entry", "invoke_i64")
    ):
        failures.append(f"{binary.name} lacks a native runtime entry symbol")
    combined = symbols + "\n" + strings
    for marker in FORBIDDEN_BINARY:
        if marker in combined:
            failures.append(f"{binary.name} contains retired binary marker {marker!r}")


def verify_help_snapshot(binary: Path, snapshot: Path, failures: list[str]) -> None:
    result = command([str(binary), "--help"], check=False)
    actual = result.stdout + result.stderr
    if result.returncode != 0:
        failures.append(f"{binary.name} --help exited with {result.returncode}")
    if not snapshot.is_file():
        failures.append(f"help snapshot is missing: {snapshot.relative_to(ROOT)}")
    elif actual != snapshot.read_text(encoding="utf-8"):
        failures.append(f"{binary.name} help differs from {snapshot.relative_to(ROOT)}")


def unused_port() -> int:
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        return int(probe.getsockname()[1])


def verify_server_start(server: Path, failures: list[str]) -> None:
    port = unused_port()
    with tempfile.TemporaryDirectory(prefix="phrust-native-server-") as temporary:
        process = subprocess.Popen(
            [
                str(server),
                "--docroot",
                temporary,
                "--listen",
                f"127.0.0.1:{port}",
                "--engine-preset",
                "default",
                "--native-cache",
                "off",
            ],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env={**os.environ, "LC_ALL": "C", "TZ": "UTC"},
        )
        healthy = False
        try:
            deadline = time.monotonic() + 10.0
            while time.monotonic() < deadline and process.poll() is None:
                try:
                    with urllib.request.urlopen(
                        f"http://127.0.0.1:{port}/healthz", timeout=0.5
                    ) as response:
                        healthy = response.status == 200 and response.read() == b"ok\n"
                        if healthy:
                            break
                except OSError:
                    time.sleep(0.05)
        finally:
            process.terminate()
            try:
                stdout, stderr = process.communicate(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                stdout, stderr = process.communicate()
        if not healthy:
            failures.append(f"release server did not pass /healthz: {stdout}{stderr}")
        identity = stdout + stderr
        for field in (
            "native_startup",
            "compiler_version=",
            "runtime_abi=",
            "helper_abi=",
            "target=",
            "cpu_features=",
            "cache_mode=off",
            "preset=default",
            "artifacts_loaded=",
            "artifacts_compiled=",
        ):
            if field not in identity:
                failures.append(f"server startup identity lacks {field}")


def run_proof(label: str, test_filter: str, failures: list[str]) -> None:
    result = command(
        ["cargo", "test", "--quiet", "-p", "php_jit", "--lib", test_filter],
        check=False,
    )
    output = result.stdout + result.stderr
    if result.returncode != 0 or re.search(r"\b[1-9][0-9]* passed", output) is None:
        failures.append(f"runtime proof {label} failed or ran no test: {output.strip()}")


def verify_runtime(server: Path, failures: list[str]) -> None:
    verify_server_start(server, failures)
    for label, test_filter in (
        ("entry and baseline/default", "baseline_and_default_policies_both_execute_native_code"),
        ("include", "include_executes_only_after_native_dynamic_compiler_returns_entry_result"),
        ("eval", "eval_executes_only_after_native_dynamic_compiler_returns_entry_result"),
        ("callback re-entry", "cranelift_dynamic_call_uses_typed_native_trampoline"),
        ("generator resume", "generator_yield_send_and_throw_use_native_resume_entry"),
        ("fiber resume", "fiber_suspend_and_resume_use_native_continuation"),
    ):
        run_proof(label, test_filter, failures)
    cache = command([str(ROOT / "scripts/verify/cranelift_native_cache.py")], check=False)
    if cache.returncode != 0 or '"second_process_hit": true' not in cache.stdout:
        failures.append(
            "runtime proof native-cache restart hit failed: "
            + (cache.stderr or cache.stdout).strip()
        )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--cli", type=Path, default=ROOT / "target/release/php-vm"
    )
    parser.add_argument(
        "--server", type=Path, default=ROOT / "target/release/phrust-server"
    )
    parser.add_argument("--source-only", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    cli = args.cli if args.cli.is_absolute() else ROOT / args.cli
    server = args.server if args.server.is_absolute() else ROOT / args.server
    failures: list[str] = []
    scanned = verify_source(failures)
    verify_cargo_graph(failures)
    verify_lowering(failures)
    if not args.source_only:
        verify_binary(cli, failures)
        verify_binary(server, failures)
        verify_help_snapshot(cli, ROOT / "tests/snapshots/php-vm-help.txt", failures)
        verify_help_snapshot(
            server, ROOT / "tests/snapshots/phrust-server-help.txt", failures
        )
        verify_runtime(server, failures)

    report = {
        "schema_version": 1,
        "architecture": "cranelift-only",
        "source_files_scanned": scanned,
        "exception_count": 0,
        "status": "pass" if not failures else "fail",
        "failures": failures,
        "runtime_proofs": [] if args.source_only else [
            "server_start",
            "entry",
            "include",
            "eval",
            "callback_reentry",
            "generator_resume",
            "fiber_resume",
            "baseline_default",
            "restart_cache_hit",
        ],
    }
    output = ROOT / "target/cranelift-only/final-ratchet.json"
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    if failures:
        print("Cranelift-only ratchet failed:", file=sys.stderr)
        print("\n".join(f"- {failure}" for failure in failures), file=sys.stderr)
        return 1
    print(
        "Cranelift-only ratchet passed "
        f"({scanned} source-contract files, exceptions=0, "
        f"runtime proofs={0 if args.source_only else 9})"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
