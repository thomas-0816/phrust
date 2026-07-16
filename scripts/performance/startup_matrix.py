#!/usr/bin/env python3
"""Measure CLI startup overhead without building inside measured commands."""

from __future__ import annotations

import argparse
import json
import os
import platform
import statistics
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_DEBUG_ENGINE = ROOT / "target/debug/php-vm"
DEFAULT_RELEASE_ENGINE = ROOT / "target/release/php-vm"
DEFAULT_FIXTURE = ROOT / "fixtures/runtime/valid/empty.php"
DEFAULT_OUT = ROOT / "target/performance/startup/summary.json"


@dataclass(frozen=True)
class StartupRow:
    id: str
    profile: str
    command: list[str]
    timing_index: int | None = None


def rel(path: Path) -> str:
    try:
        return path.resolve().relative_to(ROOT).as_posix()
    except ValueError:
        return path.as_posix()


def positive_int(value: str) -> int:
    parsed = int(value)
    if parsed < 0:
        raise argparse.ArgumentTypeError("value must be non-negative")
    return parsed


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--debug-engine", type=Path, default=DEFAULT_DEBUG_ENGINE)
    parser.add_argument("--release-engine", type=Path, default=DEFAULT_RELEASE_ENGINE)
    parser.add_argument("--fixture", type=Path, default=DEFAULT_FIXTURE)
    parser.add_argument("--out", type=Path, default=DEFAULT_OUT)
    parser.add_argument(
        "--iterations",
        type=positive_int,
        default=int(os.getenv("PHRUST_STARTUP_ITERATIONS", "3")),
    )
    parser.add_argument(
        "--warmups",
        type=positive_int,
        default=int(os.getenv("PHRUST_STARTUP_WARMUPS", "1")),
    )
    parser.add_argument(
        "--timeout",
        type=float,
        default=float(os.getenv("PHRUST_STARTUP_TIMEOUT", "10.0")),
    )
    parser.add_argument("--self-test", action="store_true")
    return parser.parse_args()


def normalized_env(out_dir: Path) -> dict[str, str]:
    tmp_dir = out_dir / "tmp"
    tmp_dir.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ)
    env.update(
        {
            "TZ": "UTC",
            "LC_ALL": "C",
            "LANG": "C",
            "TMPDIR": str(tmp_dir),
            "TMP": str(tmp_dir),
            "TEMP": str(tmp_dir),
            "PHRUST_RANDOM_SEED": "performance-startup-matrix",
            "RUST_TEST_SEED": "performance-startup-matrix",
        }
    )
    return env


def executable(path: Path) -> bool:
    return path.is_file() and os.access(path, os.X_OK)


def build_rows(debug_engine: Path, release_engine: Path, fixture: Path) -> list[StartupRow]:
    return [
        StartupRow("debug-help", "debug", [str(debug_engine), "--help"]),
        StartupRow("release-help", "release", [str(release_engine), "--help"]),
        StartupRow(
            "debug-empty-baseline",
            "debug",
            [str(debug_engine), "run", "--engine-preset=baseline", str(fixture)],
            timing_index=3,
        ),
        StartupRow(
            "release-empty-baseline",
            "release",
            [str(release_engine), "run", "--engine-preset=baseline", str(fixture)],
            timing_index=3,
        ),
        StartupRow(
            "debug-empty-fast",
            "debug",
            [str(debug_engine), "run", "--engine-preset=default", str(fixture)],
            timing_index=3,
        ),
        StartupRow(
            "release-empty-fast",
            "release",
            [str(release_engine), "run", "--engine-preset=default", str(fixture)],
            timing_index=3,
        ),
    ]


def load_timings(path: Path) -> tuple[dict[str, Any], str | None]:
    if not path.is_file():
        return {}, f"timings missing: {rel(path)}"
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        return {}, f"timings malformed: {rel(path)}: {error}"
    if not isinstance(data, dict):
        return {}, f"timings malformed: {rel(path)}: root is not an object"
    return data, None


def run_once(row: StartupRow, out_dir: Path, iteration: int, timeout: float) -> dict[str, Any]:
    timing_path = out_dir / "timings" / row.id / f"iter-{iteration}.json"
    command = list(row.command)
    if row.timing_index is not None:
        timing_path.parent.mkdir(parents=True, exist_ok=True)
        timing_path.unlink(missing_ok=True)
        command[row.timing_index:row.timing_index] = ["--timings-json", str(timing_path)]
    started = time.perf_counter_ns()
    completed = subprocess.run(
        command,
        cwd=ROOT,
        env=normalized_env(out_dir),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
        check=False,
    )
    elapsed_ms = (time.perf_counter_ns() - started) / 1_000_000.0
    timings: dict[str, Any] = {}
    warning = None
    if row.timing_index is not None:
        timings, warning = load_timings(timing_path)
    return {
        "elapsed_ms": elapsed_ms,
        "exit_code": completed.returncode,
        "stdout_bytes": len(completed.stdout.encode()),
        "stderr_bytes": len(completed.stderr.encode()),
        "phase_timings": timings,
        "timing_warning": warning,
    }


def binary_size(command: list[str]) -> int | None:
    path = Path(command[0])
    if not path.is_file():
        return None
    return path.stat().st_size


