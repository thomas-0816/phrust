#!/usr/bin/env python3
"""Run a correctness-first acceleration matrix and emit local reports."""

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
DEFAULT_ENGINE = ROOT / "target/debug/php-vm"
DEFAULT_RELEASE_ENGINE = ROOT / "target/release/php-vm"
DEFAULT_OUT_DIR = ROOT / "target/performance/acceleration"
DEFAULT_FIXTURES = (
    ROOT / "fixtures/runtime/valid/hello.php",
    ROOT / "fixtures/runtime/valid/scalars/echo.php",
    ROOT / "fixtures/runtime/valid/functions/simple.php",
    ROOT / "tests/fixtures/performance/perf_smoke/array_fast_paths_v2.php",
    ROOT / "tests/fixtures/performance/perf_smoke/arrays_packed.php",
    ROOT / "tests/fixtures/performance/perf_smoke/output_batching_v2.php",
    ROOT / "tests/fixtures/performance/perf_smoke/output_scalar_fast_paths.php",
    ROOT / "tests/fixtures/performance/perf_smoke/strings_concat.php",
    ROOT / "tests/fixtures/performance/framework_smoke/router_dispatch.php",
    ROOT / "tests/fixtures/performance/framework_smoke/template_output.php",
    ROOT / "tests/fixtures/performance/framework_smoke/packed_mixed_array_traversal.php",
    ROOT / "tests/fixtures/performance/inline_cache/method-call-guards.php",
    ROOT / "tests/fixtures/performance/inline_cache/property-assign-guards.php",
)
BYTECODE_STRICT_FIXTURES = {
    "fixtures/runtime/valid/hello.php",
    "fixtures/runtime/valid/scalars/echo.php",
    "fixtures/runtime/valid/functions/simple.php",
    "tests/fixtures/performance/perf_smoke/arrays_packed.php",
    "tests/fixtures/performance/framework_smoke/packed_mixed_array_traversal.php",
}


@dataclass(frozen=True)
class Variant:
    label: str
    engine: Path
    flags: tuple[str, ...]
    optional: bool = False
    strict_bytecode_subset: bool = False
    persistent_feedback: bool = False


@dataclass(frozen=True)
class RunSample:
    elapsed_ms: float
    returncode: int
    stdout: str
    stderr: str
    counters: dict[str, Any]


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
    parser.add_argument("--engine", type=Path, default=DEFAULT_ENGINE)
    parser.add_argument("--release-engine", type=Path, default=DEFAULT_RELEASE_ENGINE)
    parser.add_argument("--out-dir", type=Path, default=DEFAULT_OUT_DIR)
    parser.add_argument("--fixture", action="append", type=Path, default=[])
    parser.add_argument("--iterations", type=positive_int, default=1)
    parser.add_argument("--warmups", type=positive_int, default=0)
    parser.add_argument("--timeout", type=float, default=10.0)
    parser.add_argument(
        "--include-jit",
        action="store_true",
        default=os.getenv("PHRUST_ACCEL_MATRIX_JIT") == "1",
        help="Include the optional feature-gated Cranelift row.",
    )
    parser.add_argument(
        "--include-persistent-feedback",
        action="store_true",
        default=os.getenv("PHRUST_ACCEL_MATRIX_PERSISTENT_FEEDBACK") == "1",
        help="Include the optional default-off persistent-feedback advisory row.",
    )
    return parser.parse_args()


def resolved_fixtures(extra: list[Path]) -> list[Path]:
    fixtures = list(DEFAULT_FIXTURES)
    fixtures.extend(extra)
    resolved: list[Path] = []
    seen: set[Path] = set()
    for fixture in fixtures:
        path = fixture if fixture.is_absolute() else ROOT / fixture
        path = path.resolve()
        if path in seen:
            continue
        if not path.is_file():
            raise SystemExit(f"missing acceleration matrix fixture: {path}")
        seen.add(path)
        resolved.append(path)
    return resolved


