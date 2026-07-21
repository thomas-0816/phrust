#!/usr/bin/env python3
"""Measure a deterministic distribution of independent native function compiles."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import statistics
import subprocess
import tempfile
import time

from native_compile_report import diagnostic_fields, rss_kib


def percentile(values: list[float], quantile: float) -> float:
    ordered = sorted(values)
    index = max(0, min(len(ordered) - 1, int(len(ordered) * quantile + 0.999999) - 1))
    return ordered[index]


def compile_sample(binary: Path, source: Path, function: str, temp: Path) -> dict[str, object]:
    command = [
        str(binary),
        "native-compile",
        str(source),
        "--function",
        function,
        "--json",
    ]
    stdout_path = temp / "stdout"
    stderr_path = temp / "stderr"
    started = time.monotonic_ns()
    first_rss = None
    peak_rss = 0
    with stdout_path.open("wb") as stdout, stderr_path.open("wb") as stderr:
        process = subprocess.Popen(command, stdout=stdout, stderr=stderr)
        while process.poll() is None:
            sample = rss_kib(process.pid)
            if sample is not None:
                current, peak = sample
                first_rss = current if first_rss is None else first_rss
                peak_rss = max(peak_rss, current, peak)
            time.sleep(0.001)
        returncode = process.wait()
    wall_ms = (time.monotonic_ns() - started) / 1_000_000.0
    stdout_text = stdout_path.read_text(encoding="utf-8", errors="replace")
    payload: dict[str, object] | None = None
    for line in reversed(stdout_text.splitlines()):
        try:
            candidate = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(candidate, dict):
            payload = candidate
            break
    if payload is None:
        payload = {"ok": False, "diagnostics": [], "parse_error": "missing JSON result"}
    diagnostics = [str(value) for value in payload.get("diagnostics", [])]
    fields = diagnostic_fields(diagnostics)
    compile_time_nanos = payload.get("compile_time_nanos")
    return {
        **fields,
        "function": function,
        "command": command,
        "ok": returncode == 0 and payload.get("ok") is True,
        "returncode": returncode,
        "wall_ms": round(wall_ms, 3),
        "compile_time_nanos": compile_time_nanos,
        "compile_time_ms": (
            compile_time_nanos / 1_000_000.0
            if isinstance(compile_time_nanos, int)
            else None
        ),
        "rss_delta_kib": None if first_rss is None else max(0, peak_rss - first_rss),
        "stderr": stderr_path.read_text(encoding="utf-8", errors="replace"),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--php-vm", required=True)
    parser.add_argument("--file", required=True)
    parser.add_argument("--function", action="append", required=True)
    parser.add_argument("--out", required=True)
    args = parser.parse_args()

    binary = Path(args.php_vm).resolve()
    source = Path(args.file).resolve()
    if not binary.is_file():
        parser.error(f"profiling binary does not exist: {binary}")
    if not source.is_file():
        parser.error(f"native compile input does not exist: {source}")
    if len(set(args.function)) != len(args.function):
        parser.error("--function entries must be unique")

    output = Path(args.out)
    output.mkdir(parents=True, exist_ok=True)
    samples = []
    with tempfile.TemporaryDirectory(prefix="native-compile-distribution-", dir=output) as root:
        for index, function in enumerate(args.function):
            temp = Path(root) / f"sample-{index:03d}"
            temp.mkdir()
            samples.append(compile_sample(binary, source, function, temp))

    walls = [float(sample["wall_ms"]) for sample in samples]
    compiles = [
        float(sample["compile_time_ms"])
        for sample in samples
        if sample["compile_time_ms"] is not None
    ]
    code_bytes = [int(sample.get("code_bytes", 0)) for sample in samples]
    report = {
        "schema": 1,
        "host": {"machine": os.uname().machine, "sysname": os.uname().sysname},
        "source": str(source),
        "summary": {
            "count": len(samples),
            "successful": sum(sample["ok"] is True for sample in samples),
            "wall_ms_min": min(walls),
            "wall_ms_median": statistics.median(walls),
            "wall_ms_p95": percentile(walls, 0.95),
            "wall_ms_max": max(walls),
            "compile_ms_median": statistics.median(compiles) if compiles else None,
            "compile_ms_p95": percentile(compiles, 0.95) if compiles else None,
            "code_bytes_total": sum(code_bytes),
        },
        "samples": samples,
    }
    json_path = output / "compile-distribution.json"
    json_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    summary = report["summary"]
    markdown = [
        "# Native compile distribution",
        "",
        f"- Samples: {summary['successful']}/{summary['count']} successful",
        f"- Median wall time: {summary['wall_ms_median']:.3f} ms",
        f"- p95 wall time: {summary['wall_ms_p95']:.3f} ms",
        f"- Maximum wall time: {summary['wall_ms_max']:.3f} ms",
        f"- Median native compile time: {summary['compile_ms_median']:.3f} ms"
        if summary["compile_ms_median"] is not None
        else "- Median native compile time: unavailable",
        f"- p95 native compile time: {summary['compile_ms_p95']:.3f} ms"
        if summary["compile_ms_p95"] is not None
        else "- p95 native compile time: unavailable",
        f"- Total native code: {summary['code_bytes_total']} bytes",
        "",
        "| Function | Wall ms | Code bytes | Fragments | Max CLIF blocks |",
        "| --- | ---: | ---: | ---: | ---: |",
    ]
    markdown.extend(
        f"| `{sample['function']}` | {sample['wall_ms']:.3f} | "
        f"{sample.get('code_bytes', 'n/a')} | {sample.get('plan_fragments', 'n/a')} | "
        f"{sample.get('max_fragment_clif_blocks', 'n/a')} |"
        for sample in samples
    )
    (output / "compile-distribution.md").write_text(
        "\n".join(markdown) + "\n", encoding="utf-8"
    )
    print(json_path)
    return 0 if all(sample["ok"] is True for sample in samples) else 1


if __name__ == "__main__":
    raise SystemExit(main())
