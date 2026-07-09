#!/usr/bin/env python3
"""Generate a correctness-first fastest-engine comparison matrix."""

from __future__ import annotations

import argparse
import json
import os
import statistics
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from normalize_perf_output import normalize


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_DEBUG_ENGINE = ROOT / "target/debug/php-vm"
DEFAULT_RELEASE_ENGINE = ROOT / "target/release/php-vm"
DEFAULT_PGO_ENGINE = ROOT / "target/pgo/php-vm"
DEFAULT_OUT_DIR = ROOT / "target/performance/fastest"
DEFAULT_SUMMARY_DOC = ROOT / "target/performance/fastest/matrix.md"
DEFAULT_FIXTURES: tuple[tuple[str, Path], ...] = (
    ("perf-smoke", ROOT / "tests/fixtures/performance/perf_smoke/arithmetic.php"),
    ("arrays-foreach", ROOT / "tests/fixtures/performance/perf_smoke/array_fast_paths_v2.php"),
    ("arrays-foreach", ROOT / "tests/fixtures/performance/perf_smoke/arrays_packed.php"),
    ("strings-output-template", ROOT / "tests/fixtures/performance/perf_smoke/strings_concat.php"),
    ("strings-output-template", ROOT / "tests/fixtures/performance/perf_smoke/output_batching_v2.php"),
    ("strings-output-template", ROOT / "tests/fixtures/performance/framework_smoke/template_output.php"),
    ("calls-builtins", ROOT / "tests/fixtures/performance/perf_smoke/function_calls.php"),
    ("calls-builtins", ROOT / "tests/fixtures/performance/perf_smoke/stdlib_dispatch.php"),
    ("properties-methods", ROOT / "tests/fixtures/performance/perf_smoke/properties.php"),
    ("properties-methods", ROOT / "tests/fixtures/performance/perf_smoke/method_calls.php"),
    ("framework-smoke", ROOT / "tests/fixtures/performance/framework_smoke/router_dispatch.php"),
    ("framework-smoke", ROOT / "tests/fixtures/performance/framework_smoke/object_property_method_loop.php"),
)
COUNTER_KEYS = (
    "instructions_executed",
    "bytecode_instructions_executed",
    "quickening_specialized",
    "inline_cache_hits",
    "method_call_ic_hits",
    "property_fetch_ic_hits",
    "property_assign_ic_hits",
    "output_fast_appends",
    "output_batched_appends",
    "string_concat_fast_path_hits",
    "packed_fetch_fast_hits",
    "packed_append_fast_hits",
    "packed_foreach_fast_hits",
    "jit_compile_attempts",
    "jit_compile_successes",
    "jit_side_exits",
    "cow_separations",
    "reference_cell_creations",
    "object_allocations",
)


@dataclass(frozen=True)
class Fixture:
    category: str
    path: Path


@dataclass(frozen=True)
class MatrixRow:
    label: str
    kind: str
    engine: Path | None = None
    run_flags: tuple[str, ...] = ()
    reference_flags: tuple[str, ...] = ()
    compile_opt_level: int | None = None
    optional: bool = False
    require_include_jit: bool = False
    require_persistent_feedback: bool = False
    require_opcache: bool = False
    collect_counters: bool = True


@dataclass(frozen=True)
class ProcessSample:
    elapsed_ms: float
    returncode: int
    stdout: str
    stderr: str
    counters: dict[str, Any]
    command: list[str]


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
    parser.add_argument("--engine", type=Path, default=DEFAULT_DEBUG_ENGINE)
    parser.add_argument("--release-engine", type=Path, default=DEFAULT_RELEASE_ENGINE)
    parser.add_argument("--pgo-engine", type=Path, default=DEFAULT_PGO_ENGINE)
    parser.add_argument("--reference-php", type=Path, default=None)
    parser.add_argument("--out-dir", type=Path, default=DEFAULT_OUT_DIR)
    parser.add_argument("--summary-doc", type=Path, default=DEFAULT_SUMMARY_DOC)
    parser.add_argument("--fixture", action="append", type=Path, default=[])
    parser.add_argument("--iterations", type=positive_int, default=1)
    parser.add_argument("--warmups", type=positive_int, default=0)
    parser.add_argument("--timeout", type=float, default=10.0)
    parser.add_argument(
        "--include-jit",
        action="store_true",
        default=os.getenv("PHRUST_FASTEST_MATRIX_JIT") == "1",
        help="Include the optional Cranelift row when the local binary supports it.",
    )
    parser.add_argument(
        "--include-persistent-feedback",
        action="store_true",
        default=os.getenv("PHRUST_FASTEST_MATRIX_PERSISTENT_FEEDBACK") == "1",
        help="Include the optional default-off persistent-feedback advisory row.",
    )
    return parser.parse_args()