def variants(args: argparse.Namespace) -> tuple[Variant, list[Variant]]:
    engine = args.engine if args.engine.is_absolute() else ROOT / args.engine
    release_engine = (
        args.release_engine
        if args.release_engine.is_absolute()
        else ROOT / args.release_engine
    )
    common_off = (
        "--opt-level=0",
        "--superinstructions=off",
        "--quickening=off",
        "--inline-caches=off",
        "--bytecode-cache=off",
        "--jit=off",
        "--tiering=off",
    )
    baseline = Variant("baseline-ir", engine, ("--exec-format=ir", *common_off))
    rows = [
        Variant("dense-bytecode-auto", engine, ("--exec-format=auto", *common_off)),
        Variant(
            "dense-bytecode-strict",
            engine,
            ("--exec-format=bytecode", *common_off),
            strict_bytecode_subset=True,
        ),
        Variant(
            "superinstructions-on",
            engine,
            (
                "--exec-format=bytecode",
                "--opt-level=0",
                "--superinstructions=on",
                "--quickening=off",
                "--inline-caches=off",
                "--bytecode-cache=off",
                "--jit=off",
                "--tiering=off",
            ),
            strict_bytecode_subset=True,
        ),
        Variant("optimizer-level-1", engine, ("--exec-format=ir", "--opt-level=1", *common_off[1:])),
        Variant("optimizer-level-2", engine, ("--exec-format=ir", "--opt-level=2", *common_off[1:])),
        Variant(
            "quickening-on",
            engine,
            (
                "--exec-format=auto",
                "--opt-level=0",
                "--superinstructions=off",
                "--quickening=on",
                "--inline-caches=off",
                "--bytecode-cache=off",
                "--jit=off",
            ),
        ),
        Variant(
            "inline-caches-on",
            engine,
            (
                "--exec-format=ir",
                "--opt-level=0",
                "--superinstructions=off",
                "--quickening=off",
                "--inline-caches=on",
                "--bytecode-cache=off",
                "--jit=off",
            ),
        ),
        Variant(
            "all-non-jit",
            engine,
            (
                "--exec-format=auto",
                "--opt-level=2",
                "--superinstructions=off",
                "--quickening=on",
                "--inline-caches=on",
                "--bytecode-cache=off",
                "--jit=off",
            ),
        ),
        Variant("fast-preset", engine, ("--engine-preset=fast",)),
        Variant(
            "persistent-feedback-advisory",
            engine,
            (
                "--exec-format=auto",
                "--opt-level=2",
                "--superinstructions=off",
                "--quickening=on",
                "--inline-caches=on",
                "--bytecode-cache=off",
                "--jit=off",
            ),
            optional=True,
            persistent_feedback=True,
        ),
        Variant(
            "release-all-non-jit",
            release_engine,
            (
                "--exec-format=auto",
                "--opt-level=2",
                "--superinstructions=off",
                "--quickening=on",
                "--inline-caches=on",
                "--bytecode-cache=off",
                "--jit=off",
            ),
            optional=True,
        ),
    ]
    if args.include_jit:
        rows.append(
            Variant(
                "jit-cranelift",
                engine,
                (
                    "--exec-format=auto",
                    "--opt-level=2",
                    "--superinstructions=off",
                    "--quickening=on",
                    "--inline-caches=on",
                    "--bytecode-cache=off",
                    "--jit=cranelift",
                ),
                optional=True,
            )
        )
    if not args.include_persistent_feedback:
        rows = [row for row in rows if not row.persistent_feedback]
    return baseline, rows


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
            "PHRUST_RANDOM_SEED": "performance-acceleration-matrix",
            "RUST_TEST_SEED": "performance-acceleration-matrix",
        }
    )
    return env


def run_once(
    variant: Variant,
    fixture: Path,
    out_dir: Path,
    iteration: int,
    timeout: float,
) -> RunSample:
    stem = rel(fixture).replace("/", "__")
    run_dir = out_dir / "runs" / stem / variant.label
    run_dir.mkdir(parents=True, exist_ok=True)
    counters_path = run_dir / f"iter-{iteration}.counters.json"
    feedback_args: list[str] = []
    if variant.persistent_feedback:
        # Pin the consumption policy so the row keeps measuring seeded
        # execution even if the engine default changes.
        feedback_args = [
            "--persistent-feedback-read",
            str(run_dir / "advisory-feedback.pff"),
            "--persistent-feedback-consume=quickening",
            "--persistent-feedback-stats-json",
            str(run_dir / f"iter-{iteration}.persistent-feedback.json"),
        ]
    command = [
        str(variant.engine),
        "run",
        *variant.flags,
        *feedback_args,
        "--counters-json",
        str(counters_path),
        rel(fixture),
    ]
    start = time.perf_counter_ns()
    completed = subprocess.run(
        command,
        cwd=ROOT,
        env=normalized_env(out_dir / "tmp" / stem / variant.label),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
        check=False,
    )
    elapsed_ms = (time.perf_counter_ns() - start) / 1_000_000.0
    stdout = completed.stdout.replace("\r\n", "\n").replace("\r", "\n")
    stderr = normalize(completed.stderr)
    (run_dir / f"iter-{iteration}.stdout").write_text(stdout, encoding="utf-8")
    (run_dir / f"iter-{iteration}.stderr").write_text(stderr, encoding="utf-8")
    counters: dict[str, Any] = {}
    if counters_path.is_file():
        loaded = json.loads(counters_path.read_text(encoding="utf-8"))
        if not isinstance(loaded, dict):
            raise SystemExit(f"{rel(counters_path)}: counters root is not an object")
        counters = loaded
    for key, value in counters.items():
        if isinstance(value, int) and value < 0:
            raise SystemExit(f"{rel(counters_path)}: counter {key} is negative")
    return RunSample(elapsed_ms, completed.returncode, stdout, stderr, counters)


