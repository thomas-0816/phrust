#!/usr/bin/env python3
"""Run deterministic application-flow fixtures on Phrust and reference PHP."""

from __future__ import annotations

import argparse
import json
import os
import re
import statistics
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import overhead_attribution
from normalize_perf_output import normalize
from ratchet_schema import (
    counter_highlights as ratchet_counter_highlights,
    make_report,
    phase_metric_map,
    render_report_markdown,
    timing_metrics,
    validate_report,
    write_json,
)


ROOT = Path(__file__).resolve().parents[2]
MANIFEST = ROOT / "tests/fixtures/performance/app_flows/manifest.json"
FIXTURE_DIR = MANIFEST.parent
DEFAULT_ENGINE = ROOT / "target/debug/php-vm"
DEFAULT_RELEASE_ENGINE = ROOT / "target/release/php-vm"
DEFAULT_REFERENCE = ROOT / "third_party/php-src/sapi/cli/php"
DEFAULT_OUT_DIR = ROOT / "target/performance/app-flows"
SUMMARY_DOC = ROOT / "target/performance/app-flow-results.md"


@dataclass(frozen=True)
class Scenario:
    id: str
    name: str
    group: str
    shape: str
    fixture: Path
    expected_output: str
    expected_checksum: int
    summary: str
    rationale: str


@dataclass(frozen=True)
class Row:
    label: str
    kind: str
    command_prefix: tuple[str, ...]
    optional: bool = False
    counters: bool = False


@dataclass(frozen=True)
class RunSample:
    elapsed_ms: float
    returncode: int
    stdout: str
    stderr: str
    command: list[str]
    counters: dict[str, Any]
    timings: dict[str, Any]
    timing_warning: str | None = None
    timed_out: bool = False


def rel(path: Path) -> str:
    try:
        return path.resolve().relative_to(ROOT).as_posix()
    except ValueError:
        return path.as_posix()


def safe_name(value: str) -> str:
    return re.sub(r"[^A-Za-z0-9_.-]+", "__", value)


def positive_int(value: str) -> int:
    parsed = int(value)
    if parsed < 0:
        raise argparse.ArgumentTypeError("value must be non-negative")
    return parsed


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--engine", type=Path, default=DEFAULT_ENGINE)
    parser.add_argument("--release-engine", type=Path, default=DEFAULT_RELEASE_ENGINE)
    parser.add_argument("--reference-php", type=Path)
    parser.add_argument("--out-dir", type=Path, default=DEFAULT_OUT_DIR)
    parser.add_argument("--iterations", type=positive_int, default=5)
    parser.add_argument("--warmups", type=positive_int, default=1)
    parser.add_argument("--scale", type=positive_int, default=2)
    parser.add_argument(
        "--timeout",
        type=float,
        default=float(os.getenv("PHRUST_APP_FLOW_TIMEOUT", "30.0")),
    )
    parser.add_argument("--smoke", action="store_true")
    parser.add_argument("--allow-missing-reference", action="store_true")
    parser.add_argument("--ratchet-out", type=Path)
    parser.add_argument("--ratchet-markdown-out", type=Path)
    parser.add_argument("--self-test", action="store_true")
    return parser.parse_args()


def load_manifest(path: Path = MANIFEST) -> list[Scenario]:
    data = json.loads(path.read_text(encoding="utf-8"))
    if data.get("schema") != "phrust-app-flow-performance-v1":
        raise SystemExit(f"{rel(path)}: unsupported or missing schema")
    raw_scenarios = data.get("scenarios")
    if not isinstance(raw_scenarios, list) or len(raw_scenarios) != 10:
        raise SystemExit(f"{rel(path)}: expected exactly ten scenarios")
    scenarios: list[Scenario] = []
    seen: set[str] = set()
    required = {
        "id",
        "name",
        "group",
        "shape",
        "fixture",
        "expected_output",
        "expected_checksum",
        "summary",
        "rationale",
    }
    for index, item in enumerate(raw_scenarios):
        if not isinstance(item, dict):
            raise SystemExit(f"{rel(path)}: scenario {index} is not an object")
        missing = sorted(required - set(item))
        if missing:
            raise SystemExit(f"{rel(path)}: scenario {index} missing {missing}")
        scenario_id = str(item["id"])
        if scenario_id in seen:
            raise SystemExit(f"{rel(path)}: duplicate scenario id {scenario_id}")
        seen.add(scenario_id)
        fixture = FIXTURE_DIR / str(item["fixture"])
        if fixture.name != str(item["fixture"]) or not fixture.is_file():
            raise SystemExit(f"{rel(path)}: missing fixture for {scenario_id}: {item['fixture']}")
        expected_output = str(item["expected_output"])
        checksum = item["expected_checksum"]
        if not isinstance(checksum, int):
            raise SystemExit(f"{rel(path)}: {scenario_id} expected_checksum must be int")
        if expected_output != expected_output.strip() or "\n" in expected_output:
            raise SystemExit(f"{rel(path)}: {scenario_id} expected output must be one line")
        if not expected_output.startswith(f"app-flow {scenario_id} checksum={checksum} items="):
            raise SystemExit(f"{rel(path)}: {scenario_id} expected output/checksum mismatch")
        scenarios.append(
            Scenario(
                id=scenario_id,
                name=str(item["name"]),
                group=str(item["group"]),
                shape=str(item["shape"]),
                fixture=fixture,
                expected_output=expected_output + "\n",
                expected_checksum=checksum,
                summary=str(item["summary"]),
                rationale=str(item["rationale"]),
            )
        )
    return scenarios