def summarize_row(row: StartupRow, samples: list[dict[str, Any]]) -> dict[str, Any]:
    timing_samples = [sample for sample in samples if sample.get("phase_timings")]
    internal_total = None
    startup_external = None
    if timing_samples:
        internal_total = statistics.median(
            float(sample["phase_timings"].get("total_internal_ms", 0.0))
            for sample in timing_samples
        )
        startup_external = max(
            statistics.median(float(sample["elapsed_ms"]) for sample in samples)
            - internal_total,
            0.0,
        )
    warnings = [
        sample["timing_warning"]
        for sample in samples
        if isinstance(sample.get("timing_warning"), str)
    ]
    return {
        "id": row.id,
        "profile": row.profile,
        "command": [
            rel(Path(part)) if index == 0 or part.endswith(".php") else part
            for index, part in enumerate(row.command)
        ],
        "status": "pass" if all(sample["exit_code"] == 0 for sample in samples) else "fail",
        "external_wall_ms": statistics.median(float(sample["elapsed_ms"]) for sample in samples),
        "internal_total_ms": internal_total,
        "startup_external_ms": startup_external,
        "binary_size_bytes": binary_size(row.command),
        "iterations": len(samples),
        "samples": samples,
        "timing_warnings": warnings,
    }


def render_markdown(summary: dict[str, Any]) -> str:
    lines = [
        "# Startup Performance Matrix",
        "",
        "| Row | Profile | External ms | Internal ms | Startup ms | Binary bytes | Status |",
        "| --- | --- | --- | --- | --- | --- | --- |",
    ]
    for row in summary["rows"]:
        internal = row["internal_total_ms"]
        startup = row["startup_external_ms"]
        lines.append(
            f"| `{row['id']}` | `{row['profile']}` | {row['external_wall_ms']:.3f} | "
            f"{'n/a' if internal is None else f'{internal:.3f}'} | "
            f"{'n/a' if startup is None else f'{startup:.3f}'} | "
            f"{row['binary_size_bytes'] or 'n/a'} | `{row['status']}` |"
        )
    if summary["skipped"]:
        lines.extend(["", "## Skipped", ""])
        lines.extend(f"- {item}" for item in summary["skipped"])
    if summary["timing_warnings"]:
        lines.extend(["", "## Timing Warnings", ""])
        lines.extend(f"- {item}" for item in summary["timing_warnings"])
    return "\n".join(lines) + "\n"


def run_self_test() -> int:
    row = StartupRow("debug-help", "debug", ["target/debug/php-vm", "--help"])
    sample = {"elapsed_ms": 2.0, "exit_code": 0, "phase_timings": {}, "timing_warning": None}
    summary = {
        "rows": [summarize_row(row, [sample])],
        "skipped": [],
        "timing_warnings": [],
    }
    markdown = render_markdown(summary)
    assert "Startup Performance Matrix" in markdown
    assert summary["rows"][0]["external_wall_ms"] == 2.0
    print("[pass] startup_matrix self-test")
    return 0


def run_matrix(args: argparse.Namespace) -> int:
    if args.timeout <= 0:
        raise SystemExit("--timeout must be positive")
    fixture = args.fixture if args.fixture.is_absolute() else ROOT / args.fixture
    if not fixture.is_file():
        raise SystemExit(f"startup fixture is missing: {rel(fixture)}")
    out = args.out if args.out.is_absolute() else ROOT / args.out
    out_dir = out.parent
    out_dir.mkdir(parents=True, exist_ok=True)
    rows: list[dict[str, Any]] = []
    skipped: list[str] = []
    failed = False
    for row in build_rows(args.debug_engine, args.release_engine, fixture):
        engine_path = Path(row.command[0])
        if not executable(engine_path):
            skipped.append(f"{row.id}: engine unavailable: {rel(engine_path)}")
            continue
        for warmup in range(args.warmups):
            run_once(row, out_dir, -(warmup + 1), args.timeout)
        samples = [
            run_once(row, out_dir, iteration, args.timeout)
            for iteration in range(max(args.iterations, 1))
        ]
        summary_row = summarize_row(row, samples)
        failed = failed or summary_row["status"] != "pass"
        rows.append(summary_row)
    timing_warnings = [
        warning
        for row in rows
        for warning in row.get("timing_warnings", [])
        if isinstance(warning, str)
    ]
    summary = {
        "schema_version": 1,
        "status": "fail" if failed else "pass",
        "environment": {
            "platform": platform.platform(),
            "python": platform.python_version(),
            "fixture": rel(fixture),
        },
        "rows": rows,
        "skipped": skipped,
        "timing_warnings": timing_warnings,
    }
    out.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    markdown_out = out.with_suffix(".md")
    markdown_out.write_text(render_markdown(summary), encoding="utf-8")
    print(f"[pass] startup matrix wrote {rel(out)}")
    if skipped:
        for item in skipped:
            print(f"[skip] {item}")
    return 1 if failed else 0


def main() -> int:
    args = parse_args()
    if args.self_test:
        return run_self_test()
    return run_matrix(args)


if __name__ == "__main__":
    sys.exit(main())
