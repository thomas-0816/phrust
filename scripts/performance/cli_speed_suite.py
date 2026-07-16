#!/usr/bin/env python3
"""Measure CLI startup, compile, cache, and execution rows for the ratchet."""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from ratchet_schema import (
    ROOT,
    counter_highlights,
    executable,
    make_report,
    phase_metric_map,
    rel,
    render_report_markdown,
    timing_metrics,
    validate_report,
    write_json,
)


DEFAULT_ENGINE = ROOT / "target/debug/php-vm"
DEFAULT_RELEASE_ENGINE = ROOT / "target/release/php-vm"
DEFAULT_OUT = ROOT / "target/performance/ratchet/cli/current.json"


@dataclass(frozen=True)
class Row:
    scenario_id: str
    kind: str
    command: tuple[str, ...]
    expected_stdout: str | None = None
    optional: bool = False
    timings_index: int | None = None
    counters_index: int | None = None
    cache_dir: Path | None = None


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--engine", type=Path, default=DEFAULT_ENGINE)
    parser.add_argument("--release-engine", type=Path, default=DEFAULT_RELEASE_ENGINE)
    parser.add_argument("--out", type=Path, default=DEFAULT_OUT)
    parser.add_argument("--markdown-out", type=Path)
    parser.add_argument("--iterations", type=int, default=int(os.getenv("PHRUST_RATCHET_ITERATIONS", "5")))
    parser.add_argument("--warmups", type=int, default=int(os.getenv("PHRUST_RATCHET_WARMUPS", "1")))
    parser.add_argument("--timeout", type=float, default=float(os.getenv("PHRUST_RATCHET_TIMEOUT", "30.0")))
    parser.add_argument("--smoke", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    return parser.parse_args()


def safe(value: str) -> str:
    return re.sub(r"[^A-Za-z0-9_.-]+", "__", value)


def rows(engine: Path, release_engine: Path, out_root: Path) -> list[Row]:
    return [
        Row("cli.help.debug", "startup", (str(engine), "--help")),
        Row("cli.help.release.optional", "startup", (str(release_engine), "--help"), optional=True),
        Row("cli.version_or_help.debug", "startup", (str(engine), "--help")),
        Row(
            "cli.hello.baseline-ir.debug",
            "execute",
            (str(engine), "run", "--engine-preset=baseline", "fixtures/runtime/valid/hello.php"),
            "hello runtime\n",
            timings_index=3,
            counters_index=3,
        ),
        Row(
            "cli.empty.default.debug",
            "execute",
            (str(engine), "run", "--engine-preset=default", "fixtures/bytecode/lower/valid/empty.php"),
            "",
            timings_index=3,
            counters_index=3,
        ),
        Row(
            "cli.echo.default.debug",
            "execute",
            (str(engine), "run", "--engine-preset=default", "fixtures/runtime/valid/scalars/echo.php"),
            "scalar echo\n",
            timings_index=3,
            counters_index=3,
        ),
        Row(
            "cli.echo.default.release.optional",
            "execute",
            (str(release_engine), "run", "--engine-preset=default", "fixtures/runtime/valid/scalars/echo.php"),
            "scalar echo\n",
            optional=True,
            timings_index=3,
            counters_index=3,
        ),
        Row(
            "cli.compile.arithmetic.opt0",
            "compile",
            (str(engine), "compile", "--opt-level", "0", "tests/fixtures/performance/perf_smoke/arithmetic.php"),
            timings_index=4,
        ),
        Row(
            "cli.compile.arithmetic.opt2",
            "compile",
            (str(engine), "compile", "--opt-level", "2", "tests/fixtures/performance/perf_smoke/arithmetic.php"),
            timings_index=4,
        ),
        Row(
            "cli.run.arrays_packed.fast.cache-cold",
            "execute",
            (
                str(engine),
                "run",
                "--engine-preset=default",
                "--native-cache=read-write",
                "--clear-native-cache",
                "--native-cache-dir",
                str(out_root / "native-cache-cold"),
                "tests/fixtures/performance/perf_smoke/arrays_packed.php",
            ),
            (ROOT / "tests/fixtures/performance/perf_smoke/arrays_packed.php.out").read_text(encoding="utf-8"),
            timings_index=7,
            counters_index=7,
        ),
        Row(
            "cli.run.arrays_packed.fast.cache-warm",
            "execute",
            (
                str(engine),
                "run",
                "--engine-preset=default",
                "--native-cache=read-write",
                "--native-cache-dir",
                str(out_root / "native-cache-cold"),
                "tests/fixtures/performance/perf_smoke/arrays_packed.php",
            ),
            (ROOT / "tests/fixtures/performance/perf_smoke/arrays_packed.php.out").read_text(encoding="utf-8"),
            timings_index=6,
            counters_index=6,
        ),
    ]


def run_once(row: Row, run_dir: Path, iteration: int, timeout: float) -> dict[str, Any]:
    timing_path = run_dir / f"iter-{iteration}.timings.json"
    counter_path = run_dir / f"iter-{iteration}.counters.json"
    command = list(row.command)
    if row.timings_index is not None:
        command[row.timings_index:row.timings_index] = ["--timings-json", str(timing_path)]
        if row.counters_index is not None and row.counters_index >= row.timings_index:
            counters_index = row.counters_index + 2
        else:
            counters_index = row.counters_index
    else:
        counters_index = row.counters_index
    if counters_index is not None:
        command[counters_index:counters_index] = ["--counters-json", str(counter_path)]
    env = dict(os.environ)
    env.update({"TZ": "UTC", "LC_ALL": "C", "LANG": "C"})
    started = time.perf_counter_ns()
    try:
        completed = subprocess.run(
            command,
            cwd=ROOT,
            env=env,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout,
            check=False,
        )
        stdout = completed.stdout.replace("\r\n", "\n").replace("\r", "\n")
        stderr = completed.stderr.replace("\r\n", "\n").replace("\r", "\n")
        code = completed.returncode
        timed_out = False
    except subprocess.TimeoutExpired as exc:
        stdout = (exc.stdout or "").decode("utf-8", "replace") if isinstance(exc.stdout, bytes) else (exc.stdout or "")
        stderr = (exc.stderr or "").decode("utf-8", "replace") if isinstance(exc.stderr, bytes) else (exc.stderr or "")
        code = 124
        timed_out = True
    elapsed_ms = (time.perf_counter_ns() - started) / 1_000_000.0
    run_dir.mkdir(parents=True, exist_ok=True)
    (run_dir / f"iter-{iteration}.stdout").write_text(stdout, encoding="utf-8")
    (run_dir / f"iter-{iteration}.stderr").write_text(stderr, encoding="utf-8")
    timings = json.loads(timing_path.read_text(encoding="utf-8")) if timing_path.is_file() else {}
    counters = json.loads(counter_path.read_text(encoding="utf-8")) if counter_path.is_file() else {}
    return {
        "elapsed_ms": elapsed_ms,
        "exit_code": code,
        "stdout": stdout,
        "stderr": stderr,
        "timed_out": timed_out,
        "timings": timings if isinstance(timings, dict) else {},
        "counters": counters if isinstance(counters, dict) else {},
        "command": command,
    }


def measure(args: argparse.Namespace) -> dict[str, Any]:
    out = args.out if args.out.is_absolute() else ROOT / args.out
    out_root = out.parent
    iterations = 1 if args.smoke else max(args.iterations, 1)
    warmups = 0 if args.smoke else max(args.warmups, 0)
    scenarios: list[dict[str, Any]] = []
    failures: list[str] = []
    for row in rows(args.engine, args.release_engine, out_root):
        binary = Path(row.command[0])
        if row.optional and not executable(binary):
            scenarios.append(
                {
                    "id": row.scenario_id,
                    "group": "cli",
                    "kind": row.kind,
                    "correctness": "skip",
                    "metrics": {},
                    "phase_metrics": {},
                    "counter_highlights": {},
                    "artifacts": {"reason": f"binary unavailable: {rel(binary)}"},
                }
            )
            continue
        if not executable(binary):
            failures.append(f"{row.scenario_id}: binary unavailable: {rel(binary)}")
            continue
        run_dir = out_root / "runs" / safe(row.scenario_id)
        for warmup in range(warmups):
            run_once(row, run_dir, -(warmup + 1), args.timeout)
        samples = [run_once(row, run_dir, index, args.timeout) for index in range(iterations)]
        correctness = "pass"
        reason = ""
        for sample in samples:
            if sample["exit_code"] != 0 or sample["timed_out"]:
                correctness = "fail"
                reason = f"exit={sample['exit_code']} timed_out={sample['timed_out']}"
                break
            if row.expected_stdout is not None and sample["stdout"] != row.expected_stdout:
                correctness = "fail"
                reason = "stdout mismatch"
                break
        if correctness == "fail":
            failures.append(f"{row.scenario_id}: {reason}")
        external = [float(item["elapsed_ms"]) for item in samples]
        timings = [item["timings"] for item in samples if item["timings"]]
        counters = samples[-1]["counters"] if samples else {}
        scenarios.append(
            {
                "id": row.scenario_id,
                "group": "cli",
                "kind": row.kind,
                "correctness": correctness,
                "metrics": timing_metrics(external, timings),
                "phase_metrics": phase_metric_map(timings),
                "counter_highlights": counter_highlights(counters),
                "artifacts": {"run_dir": rel(run_dir), "command": samples[-1]["command"] if samples else list(row.command)},
            }
        )
    return make_report(
        run_id="cli-speed-ratchet-smoke" if args.smoke else "cli-speed-ratchet",
        created_by="cli_speed_suite.py",
        scenarios=scenarios,
        failures=failures,
    )


def run_self_test() -> int:
    report = make_report(
        run_id="self-test",
        created_by="cli_speed_suite.py",
        scenarios=[
            {
                "id": "cli.help.debug",
                "group": "cli",
                "kind": "startup",
                "correctness": "pass",
                "metrics": {"external_wall_ms.p50": 1.0},
                "phase_metrics": {},
                "counter_highlights": {},
                "artifacts": {},
            }
        ],
    )
    assert validate_report(report) == []
    print("[pass] cli_speed_suite self-test")
    return 0


def main() -> int:
    args = parse_args()
    if args.self_test:
        return run_self_test()
    report = measure(args)
    failures = validate_report(report)
    if failures:
        raise SystemExit("; ".join(failures))
    out = args.out if args.out.is_absolute() else ROOT / args.out
    markdown = args.markdown_out or out.with_suffix(".md")
    markdown = markdown if markdown.is_absolute() else ROOT / markdown
    write_json(out, report)
    markdown.parent.mkdir(parents=True, exist_ok=True)
    markdown.write_text(render_report_markdown(report, "CLI Speed Ratchet"), encoding="utf-8")
    print(f"[{'fail' if report['failures'] else 'pass'}] CLI speed ratchet wrote {rel(out)}")
    return 1 if report["failures"] else 0


if __name__ == "__main__":
    sys.exit(main())