def executable(path: Path) -> bool:
    return path.is_file() and os.access(path, os.X_OK)


def resolve_path(path: Path) -> Path:
    return path if path.is_absolute() else ROOT / path


def resolve_reference(args: argparse.Namespace) -> tuple[Path, str]:
    if args.reference_php is not None:
        return resolve_path(args.reference_php), "argument"
    env_reference = os.getenv("REFERENCE_PHP")
    if env_reference:
        return resolve_path(Path(env_reference)), "environment"
    return DEFAULT_REFERENCE, "default"


def run_rows(args: argparse.Namespace, reference: Path, reference_available: bool) -> list[Row]:
    engine = resolve_path(args.engine)
    release_engine = resolve_path(args.release_engine)
    rows: list[Row] = []
    if reference_available:
        rows.extend(
            [
                Row("reference-php-cli", "reference", (str(reference),)),
                Row(
                    "reference-php-cli-opcache",
                    "reference",
                    (
                        str(reference),
                        "-d",
                        "opcache.enable_cli=1",
                        "-d",
                        "opcache.jit=0",
                    ),
                    optional=True,
                ),
            ]
        )
    rows.extend(
        [
            Row(
                "phrust-baseline-ir",
                "phrust",
                (str(engine), "run", "--engine-preset=baseline"),
            ),
            Row(
                "phrust-default",
                "phrust",
                (str(engine), "run", "--engine-preset=default"),
                counters=True,
            ),
        ]
    )
    if not args.smoke:
        rows.append(
            Row(
                "phrust-release-fast",
                "phrust",
                (str(release_engine), "run", "--engine-preset=default"),
                optional=True,
                counters=True,
            )
        )
    return rows


def normalized_env(out_dir: Path, scenario: Scenario, row: Row, scale: int) -> dict[str, str]:
    tmp_dir = out_dir / "tmp" / safe_name(scenario.id) / safe_name(row.label)
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
            "PHRUST_APP_FLOW_SCALE": str(scale),
            "PHRUST_RANDOM_SEED": "performance-app-flow-matrix",
            "RUST_TEST_SEED": "performance-app-flow-matrix",
        }
    )
    return env


def build_command(
    row: Row,
    fixture: str,
    scale: int,
    *,
    instrumented: bool,
    counters_path: Path,
    timings_path: Path,
) -> list[str]:
    """Build the per-run command.

    Timed iterations must stay uninstrumented: `--timings-json` alone forces
    counter collection inside the CLI, which measurably inflates wall time.
    Phase timings and counters come from one dedicated instrumented run.
    """
    command = [*row.command_prefix]
    if row.kind == "phrust":
        command.extend(["--env", f"PHRUST_APP_FLOW_SCALE={scale}"])
        if instrumented:
            command.extend(["--timings-json", str(timings_path)])
            if row.counters:
                command.extend(["--counters-json", str(counters_path)])
    command.append(fixture)
    return command