def absolute(path: Path) -> Path:
    return path if path.is_absolute() else ROOT / path


def reference_php_path(explicit: Path | None) -> tuple[Path | None, str | None]:
    candidate = explicit
    if candidate is None and os.getenv("REFERENCE_PHP"):
        candidate = Path(os.environ["REFERENCE_PHP"])
    if candidate is None:
        default = ROOT / "third_party/php-src/sapi/cli/php"
        if default.is_file() and os.access(default, os.X_OK):
            candidate = default
    if candidate is None:
        return None, "REFERENCE_PHP not set and third_party/php-src/sapi/cli/php is unavailable"
    candidate = absolute(candidate)
    if candidate.is_file() and os.access(candidate, os.X_OK):
        return candidate, None
    if explicit is not None or os.getenv("REFERENCE_PHP"):
        raise SystemExit(f"reference PHP is not executable: {candidate}")
    return None, f"reference PHP is not executable: {rel(candidate)}"


def resolved_fixtures(extra: list[Path]) -> list[Fixture]:
    fixtures = [Fixture(category, path) for category, path in DEFAULT_FIXTURES]
    fixtures.extend(Fixture("extra", absolute(path)) for path in extra)
    resolved: list[Fixture] = []
    seen: set[Path] = set()
    for fixture in fixtures:
        path = absolute(fixture.path).resolve()
        if path in seen:
            continue
        if not path.is_file():
            raise SystemExit(f"missing fastest-engine matrix fixture: {path}")
        seen.add(path)
        resolved.append(Fixture(fixture.category, path))
    return resolved


def opcache_supported(reference_php: Path | None, timeout: float) -> tuple[bool, str | None]:
    if reference_php is None:
        return False, "reference PHP unavailable"
    command = [
        str(reference_php),
        "-d",
        "opcache.enable_cli=1",
        "-d",
        "opcache.jit=0",
        "-d",
        "opcache.validate_timestamps=1",
        "-d",
        "opcache.revalidate_freq=0",
        "-r",
        'echo extension_loaded("Zend OPcache") ? ini_get("opcache.enable_cli") : "missing";',
    ]
    completed = subprocess.run(
        command,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
        check=False,
    )
    if completed.returncode != 0:
        return False, normalize(completed.stderr).strip() or "opcache probe failed"
    if completed.stdout.strip() == "1":
        return True, None
    return False, "reference PHP does not expose safe CLI Zend OPcache"


