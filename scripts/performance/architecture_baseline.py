#!/usr/bin/env python3
"""Capture repeatable compile, binary-size, and runtime architecture baselines."""

from __future__ import annotations

import argparse
import json
import os
import platform
import re
import shutil
import statistics
import subprocess
import sys
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_OUT = ROOT / "target/architecture/performance-baseline"
COMPILE_CRATES = ("php_runtime", "php_vm", "php_executor", "php_server")
CRATE_ROOTS = {
    crate: ROOT / "crates" / crate / "src" / "lib.rs" for crate in COMPILE_CRATES
}
BENCHMARK_TARGETS = (
    ("vm_dispatch", "benchmark-suite"),
    ("include_cache", "inline-cache-smoke"),
    ("compiled_cache", "cache-roundtrip"),
    ("compiler", "optimizer-diff"),
    ("server", "server-benchmark-smoke"),
    ("application", "app-flow-smoke"),
    ("front_controller", "front-controller-hotpath-smoke"),
    ("wordpress", "wordpress-root-benchmark"),
)
BINARY_PATHS = (
    "target/release/php-vm",
    "target/release/phrust-php",
    "target/release/phrust-server",
)


class BaselineError(Exception):
    """An actionable measurement configuration error."""


def run_output(*command: str) -> str:
    result = subprocess.run(
        command,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode != 0:
        detail = result.stderr.strip() or result.stdout.strip()
        raise BaselineError(f"{' '.join(command)} failed: {detail}")
    return result.stdout


def just_targets() -> set[str]:
    source = (ROOT / "justfile").read_text(encoding="utf-8")
    return {
        match.group(1)
        for match in re.finditer(
            r"^([A-Za-z0-9_-]+)(?:\s+[^:]*)?:\s*$", source, re.MULTILINE
        )
    }


def time_wrapper() -> tuple[list[str], re.Pattern[str], int] | None:
    executable = Path("/usr/bin/time")
    if not executable.is_file():
        return None
    if platform.system() == "Darwin":
        return (
            [str(executable), "-l"],
            re.compile(r"^\s*(\d+)\s+maximum resident set size\s*$", re.MULTILINE),
            1,
        )
    return (
        [str(executable), "-v"],
        re.compile(
            r"^\s*Maximum resident set size \(kbytes\):\s*(\d+)\s*$",
            re.MULTILINE,
        ),
        1024,
    )


def tail(value: str, limit: int = 4_000) -> str:
    return value if len(value) <= limit else value[-limit:]


def measured_run(
    command: list[str],
    *,
    environment: dict[str, str] | None = None,
    log_path: Path,
) -> dict:
    wrapper = time_wrapper()
    wrapped = command if wrapper is None else [*wrapper[0], *command]
    started = time.perf_counter()
    result = subprocess.run(
        wrapped,
        cwd=ROOT,
        env=environment,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    elapsed = time.perf_counter() - started
    combined = "\n".join(part for part in (result.stdout, result.stderr) if part)
    log_path.parent.mkdir(parents=True, exist_ok=True)
    log_path.write_text(combined, encoding="utf-8")
    rss_bytes = None
    if wrapper is not None:
        match = wrapper[1].search(result.stderr)
        if match:
            rss_bytes = int(match.group(1)) * wrapper[2]
    lowered = combined.lower()
    status = "pass" if result.returncode == 0 else "fail"
    if result.returncode == 0 and ("[skip]" in lowered or '"status": "skip"' in lowered):
        status = "skip"
    return {
        "command": " ".join(command),
        "status": status,
        "exit_code": result.returncode,
        "wall_seconds": round(elapsed, 6),
        "peak_rss_bytes": rss_bytes,
        "output_tail": tail(combined),
        "log": log_path.relative_to(ROOT).as_posix(),
    }


def summarize_runs(runs: list[dict]) -> dict:
    durations = [run["wall_seconds"] for run in runs]
    rss_values = [run["peak_rss_bytes"] for run in runs if run["peak_rss_bytes"]]
    statuses = {run["status"] for run in runs}
    if "fail" in statuses:
        status = "fail"
    elif statuses == {"skip"}:
        status = "skip"
    else:
        status = "pass"
    return {
        "status": status,
        "runs": runs,
        "wall_seconds": {
            "median": round(statistics.median(durations), 6),
            "min": min(durations),
            "max": max(durations),
            "spread": round(max(durations) - min(durations), 6),
        },
        "peak_rss_bytes": {
            "max": max(rss_values) if rss_values else None,
            "samples": len(rss_values),
        },
    }


def cargo_environment(target_dir: Path) -> dict[str, str]:
    environment = os.environ.copy()
    environment["CARGO_TARGET_DIR"] = str(target_dir)
    environment["CARGO_INCREMENTAL"] = "1"
    environment.pop("RUSTC_WRAPPER", None)
    environment.setdefault("CARGO_BUILD_JOBS", "4")
    return environment


def run_unmeasured(command: list[str], environment: dict[str, str]) -> None:
    result = subprocess.run(
        command,
        cwd=ROOT,
        env=environment,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode != 0:
        detail = result.stderr.strip() or result.stdout.strip()
        raise BaselineError(f"{' '.join(command)} failed: {detail}")


def compile_baselines(out_dir: Path, runs: int, report: dict) -> None:
    target_dir = out_dir / "cargo-target"
    environment = cargo_environment(target_dir)
    build_all = ["cargo", "build"]
    for crate in COMPILE_CRATES:
        build_all.extend(("-p", crate))
    run_unmeasured(build_all, environment)

    for crate in COMPILE_CRATES:
        run_unmeasured(["cargo", "build", "-p", crate], environment)
        crate_report = {}
        report[crate] = crate_report
        clean_runs = []
        for index in range(1, runs + 1):
            run_unmeasured(["cargo", "clean", "-p", crate], environment)
            clean_runs.append(
                measured_run(
                    ["cargo", "build", "-p", crate],
                    environment=environment,
                    log_path=out_dir / "logs" / f"compile-clean-{crate}-{index}.log",
                )
            )
        crate_report["clean_package_rebuild"] = summarize_runs(clean_runs)
        incremental_runs = []
        source = CRATE_ROOTS[crate]
        for index in range(1, runs + 1):
            source.touch()
            incremental_runs.append(
                measured_run(
                    ["cargo", "build", "-p", crate],
                    environment=environment,
                    log_path=out_dir
                    / "logs"
                    / f"compile-incremental-{crate}-{index}.log",
                )
            )
        crate_report["incremental_root_touch_rebuild"] = summarize_runs(
            incremental_runs
        )


def binary_size_baseline(out_dir: Path) -> dict:
    build = measured_run(
        ["cargo", "build", "--release", "-p", "php_vm_cli", "-p", "php_server"],
        log_path=out_dir / "logs" / "release-binaries.log",
    )
    binaries = []
    for relative in BINARY_PATHS:
        path = ROOT / relative
        binaries.append(
            {
                "path": relative,
                "bytes": path.stat().st_size if path.is_file() else None,
            }
        )
    return {"build": build, "binaries": binaries}


def benchmark_baselines(out_dir: Path, runs: int) -> dict:
    available = just_targets()
    missing = [target for _, target in BENCHMARK_TARGETS if target not in available]
    if missing:
        raise BaselineError(f"required benchmark targets missing: {', '.join(missing)}")
    report = {}
    for category, target in BENCHMARK_TARGETS:
        samples = []
        for index in range(1, runs + 1):
            samples.append(
                measured_run(
                    ["just", target],
                    log_path=out_dir / "logs" / f"benchmark-{category}-{index}.log",
                )
            )
        report[category] = {"just_target": target, **summarize_runs(samples)}
    return report


def render_markdown(report: dict) -> str:
    lines = [
        "# Architecture Performance Baseline",
        "",
        f"Source revision: `{report['source_revision']}`",
        "",
        f"Runs per measurement: {report['runs_per_measurement']}",
        "",
    ]
    if "compile" in report:
        lines.extend(
            [
                "## Compile Time",
                "",
                (
                    "| Crate | Clean median (s) | Clean spread (s) | "
                    "Incremental median (s) | Incremental spread (s) | "
                    "Peak RSS (bytes) |"
                ),
                "| --- | ---: | ---: | ---: | ---: | ---: |",
            ]
        )
        for crate, measurements in report["compile"].items():
            clean = measurements.get("clean_package_rebuild")
            incremental = measurements.get("incremental_root_touch_rebuild")
            if clean is None or incremental is None:
                lines.append(f"| `{crate}` | incomplete |  |  |  |  |")
                continue
            rss_values = (
                clean["peak_rss_bytes"]["max"],
                incremental["peak_rss_bytes"]["max"],
            )
            peak_rss = max(value for value in rss_values if value is not None) if any(
                value is not None for value in rss_values
            ) else None
            lines.append(
                f"| `{crate}` | {clean['wall_seconds']['median']} | "
                f"{clean['wall_seconds']['spread']} | "
                f"{incremental['wall_seconds']['median']} | "
                f"{incremental['wall_seconds']['spread']} | {peak_rss or ''} |"
            )
        lines.append("")
    if "binary_size" in report:
        lines.extend(
            [
                "## Binary Size",
                "",
                "| Binary | Bytes |",
                "| --- | ---: |",
            ]
        )
        for binary in report["binary_size"]["binaries"]:
            lines.append(f"| `{binary['path']}` | {binary['bytes'] or ''} |")
        lines.append("")
    if "benchmarks" in report:
        lines.extend(
            [
                "## Runtime and Application Gates",
                "",
                "| Category | Just target | Status | Median (s) | Spread (s) | Peak RSS (bytes) |",
                "| --- | --- | --- | ---: | ---: | ---: |",
            ]
        )
        for category, measurement in report["benchmarks"].items():
            lines.append(
                f"| {category} | `{measurement['just_target']}` | {measurement['status']} | "
                f"{measurement['wall_seconds']['median']} | "
                f"{measurement['wall_seconds']['spread']} | "
                f"{measurement['peak_rss_bytes']['max'] or ''} |"
            )
        lines.append("")
    failures = report.get("failures", [])
    if failures:
        lines.extend(["## Failures", ""])
        lines.extend(f"- {failure}" for failure in failures)
        lines.append("")
    return "\n".join(lines)


def collect_failures(report: dict) -> list[str]:
    failures = []
    for crate, measurements in report.get("compile", {}).items():
        for name, measurement in measurements.items():
            if measurement["status"] == "fail":
                failures.append(f"{crate} {name} failed; inspect per-run logs")
    binary_build = report.get("binary_size", {}).get("build")
    if binary_build and binary_build["status"] == "fail":
        failures.append("release binary build failed")
    for category, measurement in report.get("benchmarks", {}).items():
        if measurement["status"] == "fail":
            failures.append(f"{category} benchmark target failed")
    return failures


def write_reports(out_dir: Path, report: dict) -> tuple[Path, Path]:
    json_path = out_dir / "report.json"
    markdown_path = out_dir / "report.md"
    json_path.write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    markdown_path.write_text(render_markdown(report), encoding="utf-8")
    return json_path, markdown_path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, default=DEFAULT_OUT)
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument(
        "--scope",
        action="append",
        choices=("compile", "binary-size", "benchmarks"),
        help="measurement group; repeat to select multiple (default: all)",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.runs < 3:
        print("[fail] architecture baseline: --runs must be at least 3", file=sys.stderr)
        return 1
    scopes = set(args.scope or ("compile", "binary-size", "benchmarks"))
    out_dir = args.out if args.out.is_absolute() else ROOT / args.out
    out_dir.mkdir(parents=True, exist_ok=True)
    source_revision = run_output("git", "rev-parse", "HEAD").strip()
    existing_path = out_dir / "report.json"
    report = {}
    if existing_path.is_file():
        try:
            existing = json.loads(existing_path.read_text(encoding="utf-8"))
        except json.JSONDecodeError as error:
            print(
                f"[fail] architecture baseline: invalid existing report: {error}",
                file=sys.stderr,
            )
            return 1
        if (
            existing.get("source_revision") == source_revision
            and existing.get("runs_per_measurement") == args.runs
        ):
            report = existing
    report.update({
        "schema_version": 1,
        "source_revision": source_revision,
        "runs_per_measurement": args.runs,
        "host": {
            "platform": platform.platform(),
            "machine": platform.machine(),
            "python": platform.python_version(),
            "peak_rss_supported": time_wrapper() is not None,
        },
        "methodology": {
            "clean_compile": (
                "cargo clean -p followed by a package build with warm dependencies "
                "in an isolated incremental target directory with sccache disabled"
            ),
            "incremental_compile": (
                "CARGO_INCREMENTAL=1 package rebuild after touching the crate root "
                "without changing content"
            ),
            "runtime": (
                "repository-owned just target; wall time and command peak RSS "
                "are supplemental to target artifacts"
            ),
        },
    })
    try:
        if "compile" in scopes:
            report["compile"] = {}
            compile_baselines(out_dir, args.runs, report["compile"])
        if "binary-size" in scopes:
            report["binary_size"] = binary_size_baseline(out_dir)
        if "benchmarks" in scopes:
            report["benchmarks"] = benchmark_baselines(out_dir, args.runs)
    except BaselineError as error:
        report["failures"] = [str(error)]
        _, markdown_path = write_reports(out_dir, report)
        print(f"[fail] architecture baseline: {error}", file=sys.stderr)
        print(f"Report: {markdown_path.relative_to(ROOT)}", file=sys.stderr)
        return 1
    report["failures"] = collect_failures(report)
    _, markdown_path = write_reports(out_dir, report)
    if report["failures"]:
        print("[fail] architecture baseline measurements failed:", file=sys.stderr)
        for failure in report["failures"]:
            print(f"  - {failure}", file=sys.stderr)
        print(f"Report: {markdown_path.relative_to(ROOT)}", file=sys.stderr)
        return 1
    shutil.rmtree(out_dir / "cargo-target", ignore_errors=True)
    print(f"[ok] architecture baseline wrote {markdown_path.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