def run_once(
    scenario: Scenario,
    row: Row,
    out_dir: Path,
    label: str,
    scale: int,
    timeout: float,
    *,
    instrumented: bool = False,
) -> RunSample:
    run_dir = out_dir / "runs" / safe_name(scenario.id) / safe_name(row.label)
    run_dir.mkdir(parents=True, exist_ok=True)
    counters_path = run_dir / f"iter-{label}.counters.json"
    timings_path = run_dir / f"iter-{label}.timings.json"
    if instrumented:
        # out_dir is a fixed path, so a crashed/timed-out run must not let a
        # previous invocation's artifacts pass as this run's counters/timings.
        counters_path.unlink(missing_ok=True)
        timings_path.unlink(missing_ok=True)
    command = build_command(
        row,
        rel(scenario.fixture),
        scale,
        instrumented=instrumented,
        counters_path=counters_path,
        timings_path=timings_path,
    )
    start = time.perf_counter_ns()
    timed_out = False
    try:
        completed = subprocess.run(
            command,
            cwd=ROOT,
            env=normalized_env(out_dir, scenario, row, scale),
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout,
            check=False,
        )
        returncode = completed.returncode
        stdout = completed.stdout.replace("\r\n", "\n").replace("\r", "\n")
        stderr = normalize(completed.stderr)
    except subprocess.TimeoutExpired as exc:
        timed_out = True
        returncode = 124
        raw_stdout = exc.stdout or ""
        raw_stderr = exc.stderr or ""
        if isinstance(raw_stdout, bytes):
            raw_stdout = raw_stdout.decode("utf-8", errors="replace")
        if isinstance(raw_stderr, bytes):
            raw_stderr = raw_stderr.decode("utf-8", errors="replace")
        stdout = raw_stdout.replace("\r\n", "\n").replace("\r", "\n")
        stderr = normalize(raw_stderr)
        timeout_line = f"timed out after {timeout:.3f}s"
        stderr = f"{stderr}\n{timeout_line}\n" if stderr else f"{timeout_line}\n"
    elapsed_ms = (time.perf_counter_ns() - start) / 1_000_000.0
    (run_dir / f"iter-{label}.stdout").write_text(stdout, encoding="utf-8")
    (run_dir / f"iter-{label}.stderr").write_text(stderr, encoding="utf-8")
    (run_dir / f"iter-{label}.status").write_text(f"{returncode}\n", encoding="utf-8")
    (run_dir / f"iter-{label}.command.json").write_text(
        json.dumps(command, indent=2) + "\n", encoding="utf-8"
    )
    counters: dict[str, Any] = {}
    if instrumented and counters_path.is_file():
        counters = json.loads(counters_path.read_text(encoding="utf-8"))
    if not isinstance(counters, dict):
        raise SystemExit(f"{rel(counters_path)}: counters root is not an object")
    timings: dict[str, Any] = {}
    timing_warning = None
    if row.kind == "phrust" and instrumented:
        if not timings_path.is_file():
            timing_warning = f"timings missing: {rel(timings_path)}"
        else:
            try:
                raw_timings = json.loads(timings_path.read_text(encoding="utf-8"))
            except (OSError, json.JSONDecodeError) as error:
                raw_timings = {}
                timing_warning = f"timings malformed: {rel(timings_path)}: {error}"
            if isinstance(raw_timings, dict):
                timings = raw_timings
            else:
                timing_warning = f"timings malformed: {rel(timings_path)}: root is not an object"
    return RunSample(
        elapsed_ms=elapsed_ms,
        returncode=returncode,
        stdout=stdout,
        stderr=stderr,
        command=command,
        counters=counters,
        timings=timings,
        timing_warning=timing_warning,
        timed_out=timed_out,
    )


def run_samples(
    scenario: Scenario,
    row: Row,
    out_dir: Path,
    warmups: int,
    iterations: int,
    scale: int,
    timeout: float,
) -> tuple[list[RunSample], RunSample | None]:
    """Run warmups, clean timed iterations, and one instrumented sample.

    Returns `(timed_samples, instrumented_sample)`. Timed samples carry wall
    times only; the instrumented sample (phrust rows only) supplies counters
    and phase timings without polluting the timed measurements.
    """
    for warmup in range(warmups):
        run_once(scenario, row, out_dir, f"-{warmup + 1}", scale, timeout)
    timed = [
        run_once(scenario, row, out_dir, str(iteration), scale, timeout)
        for iteration in range(iterations)
    ]
    instrumented: RunSample | None = None
    if row.kind == "phrust":
        instrumented = run_once(
            scenario, row, out_dir, "instrumented", scale, timeout, instrumented=True
        )
    return timed, instrumented


def median_ms(samples: list[RunSample]) -> float:
    return statistics.median(sample.elapsed_ms for sample in samples)


def min_ms(samples: list[RunSample]) -> float:
    return min(sample.elapsed_ms for sample in samples)


def max_ms(samples: list[RunSample]) -> float:
    return max(sample.elapsed_ms for sample in samples)


def counter_focus(counters: dict[str, Any]) -> dict[str, int]:
    keys = [
        "instructions_executed",
        "bytecode_instructions_executed",
        "quickening_specialized",
        "quickening_guard_hits",
        "quickening_guard_misses",
        "quickening_fallback_calls",
        "inline_cache_hits",
        "inline_cache_misses",
        "function_call_cache_hits",
        "method_call_ic_hits",
        "property_load_ic_hits",
        "property_assign_ic_hits",
        "output_fast_appends",
        "output_batched_appends",
        "string_concat_fast_path_hits",
        "packed_fetch_fast_hits",
        "array_packed_append_fast_path_hits",
        "array_sequential_foreach_fast_path_hits",
        "internal_function_dispatch_cache_hits",
        "typecheck_fast_path_hits",
    ]
    return {
        key: value
        for key in keys
        if isinstance((value := counters.get(key)), int) and value != 0
    }


def phase_ms(timings: dict[str, Any], name: str) -> float:
    phases = timings.get("phases")
    if not isinstance(phases, dict):
        return 0.0
    value = phases.get(name)
    return float(value) if isinstance(value, (int, float)) else 0.0


def compile_total_ms(timings: dict[str, Any]) -> float:
    return sum(
        phase_ms(timings, name)
        for name in (
            "cache_prepare_ms",
            "cache_load_ms",
            "cache_store_ms",
            "source_read_ms",
            "frontend_analyze_ms",
            "ir_lower_ms",
            "ir_verify_ms",
            "optimizer_ms",
            "bytecode_lower_ms",
            "bytecode_layout_ms",
            "superinstruction_select_ms",
            "vm_construct_ms",
            "diagnostics_ms",
            "counters_write_ms",
            "timings_write_ms",
        )
    )