def rows(args: argparse.Namespace, reference_php: Path | None, opcache_ok: bool) -> list[MatrixRow]:
    debug_engine = absolute(args.engine)
    release_engine = absolute(args.release_engine)
    pgo_engine = absolute(args.pgo_engine)
    baseline_flags = (
        "--exec-format=ir",
        "--opt-level=0",
        "--superinstructions=off",
        "--quickening=off",
        "--inline-caches=off",
        "--bytecode-cache=off",
        "--jit=off",
        "--tiering=off",
    )
    fast_flags = ("--engine-preset=fast",)
    rows_: list[MatrixRow] = [
        MatrixRow(
            "phrust-baseline-ir",
            "phrust",
            engine=debug_engine,
            run_flags=baseline_flags,
            compile_opt_level=0,
        ),
        MatrixRow(
            "phrust-fast-preset",
            "phrust",
            engine=debug_engine,
            run_flags=fast_flags,
            compile_opt_level=2,
        ),
        MatrixRow(
            "phrust-release-fast",
            "phrust",
            engine=release_engine,
            run_flags=fast_flags,
            compile_opt_level=2,
            optional=True,
        ),
        MatrixRow(
            "phrust-release-pgo",
            "phrust",
            engine=pgo_engine,
            run_flags=fast_flags,
            compile_opt_level=2,
            optional=True,
        ),
        MatrixRow(
            "phrust-persistent-feedback-optional",
            "phrust",
            engine=debug_engine,
            run_flags=fast_flags,
            compile_opt_level=2,
            optional=True,
            require_persistent_feedback=True,
        ),
        MatrixRow(
            "phrust-cranelift-optional",
            "phrust",
            engine=debug_engine,
            run_flags=(
                "--exec-format=auto",
                "--opt-level=2",
                "--quickening=on",
                "--inline-caches=on",
                "--jit=cranelift",
            ),
            compile_opt_level=2,
            optional=True,
            require_include_jit=True,
        ),
        MatrixRow(
            "reference-php-cli",
            "reference-php",
            engine=reference_php,
            collect_counters=False,
            optional=True,
        ),
        MatrixRow(
            "reference-php-cli-opcache",
            "reference-php",
            engine=reference_php if opcache_ok else None,
            reference_flags=(
                "-d",
                "opcache.enable_cli=1",
                "-d",
                "opcache.jit=0",
                "-d",
                "opcache.validate_timestamps=1",
                "-d",
                "opcache.revalidate_freq=0",
            ),
            collect_counters=False,
            optional=True,
            require_opcache=True,
        ),
    ]
    return rows_


def normalized_env(tmp_dir: Path) -> dict[str, str]:
    env = dict(os.environ)
    env.update(
        {
            "TZ": "UTC",
            "LC_ALL": "C",
            "LANG": "C",
            "TMPDIR": str(tmp_dir),
            "TMP": str(tmp_dir),
            "TEMP": str(tmp_dir),
            "PHRUST_RANDOM_SEED": "fastest-engine-matrix",
            "RUST_TEST_SEED": "fastest-engine-matrix",
        }
    )
    return env


def command_for(
    row: MatrixRow,
    fixture: Path,
    counters_path: Path | None,
    run_dir: Path | None = None,
    label: str = "0",
) -> list[str]:
    if row.kind == "phrust":
        counters_args: list[str] = []
        if counters_path is not None:
            counters_args = ["--counters-json", str(counters_path)]
        feedback_args: list[str] = []
        if row.require_persistent_feedback and run_dir is not None:
            # Pin the consumption policy so the row keeps measuring seeded
            # execution even if the engine default changes.
            feedback_args = [
                "--persistent-feedback-read",
                str(run_dir / "advisory-feedback.pff"),
                "--persistent-feedback-consume=quickening",
                "--persistent-feedback-stats-json",
                str(run_dir / f"iter-{label}.persistent-feedback.json"),
            ]
        return [
            str(row.engine),
            "run",
            *row.run_flags,
            *feedback_args,
            *counters_args,
            rel(fixture),
        ]
    return [str(row.engine), *row.reference_flags, rel(fixture)]


def display_command(command: list[str]) -> list[str]:
    displayed: list[str] = []
    for part in command:
        path = Path(part)
        if path.is_absolute():
            displayed.append(rel(path))
        else:
            displayed.append(part)
    return displayed


def run_process(
    row: MatrixRow,
    fixture: Fixture,
    out_dir: Path,
    label: str,
    timeout: float,
    *,
    instrumented: bool = False,
) -> ProcessSample:
    stem = rel(fixture.path).replace("/", "__")
    run_dir = out_dir / "runs" / stem / row.label
    run_dir.mkdir(parents=True, exist_ok=True)
    # Timed iterations stay uninstrumented; counters come from one dedicated
    # instrumented run so counter collection cannot inflate wall times.
    counters_path = (
        run_dir / f"iter-{label}.counters.json"
        if instrumented and row.collect_counters
        else None
    )
    if counters_path is not None:
        counters_path.unlink(missing_ok=True)
    command = command_for(row, fixture.path, counters_path, run_dir, label)
    started = time.perf_counter_ns()
    completed = subprocess.run(
        command,
        cwd=ROOT,
        env=normalized_env(out_dir / "tmp" / stem / row.label),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
        check=False,
    )
    elapsed_ms = (time.perf_counter_ns() - started) / 1_000_000.0
    stdout = completed.stdout.replace("\r\n", "\n").replace("\r", "\n")
    stderr = normalize(completed.stderr)
    (run_dir / f"iter-{label}.stdout").write_text(stdout, encoding="utf-8")
    (run_dir / f"iter-{label}.stderr").write_text(stderr, encoding="utf-8")
    counters: dict[str, Any] = {}
    if counters_path is not None and counters_path.is_file():
        data = json.loads(counters_path.read_text(encoding="utf-8"))
        if not isinstance(data, dict):
            raise SystemExit(f"{rel(counters_path)}: counters root is not an object")
        for key, value in data.items():
            if isinstance(value, int) and value < 0:
                raise SystemExit(f"{rel(counters_path)}: counter {key} is negative")
        counters = data
    return ProcessSample(
        elapsed_ms=elapsed_ms,
        returncode=completed.returncode,
        stdout=stdout,
        stderr=stderr,
        counters=counters,
        command=display_command(command),
    )


