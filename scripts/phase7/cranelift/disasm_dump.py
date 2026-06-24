#!/usr/bin/env python3
"""Generate optional Phase 7 Cranelift disassembly/code-size diagnostics."""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

import jit_bench_matrix


ROOT = Path(__file__).resolve().parents[3]
DEFAULT_ENGINE = ROOT / "target/debug/php-vm"
DEFAULT_OUT_DIR = ROOT / "target/phase7/cranelift/disasm"
DEFAULT_TIMEOUT = 10.0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--engine", type=Path, default=Path(os.getenv("PHRUST_PHP_VM", DEFAULT_ENGINE)))
    parser.add_argument("--out-dir", type=Path, default=DEFAULT_OUT_DIR)
    parser.add_argument("--timeout", type=float, default=DEFAULT_TIMEOUT)
    parser.add_argument(
        "--max-dumps",
        type=int,
        default=int(os.getenv("PHRUST_CRANELIFT_DISASM_MAX_DUMPS", "12")),
        help="Maximum successful compile dumps to emit. Use 0 for all scenarios.",
    )
    return parser.parse_args()


def rel(path: Path) -> str:
    try:
        return path.resolve().relative_to(ROOT).as_posix()
    except ValueError:
        return path.as_posix()


def normalized_env(tmp_dir: Path) -> dict[str, str]:
    env = jit_bench_matrix.normalized_env(tmp_dir)
    env["PHRUST_RANDOM_SEED"] = "phase7-cranelift-disasm-dump"
    env["RUST_TEST_SEED"] = "phase7-cranelift-disasm-dump"
    return env


def extract_jit_stats(stderr: str) -> dict[str, Any] | None:
    return jit_bench_matrix.extract_jit_stats(stderr)


def command_for(engine: Path, scenario: dict[str, Any], clif_path: Path) -> list[str]:
    command = jit_bench_matrix.command_for(engine, scenario["fixture"], "cranelift", scenario)
    return command[:-1] + ["--jit-dump-clif", rel(clif_path)] + command[-1:]


def run_dump(
    *,
    engine: Path,
    scenario: dict[str, Any],
    clif_path: Path,
    tmp_dir: Path,
    timeout: float,
) -> tuple[list[str], subprocess.CompletedProcess[str], float, dict[str, Any] | None]:
    tmp_dir.mkdir(parents=True, exist_ok=True)
    command = command_for(engine, scenario, clif_path)
    started = time.perf_counter()
    completed = subprocess.run(
        command,
        cwd=ROOT,
        env=normalized_env(tmp_dir),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
        check=False,
    )
    elapsed = time.perf_counter() - started
    return command, completed, elapsed, extract_jit_stats(completed.stderr)


def disasm_text_for(
    *,
    scenario: dict[str, Any],
    descriptor: dict[str, Any],
    clif_path: Path,
    objdump: str | None,
) -> str:
    lines = [
        "# Phase 7 Cranelift Disassembly Diagnostic",
        "",
        f"scenario: {scenario['scenario']}",
        f"fixture: {rel(scenario['fixture'])}",
        f"function_id: {descriptor['function_id']}",
        f"function_name: {descriptor['function_name']}",
        f"ir_fingerprint: {descriptor['ir_fingerprint']}",
        f"code_bytes: {descriptor['code_bytes']}",
        f"compile_time_nanos: {descriptor['compile_time_nanos']}",
        f"target_isa: {descriptor['target_isa']}",
        f"abi_hash: {descriptor['abi_hash']}",
        f"config_hash: {descriptor['config_hash']}",
        f"clif_dump: {rel(clif_path)}",
        f"objdump: {objdump or 'not-found'}",
        "",
        "native_disassembly_status: skipped",
        "native_disassembly_reason: Cranelift JITModule keeps code in process memory; this repo does not yet expose an object file or safe JIT-memory extraction path for objdump.",
        "",
        "Use the linked CLIF dump and code_bytes value for Phase 7 performance diagnostics.",
    ]
    return "\n".join(lines) + "\n"