def phase_summary(
    timed: list[RunSample], instrumented: RunSample | None
) -> dict[str, float]:
    """Combine clean timed walls with the instrumented run's phase internals.

    `external_wall_ms` is the median of the uninstrumented timed runs. The
    phase breakdown - and `startup_external_ms` - come from the dedicated
    instrumented run: startup is that run's own wall minus its own internal
    total, so instrumentation overhead cancels out instead of biasing the
    subtraction (mixing the clean wall with the instrumented internal
    understated startup, clamping it to 0.0 on some rows).
    """
    if instrumented is None or not instrumented.timings or not timed:
        return {}
    external = statistics.median(sample.elapsed_ms for sample in timed)
    internal = float(instrumented.timings.get("total_internal_ms", 0.0))
    compile_total = compile_total_ms(instrumented.timings)
    execute = phase_ms(instrumented.timings, "execute_ms")
    return {
        "external_wall_ms": external,
        "internal_total_ms": internal,
        "startup_external_ms": max(instrumented.elapsed_ms - internal, 0.0),
        "compile_total_ms": compile_total,
        "execute_ms": execute,
        "compile_share_percent": (compile_total / internal * 100.0) if internal > 0 else 0.0,
        "execute_share_percent": (execute / internal * 100.0) if internal > 0 else 0.0,
    }


def should_write_summary_doc(mode: str) -> bool:
    """Only the full matrix refreshes the committed summary doc.

    Smoke runs use scale 1, one iteration, and the debug binary; letting them
    overwrite `target/performance/app-flow-results.md` replaced the committed
    full-matrix numbers with debug-build noise.
    """
    return mode == "full"


def compare_sample(
    scenario: Scenario,
    row: Row,
    sample: RunSample,
    expected: RunSample | None,
    scale: int,
) -> tuple[str, str]:
    if sample.timed_out:
        return "fail", f"timed out after {sample.elapsed_ms / 1000.0:.3f}s"
    if scale == 1 and sample.stdout != scenario.expected_output:
        return "fail", "stdout does not match manifest expected output"
    if scale != 1:
        prefix = f"app-flow {scenario.id} checksum="
        if not sample.stdout.startswith(prefix) or sample.stdout.count("\n") != 1:
            return "fail", "stdout does not match app-flow one-line contract"
    if expected is None:
        if sample.returncode != 0:
            return "fail", f"exit status {sample.returncode}; expected 0"
        if sample.stderr != "":
            return "fail", "stderr is not empty"
        return "manifest", ""
    differences: list[str] = []
    if sample.returncode != expected.returncode:
        differences.append(
            f"exit status expected={expected.returncode} actual={sample.returncode}"
        )
    if sample.stdout != expected.stdout:
        differences.append("stdout differs")
    if sample.stderr != expected.stderr:
        differences.append("stderr differs")
    if differences:
        return "fail", "; ".join(differences)
    if row.kind == "reference":
        return "reference", ""
    return "pass", ""


def row_available(row: Row) -> tuple[bool, str]:
    if row.kind == "phrust":
        engine = Path(row.command_prefix[0])
        if not executable(engine):
            return False, f"engine unavailable: {rel(engine)}"
    return True, ""


def build_summary(
    *,
    mode: str,
    scenarios: list[Scenario],
    rows: list[dict[str, Any]],
    skipped_rows: int,
    iterations: int,
    warmups: int,
    scale: int,
    timeout: float,
    reference: Path,
    reference_status: str,
    reference_source: str,
    failures: list[str],
) -> dict[str, Any]:
    return {
        "status": "fail" if failures else "pass",
        "gate": "app-flow-matrix",
        "mode": mode,
        "scenario_count": len(scenarios),
        "scenarios": [
            {
                "id": scenario.id,
                "name": scenario.name,
                "group": scenario.group,
                "shape": scenario.shape,
                "fixture": rel(scenario.fixture),
                "expected_output": scenario.expected_output.strip(),
                "expected_checksum": scenario.expected_checksum,
                "summary": scenario.summary,
                "rationale": scenario.rationale,
            }
            for scenario in scenarios
        ],
        "iterations": iterations,
        "warmups": warmups,
        "scale": scale,
        "timeout_seconds": timeout,
        "timing_policy": "advisory-host-local",
        "reference_php": rel(reference),
        "reference_source": reference_source,
        "reference_status": reference_status,
        "rows": rows,
        "skipped_row_count": skipped_rows,
        "failures": failures,
    }