def run_samples(
    row: MatrixRow,
    fixture: Fixture,
    out_dir: Path,
    warmups: int,
    iterations: int,
    timeout: float,
) -> tuple[list[ProcessSample], ProcessSample | None]:
    """Run warmups, clean timed iterations, and one instrumented sample.

    Returns `(timed_samples, instrumented_sample)`; the instrumented sample
    (phrust counter rows only) supplies counters without polluting timings.
    """
    for warmup in range(warmups):
        run_process(row, fixture, out_dir, f"-{warmup + 1}", timeout)
    timed = [
        run_process(row, fixture, out_dir, str(iteration), timeout)
        for iteration in range(iterations)
    ]
    instrumented: ProcessSample | None = None
    if row.kind == "phrust" and row.collect_counters:
        instrumented = run_process(
            row, fixture, out_dir, "instrumented", timeout, instrumented=True
        )
    return timed, instrumented


def measure_compile_ms(
    row: MatrixRow,
    fixture: Fixture,
    out_dir: Path,
    timeout: float,
) -> float | None:
    if row.kind != "phrust" or row.compile_opt_level is None or row.engine is None:
        return None
    run_dir = out_dir / "compile" / rel(fixture.path).replace("/", "__") / row.label
    run_dir.mkdir(parents=True, exist_ok=True)
    command = [
        str(row.engine),
        "compile",
        rel(fixture.path),
        "--json",
        "--opt-level",
        str(row.compile_opt_level),
    ]
    started = time.perf_counter_ns()
    completed = subprocess.run(
        command,
        cwd=ROOT,
        env=normalized_env(run_dir),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
        check=False,
    )
    elapsed_ms = (time.perf_counter_ns() - started) / 1_000_000.0
    (run_dir / "compile.stdout").write_text(completed.stdout, encoding="utf-8")
    (run_dir / "compile.stderr").write_text(normalize(completed.stderr), encoding="utf-8")
    if completed.returncode != 0:
        raise SystemExit(
            f"[fail] compile split failed for {row.label} {rel(fixture.path)}"
        )
    return elapsed_ms


def median_ms(samples: list[ProcessSample]) -> float:
    return statistics.median(sample.elapsed_ms for sample in samples)


def counter_focus(counters: dict[str, Any]) -> dict[str, int]:
    return {
        key: value
        for key in COUNTER_KEYS
        if isinstance((value := counters.get(key)), int) and value != 0
    }


def behavior_differences(baseline: ProcessSample, sample: ProcessSample) -> list[str]:
    differences: list[str] = []
    if sample.returncode != baseline.returncode:
        differences.append(
            f"exit baseline={baseline.returncode} row={sample.returncode}"
        )
    if sample.stdout != baseline.stdout:
        differences.append("stdout differs")
    if sample.stderr != baseline.stderr:
        differences.append("stderr differs")
    return differences


