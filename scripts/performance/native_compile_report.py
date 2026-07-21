#!/usr/bin/env python3
"""Measure one production-profile native compile without perf_event_open."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time


def rss_kib(pid: int) -> tuple[int, int] | None:
    status = Path(f"/proc/{pid}/status")
    try:
        fields = {}
        for line in status.read_text(encoding="utf-8").splitlines():
            if ":" in line:
                key, value = line.split(":", 1)
                fields[key] = value.strip()
        current = int(fields.get("VmRSS", "0 kB").split()[0])
        peak = int(fields.get("VmHWM", fields.get("VmRSS", "0 kB")).split()[0])
        return current, peak
    except (FileNotFoundError, ProcessLookupError, ValueError):
        return None


def diagnostic_fields(lines: list[str]) -> dict[str, int | str | bool]:
    fields: dict[str, int | str | bool] = {}
    for line in lines:
        for item in line.split():
            if "=" not in item:
                continue
            key, value = item.split("=", 1)
            value = value.rstrip(",")
            if value in ("true", "false"):
                fields[key] = value == "true"
            else:
                try:
                    fields[key] = int(value)
                except ValueError:
                    fields[key] = value
    return fields


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--php-vm", required=True)
    parser.add_argument("--file", required=True)
    parser.add_argument("--function", required=True)
    parser.add_argument("--out", required=True)
    parser.add_argument("--label", choices=("before", "after", "large-function"), required=True)
    parser.add_argument("--max-ms", type=float)
    parser.add_argument("--max-rss-mib", type=float)
    parser.add_argument("--max-rss-delta-mib", type=float)
    parser.add_argument("--max-code-bytes", type=int)
    args = parser.parse_args()

    binary = Path(args.php_vm).resolve()
    source = Path(args.file).resolve()
    if not binary.is_file():
        parser.error(f"profiling binary does not exist: {binary}")
    if not source.is_file():
        parser.error(f"native compile input does not exist: {source}")

    output = Path(args.out)
    output.mkdir(parents=True, exist_ok=True)
    command = [
        str(binary),
        "native-compile",
        str(source),
        "--function",
        args.function,
        "--json",
    ]
    with tempfile.TemporaryDirectory(prefix="native-compile-report-", dir=output) as temp:
        stdout_path = Path(temp) / "stdout"
        stderr_path = Path(temp) / "stderr"
        started = time.monotonic_ns()
        with stdout_path.open("wb") as stdout, stderr_path.open("wb") as stderr:
            process = subprocess.Popen(command, stdout=stdout, stderr=stderr)
            first_rss = None
            maximum_rss = 0
            while process.poll() is None:
                sample = rss_kib(process.pid)
                if sample is not None:
                    current, peak = sample
                    first_rss = current if first_rss is None else first_rss
                    maximum_rss = max(maximum_rss, current, peak)
                time.sleep(0.005)
            returncode = process.wait()
        elapsed_ms = (time.monotonic_ns() - started) / 1_000_000.0
        stdout_text = stdout_path.read_text(encoding="utf-8", errors="replace")
        stderr_text = stderr_path.read_text(encoding="utf-8", errors="replace")

    payload = None
    for line in reversed(stdout_text.splitlines()):
        try:
            payload = json.loads(line)
            break
        except json.JSONDecodeError:
            continue
    if not isinstance(payload, dict):
        payload = {"ok": False, "diagnostics": [], "parse_error": "missing JSON result"}
    diagnostics = [str(value) for value in payload.get("diagnostics", [])]
    fields = diagnostic_fields(diagnostics)
    compile_time_nanos = payload.get("compile_time_nanos")
    compile_time_ms = (
        compile_time_nanos / 1_000_000.0 if isinstance(compile_time_nanos, int) else None
    )
    report = {
        "schema": 1,
        "label": args.label,
        "command": command,
        "host": {"machine": os.uname().machine, "sysname": os.uname().sysname},
        "result": payload,
        "metrics": {
            "wall_ms": round(elapsed_ms, 3),
            "compile_time_nanos": compile_time_nanos,
            "compile_time_ms": compile_time_ms,
            "rss_first_kib": first_rss,
            "rss_peak_kib": maximum_rss or None,
            "rss_delta_kib": None if first_rss is None else max(0, maximum_rss - first_rss),
            **fields,
        },
        "stderr": stderr_text,
        "returncode": returncode,
    }
    json_path = output / f"{args.label}.json"
    serialized_report = json.dumps(report, indent=2, sort_keys=True) + "\n"
    json_path.write_text(serialized_report, encoding="utf-8")
    markdown = [
        f"# Native compile {args.label}",
        "",
        f"- Function: `{args.function}`",
        f"- Compile time: {compile_time_ms:.3f} ms"
        if compile_time_ms is not None
        else "- Compile time: unavailable",
        f"- Process wall time: {elapsed_ms:.3f} ms",
        f"- Peak RSS: {maximum_rss / 1024.0:.2f} MiB" if maximum_rss else "- Peak RSS: unavailable",
        f"- Native code: {fields.get('code_bytes', 'unavailable')} bytes",
        f"- Fragments: {fields.get('plan_fragments', 'unavailable')}",
        f"- Largest CLIF job: {fields.get('max_fragment_clif_blocks', 'unavailable')} blocks, "
        f"{fields.get('max_fragment_clif_values', 'unavailable')} values",
        f"- Streaming frame: {fields.get('fragment_frame_slots', 'unavailable')} slots "
        f"({fields.get('fragment_shared_register_slots', 'unavailable')} shared, "
        f"{fields.get('fragment_scratch_register_slots', 'unavailable')} scratch)",
        f"- Maximum temporary cache: "
        f"{fields.get('max_temporary_cache_entries', 'unavailable')} entries",
        f"- Fragment CLIF memory traffic: "
        f"{fields.get('max_fragment_loads_per_source_instruction_milli', 'unavailable')} "
        f"loads/1000 IR, "
        f"{fields.get('max_fragment_stores_per_source_instruction_milli', 'unavailable')} "
        f"stores/1000 IR",
        f"- Result: {'pass' if returncode == 0 and payload.get('ok') else 'fail'}",
        "",
    ]
    serialized_markdown = "\n".join(markdown)
    (output / f"{args.label}.md").write_text(serialized_markdown, encoding="utf-8")
    if args.label == "after":
        # A8 names the exact same audited large-function sample explicitly.
        # Alias the captured report instead of running a second, noisier
        # compile with different RSS and timing samples.
        (output / "large-function.json").write_text(serialized_report, encoding="utf-8")
        (output / "large-function.md").write_text(serialized_markdown, encoding="utf-8")
    merge_contract = {
        "base_commit": "be91339047d931d4c364d4ce6a16ddbd9786be96",
        "runtime_abi_before": 20,
        "runtime_abi_after": 20,
        "native_fragment_plan_schema": 5,
        "native_cache_writer": "PNA2/PRM5",
        "native_cache_read_compatibility": ["PNA2/PRM4", "PNA1/PRM3"],
        "fragment_abi_changes": [
            "streaming baseline registers use compact function-fragment frame slots",
            "fragment plans use deterministic cost-minimizing CFG-layout partitions",
        ],
        "helper_ids_added": [],
        "shared_files_changed": [
            "crates/php_jit/src/cranelift_lowering.rs",
            "crates/php_jit/src/cranelift_lowering/executable_region.rs",
            "crates/php_vm/src/vm/mod.rs",
        ],
        "hot_native_rebase_actions": [
            "preserve NativeFragmentPlan schema version in persistent identity",
            "keep optimizing transitions on NativeFunctionFragmentLayout frame slots",
        ],
    }
    (output / "merge-contract.json").write_text(
        json.dumps(merge_contract, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )

    failures = []
    if returncode != 0 or not payload.get("ok"):
        failures.append("native compile did not complete successfully")
    measured_compile_ms = compile_time_ms if compile_time_ms is not None else elapsed_ms
    if args.max_ms is not None and measured_compile_ms >= args.max_ms:
        failures.append(
            f"compile time {measured_compile_ms:.3f} ms exceeds {args.max_ms:.3f} ms"
        )
    if args.max_rss_mib is not None and maximum_rss / 1024.0 >= args.max_rss_mib:
        failures.append(
            f"peak RSS {maximum_rss / 1024.0:.2f} MiB exceeds {args.max_rss_mib:.2f} MiB"
        )
    rss_delta_kib = None if first_rss is None else max(0, maximum_rss - first_rss)
    if args.max_rss_delta_mib is not None and (
        rss_delta_kib is None or rss_delta_kib / 1024.0 >= args.max_rss_delta_mib
    ):
        failures.append(
            f"RSS delta {rss_delta_kib!r} KiB exceeds {args.max_rss_delta_mib:.2f} MiB"
        )
    code_bytes = fields.get("code_bytes")
    if args.max_code_bytes is not None and (
        not isinstance(code_bytes, int) or code_bytes >= args.max_code_bytes
    ):
        failures.append(f"native code {code_bytes!r} exceeds {args.max_code_bytes} bytes")
    if failures:
        for failure in failures:
            print(f"native compile gate: {failure}", file=sys.stderr)
        return 1
    print(json_path)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