def render_markdown(summary: dict[str, Any]) -> str:
    lines = [
        "# Application-Flow Performance Results",
        "",
        "Generated by `nix develop -c just app-flow-matrix` (full mode only;",
        "`just app-flow-smoke` writes local artifacts without touching this doc).",
        "Raw stdout, stderr, command, status, and counter artifacts are local-only",
        "under `target/performance/app-flows/` and must not be committed.",
        "",
        "Correctness is mandatory. When reference PHP is available, Phrust rows",
        "must match reference stdout, normalized stderr, and exit status before",
        "timing data is reported. Wall-clock timings are advisory host-local data.",
        "",
        "Timed iterations run uninstrumented. Startup/Compile/Execute columns and",
        "counter highlights come from one dedicated instrumented run per row and",
        "are not part of the timed medians. Per-scenario overhead-family",
        "attribution lands in `target/performance/app-flows/overhead.{json,md}`;",
        "family definitions live in `docs/performance/counter-families.md`.",
        "",
        "## Summary",
        "",
        "| Field | Value |",
        "| --- | --- |",
        f"| Status | `{summary['status']}` |",
        f"| Mode | `{summary['mode']}` |",
        f"| Scenarios | {summary['scenario_count']} |",
        f"| Reference PHP | `{summary['reference_status']}` from `{summary['reference_php']}` |",
        f"| Warmups | {summary['warmups']} |",
        f"| Iterations | {summary['iterations']} |",
        f"| Scale | {summary['scale']} |",
        f"| Timeout seconds | {summary['timeout_seconds']:.3f} |",
        f"| Skipped rows | {summary['skipped_row_count']} |",
        f"| Failures | {len(summary['failures'])} |",
        "",
        "## Matrix",
        "",
        "| Scenario | Row | Correctness | Median ms | Ratio vs ref | Startup ms | Compile ms | Execute ms | Counter highlights | Skip/failure reason |",
        "| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |",
    ]
    for row in summary["rows"]:
        if row["status"] == "skip":
            lines.append(
                f"| `{row['scenario']}` | `{row['row']}` | skip | n/a | n/a | n/a | n/a | n/a | n/a | {row['reason']} |"
            )
            continue
        counters = row.get("counter_highlights", {})
        focus = ", ".join(f"{key}={value}" for key, value in counters.items())
        ratio = row["ratio_vs_reference"]
        ratio_text = "n/a" if ratio is None else f"{ratio:.3f}"
        phases = row.get("phase_summary", {})
        startup_text = (
            f"{phases['startup_external_ms']:.3f}"
            if isinstance(phases, dict) and "startup_external_ms" in phases
            else "n/a"
        )
        compile_text = (
            f"{phases['compile_total_ms']:.3f}"
            if isinstance(phases, dict) and "compile_total_ms" in phases
            else "n/a"
        )
        execute_text = (
            f"{phases['execute_ms']:.3f}"
            if isinstance(phases, dict) and "execute_ms" in phases
            else "n/a"
        )
        reason = row.get("reason", "")
        lines.append(
            f"| `{row['scenario']}` | `{row['row']}` | `{row['correctness']}` | "
            f"{row['median_ms']:.3f} | {ratio_text} | {startup_text} | {compile_text} | {execute_text} | "
            f"{focus or 'n/a'} | {reason or 'n/a'} |"
        )
    if summary["failures"]:
        lines.extend(["", "## Failures", ""])
        for failure in summary["failures"]:
            lines.append(f"- {failure}")
    return "\n".join(lines) + "\n"