def skip_reason(
    row: MatrixRow,
    include_jit: bool,
    include_persistent_feedback: bool,
    reference_skip: str | None,
    opcache_skip: str | None,
) -> str | None:
    if row.require_include_jit and not include_jit:
        return "Cranelift row not requested; set PHRUST_FASTEST_MATRIX_JIT=1 or --include-jit"
    if row.require_persistent_feedback and not include_persistent_feedback:
        return "persistent feedback row not requested; set PHRUST_FASTEST_MATRIX_PERSISTENT_FEEDBACK=1 or --include-persistent-feedback"
    if row.kind == "reference-php" and row.engine is None:
        return reference_skip or "reference PHP unavailable"
    if row.require_opcache and not row.engine:
        return opcache_skip or "reference PHP CLI opcache unavailable"
    if row.engine is None:
        return "engine unavailable"
    if not row.engine.is_file() or not os.access(row.engine, os.X_OK):
        if row.optional:
            return f"engine unavailable: {rel(row.engine)}"
        raise SystemExit(f"engine is not executable: {row.engine}")
    return None


def row_summary(
    row: MatrixRow,
    fixture: Fixture,
    status: str,
    reason: str,
) -> dict[str, Any]:
    return {
        "variant": row.label,
        "kind": row.kind,
        "fixture": rel(fixture.path),
        "category": fixture.category,
        "status": status,
        "reason": reason,
        "flags": list(row.run_flags if row.kind == "phrust" else row.reference_flags),
    }


def render_matrix_markdown(summary: dict[str, Any]) -> str:
    lines = [
        "# Fastest Engine Matrix",
        "",
        "Generated by `nix develop -c just fastest-engine-matrix`.",
        "Raw stdout, stderr, counter, and timing evidence stays under",
        "`target/performance/fastest/` and must not be committed.",
        "",
        "Correctness is checked before timing. Phrust rows must match",
        "`phrust-baseline-ir` for stdout, stderr/runtime diagnostics, and exit",
        "status. Reference PHP rows are reported separately so compatibility gaps",
        "are visible instead of becoming speed claims.",
        "",
        "## Summary",
        "",
        "| Field | Value |",
        "| --- | --- |",
        f"| Status | `{summary['status']}` |",
        f"| Fixtures | {summary['fixture_count']} |",
        f"| Enabled rows | {summary['enabled_row_count']} |",
        f"| Skipped rows | {summary['skipped_row_count']} |",
        f"| Known-gap rows | {summary['known_gap_row_count']} |",
        f"| Warmups | {summary['warmups']} |",
        f"| Iterations | {summary['iterations']} |",
        "",
        "## Rows",
        "",
        "| Row | Category | Fixture | Correctness | Compile ms | Execute ms | Total ms | Counters / reason |",
        "| --- | --- | --- | --- | ---: | ---: | ---: | --- |",
    ]
    for row in summary["rows"]:
        status = row["status"]
        fixture = row["fixture"]
        category = row["category"]
        variant = row["variant"]
        if status == "skip":
            lines.append(
                f"| `{variant}` | `{category}` | `{fixture}` | skip | n/a | n/a | n/a | {row['reason']} |"
            )
            continue
        if status == "known_gap":
            lines.append(
                f"| `{variant}` | `{category}` | `{fixture}` | known gap | "
                f"n/a | {row['execution_median_ms']:.3f} | {row['total_median_ms']:.3f} | "
                f"{'; '.join(row['differences'])} |"
            )
            continue
        counters = ", ".join(f"{key}={value}" for key, value in row["counter_focus"].items())
        compile_ms = row.get("compile_ms")
        compile_cell = "n/a" if compile_ms is None else f"{compile_ms:.3f}"
        lines.append(
            f"| `{variant}` | `{category}` | `{fixture}` | `{row['correctness']}` | "
            f"{compile_cell} | {row['execution_median_ms']:.3f} | "
            f"{row['total_median_ms']:.3f} | {counters or 'n/a'} |"
        )
    return "\n".join(lines) + "\n"