def run_variant(
    variant: Variant,
    fixture: Path,
    out_dir: Path,
    warmups: int,
    iterations: int,
    timeout: float,
) -> list[RunSample]:
    for warmup in range(warmups):
        run_once(variant, fixture, out_dir, -(warmup + 1), timeout)
    return [
        run_once(variant, fixture, out_dir, iteration, timeout)
        for iteration in range(iterations)
    ]


def median_ms(samples: list[RunSample]) -> float:
    return statistics.median(sample.elapsed_ms for sample in samples)


def compare_to_baseline(
    fixture: Path,
    variant: Variant,
    baseline: RunSample,
    sample: RunSample,
) -> None:
    differences: list[str] = []
    if sample.returncode != baseline.returncode:
        differences.append(
            f"exit status baseline={baseline.returncode} variant={sample.returncode}"
        )
    if sample.stdout != baseline.stdout:
        differences.append("stdout differs")
    if sample.stderr != baseline.stderr:
        differences.append("stderr/runtime diagnostics differ")
    if differences:
        raise SystemExit(
            "[fail] acceleration matrix changed behavior for "
            f"{rel(fixture)} under {variant.label}: " + "; ".join(differences)
        )


def render_markdown(summary: dict[str, Any]) -> str:
    lines = [
        "# Performance Acceleration Matrix",
        "",
        "Generated by `nix develop -c just acceleration-matrix`.",
        "Raw JSON and per-run stdout/stderr/counter artifacts are local-only under",
        "`target/performance/acceleration/` and must not be committed.",
        "",
        "Correctness is mandatory: each enabled row compares stdout, stderr/runtime",
        "diagnostics, and exit status against the `baseline-ir` row before timing",
        "is reported. Wall-clock timings are advisory host-local samples.",
        "",
        "## Summary",
        "",
        "| Field | Value |",
        "| --- | --- |",
        f"| Status | `{summary['status']}` |",
        f"| Fixtures | {summary['fixture_count']} |",
        f"| Enabled rows | {summary['enabled_row_count']} |",
        f"| Skipped rows | {summary['skipped_row_count']} |",
        f"| Warmups | {summary['warmups']} |",
        f"| Iterations | {summary['iterations']} |",
        "",
        "## Rows",
        "",
        "| Row | Fixture | Correctness | Median ms | Key counters |",
        "| --- | --- | --- | --- | --- |",
    ]
    for row in summary["rows"]:
        if row["status"] == "skip":
            lines.append(
                f"| `{row['variant']}` | `{row['fixture']}` | skip: {row['reason']} | n/a | n/a |"
            )
            continue
        counters = row["counter_focus"]
        focus = ", ".join(f"{key}={value}" for key, value in counters.items())
        lines.append(
            f"| `{row['variant']}` | `{row['fixture']}` | `{row['correctness']}` | "
            f"{row['median_ms']:.3f} | {focus or 'n/a'} |"
        )
    return "\n".join(lines) + "\n"