def ratchet_from_summary(summary: dict[str, Any], run_id: str) -> dict[str, Any]:
    scenarios: list[dict[str, Any]] = []
    baseline_by_scenario = {
        row["scenario"]: row
        for row in summary.get("rows", [])
        if isinstance(row, dict)
        and row.get("row") == "phrust-baseline-ir"
        and isinstance(row.get("median_ms"), (int, float))
    }
    for row in summary.get("rows", []):
        if not isinstance(row, dict) or row.get("status") == "skip":
            if isinstance(row, dict):
                scenarios.append(
                    {
                        "id": f"app-flow.{row.get('scenario', 'unknown')}.{row.get('row', 'unknown')}",
                        "group": "app-flow",
                        "kind": "execute",
                        "correctness": "skip",
                        "metrics": {},
                        "phase_metrics": {},
                        "counter_highlights": {},
                        "artifacts": {"reason": row.get("reason", "")},
                    }
                )
            continue
        samples = row.get("samples") if isinstance(row.get("samples"), list) else []
        external = [
            float(sample["elapsed_ms"])
            for sample in samples
            if isinstance(sample, dict) and isinstance(sample.get("elapsed_ms"), (int, float))
        ]
        # Timed samples are uninstrumented; phase timings come from the row's
        # dedicated instrumented run. Wall distributions are computed from
        # clean timed samples only, while the internal/compile/execute/startup
        # metric families are derived from the instrumented run against its
        # own wall so instrumentation overhead cancels out. Single-sample
        # distributions keep the metric keys ratchet-comparable.
        timings = []
        row_timings = row.get("phase_timings")
        if isinstance(row_timings, dict) and row_timings:
            timings = [row_timings]
        metrics = timing_metrics(external or [float(row.get("median_ms", 0.0))], [])
        instrumented_sample = row.get("instrumented_sample")
        if (
            timings
            and isinstance(instrumented_sample, dict)
            and isinstance(instrumented_sample.get("elapsed_ms"), (int, float))
        ):
            instrumented_metrics = timing_metrics(
                [float(instrumented_sample["elapsed_ms"])], timings
            )
            metrics.update(
                {
                    key: value
                    for key, value in instrumented_metrics.items()
                    if not key.startswith("external_wall_ms")
                }
            )
        phase_summary_value = row.get("phase_summary")
        if isinstance(phase_summary_value, dict):
            for key in ("compile_total_ms", "execute_ms", "internal_total_ms", "startup_external_ms"):
                value = phase_summary_value.get(key)
                if isinstance(value, (int, float)):
                    metrics.setdefault(f"{key}.p50", float(value))
        ratio = row.get("ratio_vs_reference")
        if isinstance(ratio, (int, float)):
            metrics["ratio_vs_reference.external_wall_ms.p50"] = float(ratio)
        baseline = baseline_by_scenario.get(row.get("scenario"))
        if baseline is not None and baseline is not row and isinstance(baseline.get("median_ms"), (int, float)):
            baseline_wall = float(baseline["median_ms"])
            if baseline_wall > 0:
                metrics["ratio_vs_baseline.external_wall_ms.p50"] = float(row.get("median_ms", 0.0)) / baseline_wall
            baseline_phase = baseline.get("phase_summary") if isinstance(baseline.get("phase_summary"), dict) else {}
            phase_summary_value = row.get("phase_summary") if isinstance(row.get("phase_summary"), dict) else {}
            for metric_name in ("compile_total_ms", "execute_ms"):
                before = baseline_phase.get(metric_name)
                after = phase_summary_value.get(metric_name)
                if isinstance(before, (int, float)) and before > 0 and isinstance(after, (int, float)):
                    metrics[f"ratio_vs_baseline.{metric_name}.p50"] = float(after) / float(before)
        counters = row.get("counters") if isinstance(row.get("counters"), dict) else {}
        scenarios.append(
            {
                "id": f"app-flow.{row.get('scenario')}.{row.get('row')}",
                "group": "app-flow",
                "kind": "execute",
                "correctness": "fail" if row.get("status") == "fail" else "pass",
                "metrics": metrics,
                "phase_metrics": phase_metric_map(timings),
                "counter_highlights": {
                    **ratchet_counter_highlights(counters),
                    **{
                        key: value
                        for key, value in row.get("counter_highlights", {}).items()
                        if isinstance(key, str) and isinstance(value, int)
                    },
                },
                "artifacts": {
                    "scenario": str(row.get("scenario", "")),
                    "row": str(row.get("row", "")),
                },
            }
        )
    return make_report(
        run_id=run_id,
        created_by="app_flow_matrix.py",
        scenarios=scenarios,
        failures=[str(item) for item in summary.get("failures", [])],
    )