def render_summary_doc(summary: dict[str, Any]) -> str:
    enabled_variants = sorted(
        {
            row["variant"]
            for row in summary["rows"]
            if row.get("status") in {"pass", "known_gap"}
        }
    )
    skipped = [row for row in summary["rows"] if row.get("status") == "skip"]
    known_gaps = [row for row in summary["rows"] if row.get("status") == "known_gap"]
    lines = [
        "# Fastest Engine Results",
        "",
        "This is a committed summary of the local fastest-engine matrix. The raw",
        "JSON, Markdown, stdout/stderr captures, and counter files are generated",
        "under `target/performance/fastest/` and are not committed.",
        "",
        "The matrix is correctness-first. It does not claim that Phrust is the",
        "globally fastest PHP engine; timings are advisory host-local samples over",
        "a bounded fixture corpus.",
        "",
        "## Latest Matrix",
        "",
        "| Field | Value |",
        "| --- | --- |",
        f"| Status | `{summary['status']}` |",
        f"| Fixtures | {summary['fixture_count']} |",
        f"| Enabled rows | {summary['enabled_row_count']} |",
        f"| Skipped rows | {summary['skipped_row_count']} |",
        f"| Known-gap rows | {summary['known_gap_row_count']} |",
        f"| Iterations | {summary['iterations']} |",
        f"| Warmups | {summary['warmups']} |",
        "",
        "## Compared Rows",
        "",
    ]
    for variant in enabled_variants:
        lines.append(f"- `{variant}`")
    if skipped:
        lines.extend(["", "## Explicit Skips", ""])
        skip_pairs = sorted({(row["variant"], row["reason"]) for row in skipped})
        for variant, reason in skip_pairs:
            lines.append(f"- `{variant}`: {reason}")
    if known_gaps:
        lines.extend(["", "## Reference Compatibility Gaps", ""])
        for row in known_gaps:
            lines.append(
                f"- `{row['variant']}` on `{row['fixture']}`: "
                f"{'; '.join(row['differences'])}"
            )
    lines.extend(
        [
            "",
            "## Artifacts",
            "",
            "- `target/performance/fastest/matrix.json`",
            "- `target/performance/fastest/matrix.md`",
            "- `target/performance/fastest/runs/`",
            "",
            "## Policy",
            "",
            "- Phrust rows fail if PHP-visible stdout, stderr/runtime diagnostics, or exit status diverge from `phrust-baseline-ir`.",
            "- Reference PHP rows skip cleanly when no local reference binary is available.",
            "- CLI opcache is only reported when the local reference binary accepts the recorded safe INI flags.",
            "- Compile, execution, and total timing fields are separated where Phrust exposes a compile-only command.",
        ]
    )
    return "\n".join(lines) + "\n"


