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

from normalize_perf_output import normalize


ROOT = Path(__file__).resolve().parents[2]
MANIFEST = ROOT / "tests/fixtures/performance/app_flows/manifest.json"
FIXTURE_DIR = MANIFEST.parent
DEFAULT_ENGINE = ROOT / "target/debug/php-vm"
DEFAULT_RELEASE_ENGINE = ROOT / "target/release/php-vm"
DEFAULT_REFERENCE = ROOT / "third_party/php-src/sapi/cli/php"
DEFAULT_OUT_DIR = ROOT / "target/performance/app-flows"
SUMMARY_DOC = ROOT / "docs/performance-app-flow-results.md"


@dataclass(frozen=True)
class Scenario:
    id: str
    name: str
    group: str
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
                "phrust-fast-preset",
                "phrust",
                (str(engine), "run", "--engine-preset=fast"),
                counters=True,
            ),
        ]
    )
    if not args.smoke:
        rows.append(
            Row(
                "phrust-release-fast",
                "phrust",
                (str(release_engine), "run", "--engine-preset=fast"),
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


def run_once(
    scenario: Scenario,
    row: Row,
    out_dir: Path,
    iteration: int,
    scale: int,
    timeout: float,
) -> RunSample:
    run_dir = out_dir / "runs" / safe_name(scenario.id) / safe_name(row.label)
    run_dir.mkdir(parents=True, exist_ok=True)
    command = [*row.command_prefix]
    counters_path = run_dir / f"iter-{iteration}.counters.json"
    if row.kind == "phrust":
        command.extend(["--env", f"PHRUST_APP_FLOW_SCALE={scale}"])
    if row.counters:
        command.extend(["--counters-json", str(counters_path)])
    command.append(rel(scenario.fixture))
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
    (run_dir / f"iter-{iteration}.stdout").write_text(stdout, encoding="utf-8")
    (run_dir / f"iter-{iteration}.stderr").write_text(stderr, encoding="utf-8")
    (run_dir / f"iter-{iteration}.status").write_text(f"{returncode}\n", encoding="utf-8")
    (run_dir / f"iter-{iteration}.command.json").write_text(
        json.dumps(command, indent=2) + "\n", encoding="utf-8"
    )
    counters: dict[str, Any] = {}
    if counters_path.is_file():
        counters = json.loads(counters_path.read_text(encoding="utf-8"))
    if not isinstance(counters, dict):
        raise SystemExit(f"{rel(counters_path)}: counters root is not an object")
    return RunSample(
        elapsed_ms=elapsed_ms,
        returncode=returncode,
        stdout=stdout,
        stderr=stderr,
        command=command,
        counters=counters,
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
) -> list[RunSample]:
    for warmup in range(warmups):
        run_once(scenario, row, out_dir, -(warmup + 1), scale, timeout)
    return [
        run_once(scenario, row, out_dir, iteration, scale, timeout)
        for iteration in range(iterations)
    ]


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
        "Generated by `nix develop -c just app-flow-smoke` or",
        "`nix develop -c just app-flow-matrix`.",
        "Raw stdout, stderr, command, status, and counter artifacts are local-only",
        "under `target/performance/app-flows/` and must not be committed.",
        "",
        "Correctness is mandatory. When reference PHP is available, Phrust rows",
        "must match reference stdout, normalized stderr, and exit status before",
        "timing data is reported. Wall-clock timings are advisory host-local data.",
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
        "| Scenario | Row | Correctness | Median ms | Ratio vs ref | Counter highlights | Skip/failure reason |",
        "| --- | --- | --- | --- | --- | --- | --- |",
    ]
    for row in summary["rows"]:
        if row["status"] == "skip":
            lines.append(
                f"| `{row['scenario']}` | `{row['row']}` | skip | n/a | n/a | n/a | {row['reason']} |"
            )
            continue
        counters = row.get("counter_highlights", {})
        focus = ", ".join(f"{key}={value}" for key, value in counters.items())
        ratio = row["ratio_vs_reference"]
        ratio_text = "n/a" if ratio is None else f"{ratio:.3f}"
        reason = row.get("reason", "")
        lines.append(
            f"| `{row['scenario']}` | `{row['row']}` | `{row['correctness']}` | "
            f"{row['median_ms']:.3f} | {ratio_text} | {focus or 'n/a'} | {reason or 'n/a'} |"
        )
    if summary["failures"]:
        lines.extend(["", "## Failures", ""])
        for failure in summary["failures"]:
            lines.append(f"- {failure}")
    return "\n".join(lines) + "\n"


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
                    samples = run_samples(
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
                samples = run_samples(
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
                        }
                        for item in samples
                    ],
                    "counters": samples[-1].counters,
                    "counter_highlights": counter_focus(samples[-1].counters),
                }
            )

        if reference_sample is None and not reference_available:
            baseline = scenario_results.get("phrust-baseline-ir")
            fast = scenario_results.get("phrust-fast-preset")
            if baseline and fast:
                correctness, reason = compare_sample(
                    scenario, matrix_rows[1], fast[0], baseline[0], scale
                )
                if correctness == "fail":
                    failures.append(f"{scenario.id} phrust-fast-preset: {reason}")

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
    SUMMARY_DOC.write_text(rendered, encoding="utf-8")
    if failures:
        print(f"[fail] app-flow matrix wrote {rel(json_path)} with {len(failures)} failure(s)")
        for failure in failures:
            print(f"[fail] {failure}")
        return 1
    print(
        "[pass] app-flow matrix compared "
        f"{len(scenarios)} scenario(s), wrote {rel(json_path)}, "
        f"reference_status={reference_status}"
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
    print(f"[pass] app-flow matrix self-test validated {len(scenarios)} scenarios")
    return 0


def main() -> int:
    args = parse_args()
    if args.self_test:
        return self_test()
    return run_matrix(args)


if __name__ == "__main__":
    sys.exit(main())