def run_matrix(args: argparse.Namespace) -> int:
    if args.timeout <= 0:
        raise SystemExit("--timeout must be positive")
    scenarios = load_manifest()
    out_dir = resolve_path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    iterations = 1 if args.smoke else max(args.iterations, 1)
    warmups = 0 if args.smoke else args.warmups
    scale = 1 if args.smoke else max(args.scale, 1)
    reference, reference_source = resolve_reference(args)
    reference_available = executable(reference)
    mode = "smoke" if args.smoke else "full"
    if not reference_available and not args.smoke and not args.allow_missing_reference:
        raise SystemExit(
            "reference PHP is required for full app-flow matrix; set REFERENCE_PHP, "
            "pass --reference-php, or use --allow-missing-reference"
        )
    engine = resolve_path(args.engine)
    if not executable(engine):
        raise SystemExit(f"Phrust engine is not executable: {rel(engine)}")

    reference_status = "available" if reference_available else "skipped"
    matrix_rows = run_rows(args, reference, reference_available)
    rows: list[dict[str, Any]] = []
    failures: list[str] = []
    skipped = 0

    for scenario in scenarios:
        reference_sample: RunSample | None = None
        reference_median: float | None = None
        scenario_results: dict[str, list[RunSample]] = {}

        for row in matrix_rows:
            available, reason = row_available(row)
            if row.optional and not available:
                skipped += 1
                rows.append(
                    {
                        "scenario": scenario.id,
                        "row": row.label,
                        "status": "skip",
                        "reason": reason,
                    }
                )
                continue
            if not available:
                failure = f"{scenario.id} {row.label}: {reason}"
                failures.append(failure)
                rows.append(
                    {
                        "scenario": scenario.id,
                        "row": row.label,
                        "status": "fail",
                        "correctness": "fail",
                        "reason": reason,
                        "median_ms": 0.0,
                        "min_ms": 0.0,
                        "max_ms": 0.0,
                        "ratio_vs_reference": None,
                        "samples": [],
                        "command": list(row.command_prefix),
                        "counter_highlights": {},
                    }
                )
                continue
            if row.optional and row.kind == "reference":
                try:
                    samples, instrumented = run_samples(
                        scenario,
                        row,
                        out_dir,
                        warmups,
                        iterations,
                        scale,
                        args.timeout,
                    )
                except (subprocess.SubprocessError, OSError) as exc:
                    skipped += 1
                    rows.append(
                        {
                            "scenario": scenario.id,
                            "row": row.label,
                            "status": "skip",
                            "reason": f"optional reference row failed: {exc}",
                        }
                    )
                    continue
            else:
                samples, instrumented = run_samples(
                    scenario,
                    row,
                    out_dir,
                    warmups,
                    iterations,
                    scale,
                    args.timeout,
                )
            scenario_results[row.label] = samples
            sample = samples[0]
            if row.label == "reference-php-cli":
                reference_sample = sample
                reference_median = median_ms(samples)
            expected = reference_sample
            if row.label == "reference-php-cli":
                expected = None
            elif reference_sample is None and not reference_available:
                expected = scenario_results.get("phrust-baseline-ir", [sample])[0]
                if row.label == "phrust-baseline-ir":
                    expected = None
            correctness, reason = compare_sample(scenario, row, sample, expected, scale)
            if correctness == "fail":
                failures.append(f"{scenario.id} {row.label}: {reason}")
            elif instrumented is not None:
                # The instrumented run is the sole source of the published
                # counters and phase timings, so it must complete and must
                # not change PHP-visible behavior (stdout, stderr, exit).
                if instrumented.timed_out:
                    correctness, reason = (
                        "fail",
                        "instrumented run timed out; counters/phase timings unavailable",
                    )
                    failures.append(f"{scenario.id} {row.label}: {reason}")
                elif (
                    instrumented.stdout != sample.stdout
                    or instrumented.stderr != sample.stderr
                    or instrumented.returncode != sample.returncode
                ):
                    correctness, reason = (
                        "fail",
                        "instrumented run output differs from timed run",
                    )
                    failures.append(f"{scenario.id} {row.label}: {reason}")
            row_median = median_ms(samples)
            ratio = None
            if reference_median and row_median > 0:
                ratio = row_median / reference_median
            rows.append(
                {
                    "scenario": scenario.id,
                    "row": row.label,
                    "status": "pass" if correctness != "fail" else "fail",
                    "correctness": correctness,
                    "reason": reason,
                    "median_ms": row_median,
                    "min_ms": min_ms(samples),
                    "max_ms": max_ms(samples),
                    "ratio_vs_reference": ratio,
                    "iterations": iterations,
                    "warmups": warmups,
                    "scale": scale,
                    "engine_label": row.label,
                    "command": sample.command,
                    "samples": [
                        {
                            "elapsed_ms": item.elapsed_ms,
                            "exit_code": item.returncode,
                            "stdout": item.stdout,
                            "stderr": item.stderr,
                            "timed_out": item.timed_out,
                            "phase_timings": item.timings,
                            "timing_warning": item.timing_warning,
                        }
                        for item in samples
                    ],
                    "instrumented_sample": (
                        {
                            "elapsed_ms": instrumented.elapsed_ms,
                            "exit_code": instrumented.returncode,
                            "timed_out": instrumented.timed_out,
                            "phase_timings": instrumented.timings,
                            "timing_warning": instrumented.timing_warning,
                        }
                        if instrumented is not None
                        else None
                    ),
                    "counters": instrumented.counters if instrumented is not None else {},
                    "phase_timings": instrumented.timings if instrumented is not None else {},
                    "phase_summary": phase_summary(samples, instrumented),
                    "timing_warnings": [
                        item.timing_warning
                        for item in (*samples, *((instrumented,) if instrumented else ()))
                        if item.timing_warning
                    ],
                    "counter_highlights": counter_focus(
                        instrumented.counters if instrumented is not None else {}
                    ),
                    "overhead_families": (
                        overhead_attribution.attribute(instrumented.counters)
                        if instrumented is not None and instrumented.counters
                        else {}
                    ),
                }
            )

        if reference_sample is None and not reference_available:
            baseline = scenario_results.get("phrust-baseline-ir")
            fast = scenario_results.get("phrust-default")
            if baseline and fast:
                correctness, reason = compare_sample(
                    scenario, matrix_rows[1], fast[0], baseline[0], scale
                )
                if correctness == "fail":
                    failures.append(f"{scenario.id} phrust-default: {reason}")

    summary = build_summary(
        mode=mode,
        scenarios=scenarios,
        rows=rows,
        skipped_rows=skipped,
        iterations=iterations,
        warmups=warmups,
        scale=scale,
        timeout=args.timeout,
        reference=reference,
        reference_status=reference_status,
        reference_source=reference_source,
        failures=failures,
    )
    json_path = out_dir / "matrix.json"
    markdown_path = out_dir / "matrix.md"
    rendered = render_markdown(summary)
    json_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    markdown_path.write_text(rendered, encoding="utf-8")
    overhead_report = overhead_attribution.build_report(
        [
            {
                "scenario": row["scenario"],
                "row": row["row"],
                "counters": row["counters"],
            }
            for row in rows
            if isinstance(row.get("counters"), dict) and row["counters"]
        ]
    )
    overhead_json_path = out_dir / "overhead.json"
    overhead_json_path.write_text(
        json.dumps(overhead_report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    (out_dir / "overhead.md").write_text(
        overhead_attribution.render_markdown(overhead_report), encoding="utf-8"
    )
    if should_write_summary_doc(mode):
        SUMMARY_DOC.write_text(rendered, encoding="utf-8")
    if args.ratchet_out is not None:
        ratchet_path = args.ratchet_out if args.ratchet_out.is_absolute() else ROOT / args.ratchet_out
        ratchet = ratchet_from_summary(summary, "app-flow-ratchet-smoke" if args.smoke else "app-flow-ratchet")
        validation_failures = validate_report(ratchet)
        if validation_failures:
            raise SystemExit("; ".join(validation_failures))
        write_json(ratchet_path, ratchet)
        markdown_target = args.ratchet_markdown_out
        if markdown_target is None:
            markdown_target = ratchet_path.with_suffix(".md")
        markdown_path_target = markdown_target if markdown_target.is_absolute() else ROOT / markdown_target
        markdown_path_target.parent.mkdir(parents=True, exist_ok=True)
        markdown_path_target.write_text(
            render_report_markdown(ratchet, "Application-Flow Ratchet"),
            encoding="utf-8",
        )
    if failures:
        print(f"[fail] app-flow matrix wrote {rel(json_path)} with {len(failures)} failure(s)")
        for failure in failures:
            print(f"[fail] {failure}")
        return 1
    print(
        "[pass] app-flow matrix compared "
        f"{len(scenarios)} scenario(s), wrote {rel(json_path)} and "
        f"{rel(overhead_json_path)}, reference_status={reference_status}"
    )
    if not reference_available:
        print(f"[skip] reference PHP unavailable: {rel(reference)}")
    return 0


def self_test() -> int:
    scenarios = load_manifest()
    sample_summary = build_summary(
        mode="self-test",
        scenarios=scenarios[:1],
        rows=[
            {
                "scenario": scenarios[0].id,
                "row": "phrust-baseline-ir",
                "status": "pass",
                "correctness": "manifest",
                "reason": "",
                "median_ms": 1.0,
                "min_ms": 1.0,
                "max_ms": 1.0,
                "ratio_vs_reference": None,
                "counter_highlights": {"instructions_executed": 1},
                "phase_summary": {
                    "startup_external_ms": 0.5,
                    "compile_total_ms": 1.0,
                    "execute_ms": 2.0,
                },
            }
        ],
        skipped_rows=0,
        iterations=1,
        warmups=0,
        scale=1,
        timeout=30.0,
        reference=DEFAULT_REFERENCE,
        reference_status="skipped",
        reference_source="self-test",
        failures=[],
    )
    rendered = render_markdown(sample_summary)
    if "Application-Flow Performance Results" not in rendered:
        raise SystemExit("self-test markdown render failed")
    if (
        "Startup ms" not in rendered
        or "Compile ms" not in rendered
        or "Execute ms" not in rendered
    ):
        raise SystemExit("self-test phase summary render failed")
    header_line = next(line for line in rendered.splitlines() if line.startswith("| Scenario |"))
    divider_line = rendered.splitlines()[rendered.splitlines().index(header_line) + 1]
    if header_line.count("|") != divider_line.count("|"):
        raise SystemExit("self-test matrix header/divider column mismatch")
    phrust_row = Row("self-test", "phrust", ("php-vm", "run"), counters=True)
    timed_command = build_command(
        phrust_row,
        "fixture.php",
        1,
        instrumented=False,
        counters_path=Path("counters.json"),
        timings_path=Path("timings.json"),
    )
    if any(flag in " ".join(timed_command) for flag in ("--counters-json", "--timings-json")):
        raise SystemExit("self-test: timed command must be uninstrumented")
    instrumented_command = build_command(
        phrust_row,
        "fixture.php",
        1,
        instrumented=True,
        counters_path=Path("counters.json"),
        timings_path=Path("timings.json"),
    )
    if "--counters-json" not in instrumented_command or "--timings-json" not in instrumented_command:
        raise SystemExit("self-test: instrumented command must carry instrumentation flags")
    if should_write_summary_doc("smoke") or not should_write_summary_doc("full"):
        raise SystemExit("self-test: summary doc gating incorrect")
    overhead_attribution.self_test()
    overhead_report = overhead_attribution.build_report(
        [
            {
                "scenario": scenarios[0].id,
                "row": "phrust-default",
                "counters": {"value_clones": 5, "array_handle_clones": 2},
            }
        ]
    )
    if overhead_report["schema"] != overhead_attribution.OVERHEAD_SCHEMA:
        raise SystemExit("self-test: overhead report schema mismatch")
    top = overhead_report["rows"][0]["top_families"]
    if [entry["family"] for entry in top] != ["value_clones", "array_handle_clones"]:
        raise SystemExit(f"self-test: overhead top families incorrect: {top}")
    overhead_markdown = overhead_attribution.render_markdown(overhead_report)
    if "value_clones=5" not in overhead_markdown:
        raise SystemExit("self-test: overhead markdown render failed")
    print(f"[pass] app-flow matrix self-test validated {len(scenarios)} scenarios")
    return 0


def main() -> int:
    args = parse_args()
    if args.self_test:
        return self_test()
    return run_matrix(args)


if __name__ == "__main__":
    sys.exit(main())