def main() -> int:
    args = parse_args()
    out_dir = absolute(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    fixtures = resolved_fixtures(args.fixture)
    reference_php, reference_skip = reference_php_path(args.reference_php)
    opcache_ok, opcache_skip = opcache_supported(reference_php, args.timeout)
    matrix_rows = rows(args, reference_php, opcache_ok)
    baseline = matrix_rows[0]
    if baseline.engine is None or not baseline.engine.is_file() or not os.access(baseline.engine, os.X_OK):
        raise SystemExit(f"baseline engine is not executable: {baseline.engine}")

    output_rows: list[dict[str, Any]] = []
    enabled = 0
    skipped = 0
    known_gaps = 0
    for fixture in fixtures:
        baseline_samples, baseline_instrumented = run_samples(
            baseline,
            fixture,
            out_dir,
            args.warmups,
            max(args.iterations, 1),
            args.timeout,
        )
        baseline_sample = baseline_samples[0]
        if baseline_instrumented is not None:
            # Counter collection must not change baseline behavior either;
            # its output feeds the published counter_focus.
            differences = sorted(set(behavior_differences(baseline_sample, baseline_instrumented)))
            if differences:
                raise SystemExit(
                    "[fail] fastest-engine matrix counter collection changed baseline "
                    f"behavior for {rel(fixture.path)}: " + "; ".join(differences)
                )
        baseline_compile_ms = measure_compile_ms(baseline, fixture, out_dir, args.timeout)
        output_rows.append(
            {
                "variant": baseline.label,
                "kind": baseline.kind,
                "fixture": rel(fixture.path),
                "category": fixture.category,
                "status": "pass",
                "correctness": "baseline",
                "compile_ms": baseline_compile_ms,
                "execution_median_ms": median_ms(baseline_samples),
                "total_median_ms": (baseline_compile_ms or 0.0) + median_ms(baseline_samples),
                "counter_focus": counter_focus(
                    baseline_instrumented.counters
                    if baseline_instrumented is not None
                    else {}
                ),
                "flags": list(baseline.run_flags),
                "engine": rel(baseline.engine),
                "command": baseline_samples[-1].command,
            }
        )
        enabled += 1
        for row in matrix_rows[1:]:
            reason = skip_reason(
                row,
                args.include_jit,
                args.include_persistent_feedback,
                reference_skip,
                opcache_skip,
            )
            if reason:
                output_rows.append(row_summary(row, fixture, "skip", reason))
                skipped += 1
                continue
            samples, instrumented = run_samples(
                row,
                fixture,
                out_dir,
                args.warmups,
                max(args.iterations, 1),
                args.timeout,
            )
            differences: list[str] = []
            for sample in samples:
                differences.extend(behavior_differences(baseline_sample, sample))
            if instrumented is not None:
                # Counter collection must not change PHP-visible behavior.
                differences.extend(behavior_differences(baseline_sample, instrumented))
            differences = sorted(set(differences))
            if differences and row.kind == "phrust":
                raise SystemExit(
                    "[fail] fastest-engine matrix changed behavior for "
                    f"{rel(fixture.path)} under {row.label}: " + "; ".join(differences)
                )
            compile_ms = measure_compile_ms(row, fixture, out_dir, args.timeout)
            execution_ms = median_ms(samples)
            status = "known_gap" if differences else "pass"
            if status == "known_gap":
                known_gaps += 1
            enabled += 1
            output_rows.append(
                {
                    "variant": row.label,
                    "kind": row.kind,
                    "fixture": rel(fixture.path),
                    "category": fixture.category,
                    "status": status,
                    "correctness": "pass" if not differences else "reference-diff",
                    "differences": differences,
                    "compile_ms": compile_ms,
                    "execution_median_ms": execution_ms,
                    "total_median_ms": (compile_ms or 0.0) + execution_ms,
                    "counter_focus": counter_focus(
                        instrumented.counters if instrumented is not None else {}
                    ),
                    "flags": list(row.run_flags if row.kind == "phrust" else row.reference_flags),
                    "engine": rel(row.engine) if row.engine is not None else None,
                    "command": samples[-1].command,
                }
            )

    status = "pass" if known_gaps == 0 else "pass_with_known_gaps"
    summary: dict[str, Any] = {
        "status": status,
        "gate": "fastest-engine-matrix",
        "timing_policy": "advisory-host-local",
        "correctness_policy": "phrust rows must match baseline; reference rows report explicit compatibility gaps",
        "fixtures": [
            {"category": fixture.category, "path": rel(fixture.path)}
            for fixture in fixtures
        ],
        "fixture_count": len(fixtures),
        "iterations": max(args.iterations, 1),
        "warmups": args.warmups,
        "enabled_row_count": enabled,
        "skipped_row_count": skipped,
        "known_gap_row_count": known_gaps,
        "include_jit": args.include_jit,
        "include_persistent_feedback": args.include_persistent_feedback,
        "reference_php": rel(reference_php) if reference_php else None,
        "reference_php_skip_reason": reference_skip,
        "reference_php_opcache_supported": opcache_ok,
        "reference_php_opcache_skip_reason": opcache_skip,
        "rows": output_rows,
    }
    json_path = out_dir / "matrix.json"
    markdown_path = out_dir / "matrix.md"
    json_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    markdown_path.write_text(render_matrix_markdown(summary), encoding="utf-8")
    summary_doc = absolute(args.summary_doc)
    summary_doc.write_text(render_summary_doc(summary), encoding="utf-8")
    print(
        "[pass] fastest-engine matrix compared "
        f"{enabled} enabled row(s), skipped {skipped} optional row(s), "
        f"known-gap rows {known_gaps}, and wrote {rel(json_path)}"
    )
    if not args.include_jit:
        print("[skip] fastest-engine matrix Cranelift row: feature/platform not requested")
    if not args.include_persistent_feedback:
        print("[skip] fastest-engine matrix persistent-feedback row: default-off policy")
    if reference_skip:
        print(f"[skip] fastest-engine matrix reference PHP: {reference_skip}")
    if opcache_skip:
        print(f"[skip] fastest-engine matrix reference PHP opcache: {opcache_skip}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