def counter_focus(counters: dict[str, Any]) -> dict[str, int]:
    keys = [
        "instructions_executed",
        "bytecode_instructions_executed",
        "superinstruction_candidates",
        "superinstructions_emitted",
        "superinstructions_executed",
        "quickening_specialized",
        "inline_cache_hits",
        "property_assign_ic_hits",
        "property_assign_ic_misses",
        "property_assign_ic_visibility_exits",
        "property_assign_ic_type_exits",
        "property_assign_ic_readonly_exits",
        "property_assign_ic_hook_magic_exits",
        "property_assign_ic_dynamic_exits",
        "output_fast_appends",
        "output_batched_appends",
        "output_batch_bytes",
        "string_concat_fast_path_hits",
        "concat_prealloc_hits",
        "packed_fetch_fast_hits",
        "value_clones",
        "string_allocations",
        "array_handle_clones",
        "cow_separations",
        "reference_cell_creations",
        "object_allocations",
    ]
    return {
        key: value
        for key in keys
        if isinstance((value := counters.get(key)), int) and value != 0
    }


def main() -> int:
    args = parse_args()
    out_dir = args.out_dir if args.out_dir.is_absolute() else ROOT / args.out_dir
    out_dir.mkdir(parents=True, exist_ok=True)
    fixtures = resolved_fixtures(args.fixture)
    baseline, matrix_rows = variants(args)
    if not baseline.engine.is_file() or not os.access(baseline.engine, os.X_OK):
        raise SystemExit(f"baseline engine is not executable: {baseline.engine}")

    rows: list[dict[str, Any]] = []
    skipped = 0
    enabled = 0
    for fixture in fixtures:
        baseline_samples = run_variant(
            baseline,
            fixture,
            out_dir,
            args.warmups,
            max(args.iterations, 1),
            args.timeout,
        )
        baseline_sample = baseline_samples[0]
        rows.append(
            {
                "variant": baseline.label,
                "fixture": rel(fixture),
                "status": "pass",
                "correctness": "baseline",
                "median_ms": median_ms(baseline_samples),
                "counter_focus": counter_focus(baseline_samples[-1].counters),
                "flags": list(baseline.flags),
                "engine": rel(baseline.engine),
            }
        )
        enabled += 1
        for variant in matrix_rows:
            fixture_key = rel(fixture)
            if variant.strict_bytecode_subset and fixture_key not in BYTECODE_STRICT_FIXTURES:
                skipped += 1
                rows.append(
                    {
                        "variant": variant.label,
                        "fixture": fixture_key,
                        "status": "skip",
                        "reason": "fixture outside strict dense-bytecode subset",
                    }
                )
                continue
            if variant.optional and (
                not variant.engine.is_file() or not os.access(variant.engine, os.X_OK)
            ):
                skipped += 1
                rows.append(
                    {
                        "variant": variant.label,
                        "fixture": fixture_key,
                        "status": "skip",
                        "reason": f"engine unavailable: {rel(variant.engine)}",
                    }
                )
                continue
            samples = run_variant(
                variant,
                fixture,
                out_dir,
                args.warmups,
                max(args.iterations, 1),
                args.timeout,
            )
            for sample in samples:
                compare_to_baseline(fixture, variant, baseline_sample, sample)
            rows.append(
                {
                    "variant": variant.label,
                    "fixture": fixture_key,
                    "status": "pass",
                    "correctness": "pass",
                    "median_ms": median_ms(samples),
                    "counter_focus": counter_focus(samples[-1].counters),
                    "flags": list(variant.flags),
                    "engine": rel(variant.engine),
                }
            )
            enabled += 1

    summary: dict[str, Any] = {
        "status": "pass",
        "gate": "acceleration-matrix",
        "fixtures": [rel(path) for path in fixtures],
        "fixture_count": len(fixtures),
        "iterations": max(args.iterations, 1),
        "warmups": args.warmups,
        "timing_policy": "advisory-host-local",
        "enabled_row_count": enabled,
        "skipped_row_count": skipped,
        "include_jit": args.include_jit,
        "include_persistent_feedback": args.include_persistent_feedback,
        "rows": rows,
    }
    json_path = out_dir / "summary.json"
    markdown_path = out_dir / "summary.md"
    json_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    markdown_path.write_text(render_markdown(summary), encoding="utf-8")
    print(
        "[pass] acceleration matrix compared "
        f"{enabled} enabled row(s), skipped {skipped} optional/subset row(s), "
        f"and wrote {rel(json_path)}"
    )
    if not args.include_jit:
        print("[skip] acceleration matrix JIT row: feature/platform not requested")
    if not args.include_persistent_feedback:
        print("[skip] acceleration matrix persistent-feedback row: default-off policy")
    return 0


if __name__ == "__main__":
    sys.exit(main())