def write_json(path: Path, value: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def main() -> int:
    args = parse_args()
    out_dir = args.out_dir
    out_dir.mkdir(parents=True, exist_ok=True)
    tmp_dir = ROOT / "target/phase7/cranelift/tmp-disasm"
    objdump = shutil.which("llvm-objdump") or shutil.which("objdump")
    entries: list[dict[str, Any]] = []
    failures: list[str] = []
    skipped: list[dict[str, str]] = []

    scenarios = list(jit_bench_matrix.SCENARIOS)
    for scenario in scenarios:
        if args.max_dumps > 0 and len(entries) >= args.max_dumps:
            break
        clif_path = out_dir / f"{scenario['scenario']}.clif"
        try:
            command, completed, elapsed, stats = run_dump(
                engine=args.engine,
                scenario=scenario,
                clif_path=clif_path,
                tmp_dir=tmp_dir / scenario["scenario"],
                timeout=args.timeout,
            )
        except subprocess.TimeoutExpired:
            skipped.append({"scenario": scenario["scenario"], "reason": f"timed out after {args.timeout}s"})
            continue

        if completed.returncode != 0:
            skipped.append({"scenario": scenario["scenario"], "reason": f"php-vm exited {completed.returncode}"})
            continue
        if stats is None:
            skipped.append({"scenario": scenario["scenario"], "reason": "missing jit stats"})
            continue
        descriptors = stats.get("compile_descriptors")
        if not isinstance(descriptors, list) or not descriptors:
            skipped.append({"scenario": scenario["scenario"], "reason": "no successful compile descriptor"})
            continue
        descriptor = descriptors[0]
        if not clif_path.exists():
            skipped.append({"scenario": scenario["scenario"], "reason": f"no CLIF dump produced at {rel(clif_path)}"})
            continue

        disasm_path = out_dir / f"{scenario['scenario']}.disasm.txt"
        descriptor_path = out_dir / f"{scenario['scenario']}.json"
        entry = {
            "scenario": scenario["scenario"],
            "target": scenario["target"],
            "fixture": rel(scenario["fixture"]),
            "command": [rel(Path(command[0])), *command[1:]],
            "wall_time_seconds": elapsed,
            "function_id": descriptor["function_id"],
            "function_name": descriptor["function_name"],
            "ir_fingerprint": descriptor["ir_fingerprint"],
            "code_bytes": descriptor["code_bytes"],
            "compile_time_nanos": descriptor["compile_time_nanos"],
            "target_isa": descriptor["target_isa"],
            "abi_hash": descriptor["abi_hash"],
            "config_hash": descriptor["config_hash"],
            "clif_dump": rel(clif_path),
            "disassembly_dump": rel(disasm_path),
            "descriptor_json": rel(descriptor_path),
            "native_disassembly_status": "skipped",
            "native_disassembly_reason": "no object file or safe JIT-memory extraction path is available in Phase 7",
            "objdump": objdump,
        }
        disasm_path.write_text(
            disasm_text_for(
                scenario=scenario,
                descriptor=descriptor,
                clif_path=clif_path,
                objdump=objdump,
            ),
            encoding="utf-8",
        )
        write_json(descriptor_path, entry)
        entries.append(entry)

    status = "pass" if entries else "fail"
    manifest = {
        "schema_version": 1,
        "status": status,
        "dump_kind": "phase7-cranelift-code-size-and-clif",
        "output_dir": rel(out_dir),
        "entries": entries,
        "entry_count": len(entries),
        "native_disassembly_status": "skipped",
        "native_disassembly_reason": "JITModule native code is not materialized as an object file by this diagnostic path.",
        "objdump": objdump,
        "max_dumps": args.max_dumps,
        "failures": failures,
        "skipped": skipped,
        "ci_policy": "optional-local-diagnostic; no architecture-specific CI gate",
    }
    write_json(out_dir / "manifest.json", manifest)
    if status != "pass":
        print(f"[fail] Cranelift disasm dump wrote {len(entries)} entries; failures: {failures}", file=sys.stderr)
        return 1
    print(f"[pass] Cranelift disasm dump wrote {len(entries)} entry(s) to {rel(out_dir)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
