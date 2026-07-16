#!/usr/bin/env python3
"""Compare clean Phrust and PHP-FPM/OPcache WordPress HTTP requests."""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import os
import platform
import re
import shutil
import socket
import subprocess
import sys
import tempfile
import threading
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

WORDPRESS_SCRIPT_DIR = Path(__file__).resolve().parents[1] / "wordpress"
sys.path.insert(0, str(WORDPRESS_SCRIPT_DIR))
sys.path.insert(0, str(Path(__file__).resolve().parent))

from common import REPO_ROOT, now_run_id, repo_path, wordpress_shape_blockers  # noqa: E402
from http_engine_benchmark import (  # noqa: E402
    HttpTarget,
    bootstrap_percentile_ci,
    bootstrap_percentile_ratio_ci,
    http_get,
    normalize_headers,
    percentile,
    sample_curve,
)

DEFAULT_OUT_DIR = REPO_ROOT / "target/performance/wordpress-root"
DEFAULT_PHP_FPM_IMAGE = "phrust-php-fpm:8.5.7"
DEFAULT_NGINX_IMAGE = "nginx:1.28.0-alpine"
DEFAULT_OBSERVABLES = (("root", "/"),)
TARGET_PHP_VERSION = "8.5.7"
SUPPORTED_CRANELIFT_MACHINES = {"x86_64", "amd64", "aarch64", "arm64"}
CLEAN_TIMING_FORBIDDEN_ENV = (
    "PHRUST_PERF_TRACE",
    "PHRUST_SERVER_PERF_TRACE_VM_COUNTERS",
    "PHRUST_REQUEST_PROFILE",
    "PHRUST_REQUEST_PROFILE_VM_COUNTERS",
    "PHRUST_REQUEST_PROFILE_SOURCE_ATTRIBUTION",
    "PHRUST_PERF_ABLATION",
)
# Clean runs override and report these values instead of inheriting ambient
# state. Ablations and instrumentation remain hard failures above.
MANAGED_CLEAN_ENV = {
    "PHRUST_INCLUDE_REVALIDATE_MS": "2000",
    "PHRUST_WORKER_SYMBOL_EPOCH": "1",
    "PHRUST_PERSISTENT_FEEDBACK": "1",
}


@dataclass
class ManagedTarget:
    target: HttpTarget
    command: list[str]
    identity: dict[str, Any]
    cleanup_commands: list[list[str]] = field(default_factory=list)
    process: subprocess.Popen[str] | None = None
    log: Any = None
    artifacts: dict[str, str] = field(default_factory=dict)

    def stop(self) -> None:
        if self.process is not None and self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=5)
        if self.log is not None:
            self.log.close()
        for command in self.cleanup_commands:
            subprocess.run(command, cwd=REPO_ROOT, check=False, capture_output=True, text=True)


def main() -> int:
    args = parse_args()
    if args.self_test:
        return self_test()
    out_dir = output_dir(args)
    out_dir.mkdir(parents=True, exist_ok=True)
    if args.feedback_ab:
        report = run_feedback_ab(args, out_dir)
        write_json(report, out_dir / "summary.json")
        write_feedback_ab_markdown(report, out_dir / "summary.md")
        print(f"[{report['status']}] feedback A/B wrote {rel(out_dir / 'summary.md')}")
        if report["status"] == "fail":
            return 1
        if report["status"] == "skip" and args.strict:
            return 2
        return 0
    report = run(args, out_dir)
    write_json(report, out_dir / "summary.json")
    write_markdown(report, out_dir / "summary.md")
    print(f"[{report['status']}] wordpress performance gate wrote {rel(out_dir / 'summary.md')}")
    if report["status"] == "pass" and args.record_baseline:
        baseline_path = repo_path(args.record_baseline) or Path(args.record_baseline).expanduser()
        baseline_path.parent.mkdir(parents=True, exist_ok=True)
        write_json(report, baseline_path)
        print(f"[ok] recorded baseline at {rel(baseline_path)}")
    if report["status"] == "fail":
        return 1
    if report["status"] == "inconclusive":
        return 2
    if report["status"] == "skip" and args.strict:
        return 2
    return 0


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mode", choices=("clean", "diagnostic"), default="clean")
    parser.add_argument(
        "--phrust-url",
        "--url",
        dest="phrust_url",
        default=os.environ.get("PHRUST_WORDPRESS_PHRUST_URL", os.environ.get("PHRUST_WORDPRESS_URL", "")),
    )
    parser.add_argument("--php-url", default=os.environ.get("PHRUST_WORDPRESS_PHP_URL", ""))
    parser.add_argument("--host-header", default=os.environ.get("PHRUST_WORDPRESS_HOST_HEADER", ""))
    parser.add_argument("--wordpress-dir", default=os.environ.get("PHRUST_WORDPRESS_DIR", ""))
    parser.add_argument("--docroot", default=os.environ.get("PHRUST_WORDPRESS_DOCROOT", ""))
    parser.add_argument(
        "--server",
        default=os.environ.get("PHRUST_SERVER", "target/release/phrust-server"),
    )
    parser.add_argument(
        "--server-source-commit",
        default=os.environ.get("PHRUST_SERVER_SOURCE_COMMIT", ""),
        help="source commit used to build the supplied Phrust server binary",
    )
    parser.add_argument(
        "--server-source-patch-sha256",
        default=os.environ.get("PHRUST_SERVER_SOURCE_PATCH_SHA256", ""),
        help="optional SHA-256 identity of uncommitted source applied to that commit",
    )
    parser.add_argument("--php-version", default=os.environ.get("PHRUST_WORDPRESS_PHP_VERSION", ""))
    parser.add_argument("--php-fpm-image", default=os.environ.get("PHRUST_PHP_FPM_IMAGE", DEFAULT_PHP_FPM_IMAGE))
    parser.add_argument("--nginx-image", default=os.environ.get("PHRUST_NGINX_IMAGE", DEFAULT_NGINX_IMAGE))
    parser.add_argument("--docker-network", default=os.environ.get("PHRUST_WORDPRESS_DOCKER_NETWORK", ""))
    parser.add_argument("--out-dir", default="")
    parser.add_argument(
        "--samples",
        type=int,
        default=int(
            os.environ.get(
                "PHRUST_WORDPRESS_ROOT_SAMPLES", str(max(30, available_cpus() * 2))
            )
        ),
    )
    parser.add_argument("--warmups", type=int, default=int(os.environ.get("PHRUST_WORDPRESS_ROOT_WARMUPS", "5")))
    parser.add_argument(
        "--concurrency",
        default=os.environ.get("PHRUST_WORDPRESS_CONCURRENCY", ""),
        help="comma-separated levels; default is 1, CPU count, and twice CPU count",
    )
    parser.add_argument("--path", default="/")
    parser.add_argument(
        "--observable",
        action="append",
        default=[],
        metavar="NAME=PATH",
        help="HTTP observable compared after timing; repeat as needed",
    )
    parser.add_argument("--database-identity", default=os.environ.get("PHRUST_WORDPRESS_DB_IDENTITY", ""))
    parser.add_argument(
        "--timeout-seconds",
        type=float,
        default=float(os.environ.get("PHRUST_WORDPRESS_TIMEOUT_SECONDS", "120")),
    )
    parser.add_argument("--metrics-token", default=os.environ.get("PHRUST_METRICS_TOKEN", ""))
    parser.add_argument(
        "--engine-preset",
        choices=("baseline", "default"),
        default="default",
        help="Native compiler preset",
    )
    parser.add_argument(
        "--persistent-feedback",
        choices=("on", "off"),
        default="on",
        help="A/B switch for request-persistent quickening and callsite feedback",
    )
    parser.add_argument(
        "--feedback-ab",
        action="store_true",
        help="run isolated persistent-feedback off/on arms and write one comparison report",
    )
    parser.add_argument("--strict", action="store_true")
    parser.add_argument("--baseline", default="")
    parser.add_argument("--compare", default="", help="legacy baseline comparison; implies --strict")
    parser.add_argument("--record-baseline", default="")
    parser.add_argument("--max-latency-regression-pct", type=float, default=20.0)
    parser.add_argument(
        "--min-c1-p50-improvement-pct",
        type=float,
        default=None,
        help=(
            "require this minimum Phrust concurrency-1 p50 improvement over "
            "--baseline; use 3 for a normal performance tranche"
        ),
    )
    parser.add_argument(
        "--max-php-control-p50-delta-pct",
        type=float,
        default=None,
        help=(
            "reject a baseline comparison when the independently measured "
            "PHP-FPM concurrency-1 p50 drifts by more than this percentage; "
            "use 10 for adjacent performance tranches"
        ),
    )
    parser.add_argument(
        "--require-idle-host",
        action="store_true",
        help=(
            "capture host-process evidence before and during clean timing and "
            "mark the run inconclusive when competing build, test, benchmark, "
            "or CPU-intensive processes are observed"
        ),
    )
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)
    if args.compare:
        args.baseline = args.compare
        args.strict = True
    return args


def output_dir(args: argparse.Namespace) -> Path:
    if args.out_dir:
        return repo_path(args.out_dir) or Path(args.out_dir).expanduser()
    return DEFAULT_OUT_DIR / now_run_id(args.mode)


def run(args: argparse.Namespace, out_dir: Path) -> dict[str, Any]:
    errors = validate_configuration(args)
    if errors:
        return failure_report(errors, args, out_dir)
    preflight = capture_host_preflight() if args.require_idle_host else []
    if preflight and host_check_failures(preflight):
        return inconclusive_report(
            [],
            args,
            out_dir,
            preflight,
        )
    docroot = repo_path(args.docroot) or repo_path(args.wordpress_dir)
    if not args.phrust_url:
        blockers = wordpress_shape_blockers(docroot)
        if blockers:
            return unavailable_report(blockers, args, out_dir)
    targets: list[ManagedTarget] = []
    try:
        phrust = resolve_phrust(args, out_dir, docroot)
        targets.append(phrust)
        if args.mode == "diagnostic":
            return collect_diagnostics(args, out_dir, phrust, docroot)
        php = resolve_php_fpm(args, out_dir, docroot)
        targets.append(php)
        return collect_clean(args, out_dir, phrust, php, docroot)
    except EnvironmentError as error:
        return unavailable_report([str(error)], args, out_dir)
    except Exception as error:
        return failure_report([str(error)], args, out_dir)
    finally:
        for target in reversed(targets):
            target.stop()


def run_feedback_ab(args: argparse.Namespace, out_dir: Path) -> dict[str, Any]:
    """Run feedback-off and feedback-on arms with an explicit joint report."""
    errors = validate_configuration(args)
    if errors:
        return {
            "schema_version": 1,
            "status": "fail",
            "mode": "feedback-ab",
            "timing_eligible": False,
            "comparison": [],
            "arms": {},
            "failures": errors,
        }
    arms: dict[str, dict[str, Any]] = {}
    for arm in ("off", "on"):
        arm_args = copy.copy(args)
        arm_args.feedback_ab = False
        arm_args.persistent_feedback = arm
        arm_args.out_dir = str(out_dir / arm)
        arm_args.baseline = ""
        arm_args.compare = ""
        arm_args.record_baseline = ""
        arm_dir = out_dir / arm
        arm_dir.mkdir(parents=True, exist_ok=True)
        arm_report = run(arm_args, arm_dir)
        write_json(arm_report, arm_dir / "summary.json")
        write_markdown(arm_report, arm_dir / "summary.md")
        arms[arm] = arm_report

    statuses = {report.get("status") for report in arms.values()}
    status = "fail" if "fail" in statuses else "skip" if "skip" in statuses else "pass"
    return {
        "schema_version": 1,
        "status": status,
        "mode": "feedback-ab",
        "timing_eligible": all(report.get("timing_eligible") is True for report in arms.values()),
        "comparison": build_feedback_ab_ratios(arms["off"], arms["on"]),
        "arms": {
            arm: {
                "status": report.get("status"),
                "summary_json": rel(out_dir / arm / "summary.json"),
                "summary_markdown": rel(out_dir / arm / "summary.md"),
                "phrust_identity": ((report.get("engines") or {}).get("phrust") or {}).get("identity"),
                "correctness_failures": (report.get("correctness") or {}).get("failures", []),
            }
            for arm, report in arms.items()
        },
    }


def combined_status(*reports: dict[str, Any]) -> str:
    statuses = {report.get("status") for report in reports}
    return "fail" if "fail" in statuses else "skip" if "skip" in statuses else "pass"


def build_feedback_ab_ratios(
    off_report: dict[str, Any], on_report: dict[str, Any]
) -> list[dict[str, Any]]:
    off_curves = (((off_report.get("engines") or {}).get("phrust") or {}).get("curves") or [])
    on_curves = (((on_report.get("engines") or {}).get("phrust") or {}).get("curves") or [])
    on_by_concurrency = {curve.get("concurrency"): curve for curve in on_curves}
    comparisons: list[dict[str, Any]] = []
    for off in off_curves:
        concurrency = off.get("concurrency")
        on = on_by_concurrency.get(concurrency)
        if on is None:
            continue
        off_walls = [float(sample["wall_ms"]) for sample in off.get("samples", [])]
        on_walls = [float(sample["wall_ms"]) for sample in on.get("samples", [])]
        comparisons.append(
            {
                "concurrency": concurrency,
                "off_to_on_p50_latency": safe_ratio(
                    (off.get("latency_ms") or {}).get("p50"),
                    (on.get("latency_ms") or {}).get("p50"),
                ),
                "off_to_on_p95_latency": safe_ratio(
                    (off.get("latency_ms") or {}).get("p95"),
                    (on.get("latency_ms") or {}).get("p95"),
                ),
                "off_to_on_p95_latency_ci95": bootstrap_percentile_ratio_ci(
                    off_walls,
                    on_walls,
                    95,
                    seed=int(concurrency or 0) * 131 + len(off_walls),
                ) if off_walls and on_walls else None,
                "on_to_off_requests_per_second": safe_ratio(
                    on.get("requests_per_second"), off.get("requests_per_second")
                ),
            }
        )
    return comparisons


def validate_configuration(args: argparse.Namespace) -> list[str]:
    errors: list[str] = []
    if args.feedback_ab and args.mode != "clean":
        errors.append("--feedback-ab requires --mode clean")
    if args.feedback_ab and (args.baseline or args.compare or args.record_baseline):
        errors.append("--feedback-ab cannot record or compare a regression baseline")
    if args.require_idle_host and args.feedback_ab:
        errors.append("--require-idle-host is supported only by a single clean timing arm")
    if args.require_idle_host and args.mode != "clean":
        errors.append("--require-idle-host requires --mode clean")
    levels = concurrency_levels(args.concurrency)
    if args.samples < 1:
        errors.append("samples must be positive")
    if args.strict and args.mode == "clean" and args.samples < 30:
        errors.append("strict clean mode requires at least 30 measured requests per concurrency")
    if args.strict and args.mode == "clean" and not args.database_identity:
        errors.append(
            "strict clean mode requires --database-identity for the restored database snapshot"
        )
    if levels and args.samples < max(levels):
        errors.append("samples must be at least the largest concurrency level")
    if args.mode == "clean" and args.strict and args.phrust_url and not args.php_url:
        errors.append("strict URL mode requires --php-url; missing reference PHP-FPM")
    if args.mode == "clean" and args.php_url and args.strict and args.php_version != TARGET_PHP_VERSION:
        errors.append(
            f"strict remote PHP-FPM requires --php-version {TARGET_PHP_VERSION}"
        )
    if args.mode == "clean":
        active_instrumentation = [
            name for name in CLEAN_TIMING_FORBIDDEN_ENV if os.environ.get(name)
        ]
        if active_instrumentation:
            errors.append(
                "clean timing rejects Phrust instrumentation environment: "
                + ", ".join(active_instrumentation)
            )
    if args.baseline:
        baseline_path = repo_path(args.baseline)
        if baseline_path is None or not baseline_path.is_file():
            errors.append(f"strict regression baseline is missing: {args.baseline}")
    if args.min_c1_p50_improvement_pct is not None:
        if args.min_c1_p50_improvement_pct < 0:
            errors.append("--min-c1-p50-improvement-pct must be non-negative")
        if args.mode != "clean" or not args.strict:
            errors.append("--min-c1-p50-improvement-pct requires strict clean mode")
        if not args.baseline:
            errors.append("--min-c1-p50-improvement-pct requires --baseline")
    if args.max_php_control_p50_delta_pct is not None:
        if args.max_php_control_p50_delta_pct < 0:
            errors.append("--max-php-control-p50-delta-pct must be non-negative")
        if args.mode != "clean" or not args.strict:
            errors.append("--max-php-control-p50-delta-pct requires strict clean mode")
        if not args.baseline:
            errors.append("--max-php-control-p50-delta-pct requires --baseline")
    try:
        parse_observables(args.observable)
    except ValueError as error:
        errors.append(str(error))
    return errors


def concurrency_levels(value: str) -> list[int]:
    cpus = available_cpus()
    raw = [part.strip() for part in value.split(",") if part.strip()] if value else ["1", str(cpus), str(cpus * 2)]
    try:
        levels = [int(part) for part in raw]
    except ValueError as error:
        raise ValueError("concurrency levels must be positive integers") from error
    if any(level < 1 for level in levels):
        raise ValueError("concurrency levels must be positive integers")
    return list(dict.fromkeys(levels))


def available_cpus() -> int:
    if hasattr(os, "sched_getaffinity"):
        return max(1, len(os.sched_getaffinity(0)))
    return max(1, os.cpu_count() or 1)


HOST_BLOCKING_COMMANDS = re.compile(
    r"(?:^|[/ ])(?:cargo|rustc|rustfmt|clippy|pytest|hyperfine|oha)(?:$|[ ])"
    r"|\bjust\s+(?:ci|test|verify|perf)"
    r"|scripts/performance/.+(?:benchmark|smoke)",
    re.IGNORECASE,
)
HOST_CPU_BLOCKER_PCT = 20.0
HOST_AMBIENT_CPU_COMMANDS = re.compile(r"^(?:/System/Library/|/usr/libexec/)")
DOCKER_RUNTIME_MARKERS = (
    "com.docker.virtualization",
    "com.docker.backend",
    "com.apple.Virtualization.VirtualMachine",
)


def capture_host_snapshot(
    label: str,
    excluded_pids: tuple[int, ...] = (),
    *,
    allow_docker_runtime: bool = False,
) -> dict[str, Any]:
    completed = subprocess.run(
        ["ps", "-axo", "pid=,ppid=,%cpu=,%mem=,time=,etime=,command="],
        text=True,
        capture_output=True,
        check=False,
    )
    processes: list[dict[str, Any]] = []
    for line in completed.stdout.splitlines():
        fields = line.strip().split(None, 6)
        if len(fields) != 7:
            continue
        try:
            pid, ppid = int(fields[0]), int(fields[1])
            cpu_pct, memory_pct = float(fields[2]), float(fields[3])
            cpu_seconds = parse_cpu_time(fields[4])
        except ValueError:
            continue
        processes.append(
            {
                "pid": pid,
                "ppid": ppid,
                "cpu_pct": cpu_pct,
                "cpu_seconds": cpu_seconds,
                "memory_pct": memory_pct,
                "elapsed": fields[5],
                "command": fields[6],
            }
        )
    excluded = {os.getpid(), *excluded_pids}
    process_by_pid = {process["pid"]: process for process in processes}
    parent_pid = os.getppid()
    while parent_pid > 1 and parent_pid not in excluded:
        excluded.add(parent_pid)
        parent = process_by_pid.get(parent_pid)
        if parent is None:
            break
        parent_pid = parent["ppid"]
    if allow_docker_runtime:
        excluded.update(
            process["pid"]
            for process in processes
            if any(marker in process["command"] for marker in DOCKER_RUNTIME_MARKERS)
        )
    changed = True
    while changed:
        changed = False
        for process in processes:
            if process["ppid"] in excluded and process["pid"] not in excluded:
                excluded.add(process["pid"])
                changed = True
    blockers = []
    for process in processes:
        command = process["command"]
        if process["pid"] in excluded:
            continue
        reasons = []
        if HOST_BLOCKING_COMMANDS.search(command):
            reasons.append("competing build/test/benchmark command")
        if reasons:
            blockers.append({**process, "reasons": reasons})
    try:
        load_average = list(os.getloadavg())
    except OSError:
        load_average = None
    top_processes = sorted(processes, key=lambda process: process["cpu_pct"], reverse=True)[:12]
    return {
        "label": label,
        "captured_unix_seconds": time.time(),
        "captured_monotonic_seconds": time.monotonic(),
        "load_average": load_average,
        "logical_cpus": os.cpu_count(),
        "cpu_blocker_threshold_pct": HOST_CPU_BLOCKER_PCT,
        "blockers": blockers,
        "cpu_excluded_pids": sorted(excluded),
        "ambient_cpu_observations": [],
        "interval_cpu_blockers": [],
        "processes": processes,
        "top_processes": top_processes,
    }


def parse_cpu_time(value: str) -> float:
    days = 0.0
    if "-" in value:
        day_text, value = value.split("-", 1)
        days = float(day_text)
    fields = value.split(":")
    if len(fields) == 2:
        minutes, seconds = fields
        return days * 86400.0 + float(minutes) * 60.0 + float(seconds)
    if len(fields) == 3:
        hours, minutes, seconds = fields
        return (
            days * 86400.0
            + float(hours) * 3600.0
            + float(minutes) * 60.0
            + float(seconds)
        )
    raise ValueError(f"unsupported process CPU time: {value}")


def capture_host_preflight() -> list[dict[str, Any]]:
    snapshots = [capture_host_snapshot("preflight-before")]
    time.sleep(1.0)
    snapshots.append(capture_host_snapshot("preflight-after"))
    return snapshots


class HostIdleMonitor:
    """Capture low-frequency host evidence without instrumenting either engine."""

    def __init__(
        self,
        label: str,
        excluded_pids: tuple[int, ...],
        *,
        allow_docker_runtime: bool,
    ) -> None:
        self.label = label
        self.excluded_pids = excluded_pids
        self.allow_docker_runtime = allow_docker_runtime
        self.snapshots: list[dict[str, Any]] = []
        self._stop = threading.Event()
        self._thread: threading.Thread | None = None

    def start(self) -> None:
        self._capture("before")
        self._thread = threading.Thread(target=self._poll, daemon=True)
        self._thread.start()

    def _capture(self, phase: str) -> None:
        self.snapshots.append(
            capture_host_snapshot(
                f"{self.label}-{phase}",
                self.excluded_pids,
                allow_docker_runtime=self.allow_docker_runtime,
            )
        )

    def _poll(self) -> None:
        sample = 0
        while not self._stop.wait(2.0):
            sample += 1
            self._capture(f"during-{sample}")

    def stop(self) -> list[dict[str, Any]]:
        self._stop.set()
        if self._thread is not None:
            self._thread.join(timeout=3)
        self._capture("after")
        return self.snapshots


def host_check_failures(snapshots: list[dict[str, Any]]) -> list[str]:
    details = []
    for snapshot in snapshots:
        snapshot["ambient_cpu_observations"] = []
        snapshot["interval_cpu_blockers"] = []
        for blocker in snapshot["blockers"]:
            details.append(
                f"{snapshot['label']}: pid {blocker['pid']} "
                f"({', '.join(blocker['reasons'])})"
            )
    for previous, current in zip(snapshots, snapshots[1:]):
        if host_snapshot_group(previous["label"]) != host_snapshot_group(current["label"]):
            continue
        interval = current["captured_monotonic_seconds"] - previous["captured_monotonic_seconds"]
        if interval < 0.5:
            continue
        prior_processes = {process["pid"]: process for process in previous["processes"]}
        current_processes = {process["pid"]: process for process in current["processes"]}
        excluded = set(previous["cpu_excluded_pids"]) | set(current["cpu_excluded_pids"])
        for pid, process in current_processes.items():
            prior = prior_processes.get(pid)
            if prior is None or pid in excluded or prior["command"] != process["command"]:
                continue
            cpu_delta = max(0.0, process["cpu_seconds"] - prior["cpu_seconds"])
            cpu_pct = cpu_delta / interval * 100.0
            if cpu_pct < HOST_CPU_BLOCKER_PCT:
                continue
            blocker = {
                **process,
                "interval_seconds": interval,
                "interval_cpu_pct": cpu_pct,
                "reasons": [f"interval CPU usage >= {HOST_CPU_BLOCKER_PCT:.0f}%"],
            }
            if process["ppid"] == 1 and HOST_AMBIENT_CPU_COMMANDS.search(process["command"]):
                blocker["reasons"] = ["recorded macOS platform CPU usage"]
                current["ambient_cpu_observations"].append(blocker)
                continue
            current["interval_cpu_blockers"].append(blocker)
            details.append(
                f"{current['label']}: pid {pid} at {cpu_pct:.1f}% interval CPU"
            )
    if not details:
        return []
    return ["host idle control failed: " + "; ".join(details)]


def host_snapshot_group(label: str) -> str:
    if "-timing-" in label:
        return label.split("-timing-", 1)[0]
    if label.startswith("preflight-"):
        return "preflight"
    return label


def build_performance_decision(
    baseline: dict[str, Any] | None,
    control_failures: list[str],
    candidate_failures: list[str],
) -> dict[str, Any]:
    if baseline is None:
        return {
            "eligible": False,
            "status": "baseline_only",
            "reasons": ["no adjacent candidate comparison was requested"],
        }
    if control_failures:
        return {
            "eligible": False,
            "status": "inconclusive",
            "reasons": control_failures,
        }
    if candidate_failures:
        return {
            "eligible": True,
            "status": "revert",
            "reasons": candidate_failures,
        }
    return {"eligible": True, "status": "keep", "reasons": []}


def parse_observables(values: list[str]) -> list[tuple[str, str]]:
    if not values:
        return list(DEFAULT_OBSERVABLES)
    parsed = []
    for value in values:
        name, separator, path = value.partition("=")
        if not separator or not name.strip() or not path.startswith("/"):
            raise ValueError(f"invalid observable {value!r}; expected NAME=/path")
        parsed.append((name.strip(), path))
    return parsed


def resolve_phrust(args: argparse.Namespace, out_dir: Path, docroot: Path | None) -> ManagedTarget:
    if args.phrust_url:
        return ManagedTarget(
            HttpTarget("phrust", args.phrust_url.rstrip("/"), args.host_header),
            [],
            {"kind": "remote", "version": "operator supplied"},
        )
    if docroot is None:
        raise EnvironmentError("missing WordPress docroot")
    server = repo_path(args.server)
    if server is None or not server.is_file():
        raise EnvironmentError(f"missing Phrust server binary: {args.server}")
    if args.strict and not is_release_binary(server):
        raise RuntimeError(f"strict clean timing rejects non-release binary: {server}")
    log_path = out_dir / "phrust-server.log"
    log = log_path.open("w+", encoding="utf-8")
    command = [
        str(server),
        "--listen",
        "127.0.0.1:0",
        "--docroot",
        str(docroot),
        "--front-controller",
        "index.php",
        "--deployment-mode",
        "immutable",
        "--engine-preset",
        args.engine_preset,
        "--max-execution-ms",
        str(max(1, int(args.timeout_seconds * 1000))),
    ]
    preload_manifest = out_dir / "phrust-script-cache-preload.txt"
    preload_manifest.write_text("index.php\n", encoding="utf-8")
    command.extend(["--script-cache-preload", str(preload_manifest)])
    artifacts = {
        "server_log": rel(log_path),
        "script_cache_preload": rel(preload_manifest),
    }
    if args.mode == "diagnostic":
        profile_dir = out_dir / "request-profiles"
        trace_path = out_dir / "perf-trace.jsonl"
        profile_dir.mkdir(parents=True, exist_ok=True)
        command.extend(
            [
                "--perf-trace",
                str(trace_path),
                "--perf-trace-vm-counters",
                "--request-profile",
                str(profile_dir),
                "--request-profile-vm-counters",
            ]
        )
        artifacts.update({"request_profiles": rel(profile_dir), "trace": rel(trace_path)})
    startup_started_ns = time.perf_counter_ns()
    process_env = os.environ.copy()
    performance_environment = dict(MANAGED_CLEAN_ENV)
    performance_environment["PHRUST_PERSISTENT_FEEDBACK"] = (
        "1" if args.persistent_feedback == "on" else "0"
    )
    if args.mode == "clean":
        process_env.update(performance_environment)
        process_env.pop("PHRUST_PERF_ABLATION", None)
    process = subprocess.Popen(
        command,
        cwd=REPO_ROOT,
        env=process_env,
        text=True,
        stdout=log,
        stderr=subprocess.STDOUT,
    )
    base_url = wait_for_server(process, log)
    target = HttpTarget("phrust", base_url, args.host_header, (process.pid,))
    readiness = wait_for_native_readiness(target, args.timeout_seconds, args.metrics_token)
    identity = binary_identity(server)
    identity["source_commit"] = args.server_source_commit or None
    identity["source_patch_sha256"] = args.server_source_patch_sha256 or None
    identity["deployment_mode"] = "immutable"
    identity["engine_preset"] = args.engine_preset
    identity["persistent_feedback"] = args.persistent_feedback
    identity["performance_environment"] = (
        performance_environment if args.mode == "clean" else "diagnostic"
    )
    identity["startup_ms"] = (time.perf_counter_ns() - startup_started_ns) / 1_000_000.0
    identity["native_readiness"] = readiness
    return ManagedTarget(
        target,
        command,
        identity,
        process=process,
        log=log,
        artifacts=artifacts,
    )


def wait_for_server(process: subprocess.Popen[str], log: Any) -> str:
    for _ in range(300):
        if process.poll() is not None:
            log.flush()
            log.seek(0)
            raise RuntimeError(f"Phrust server exited early:\n{log.read()}")
        log.flush()
        log.seek(0)
        matches = re.findall(r"^listening http://(.+)$", log.read(), flags=re.MULTILINE)
        if matches:
            return f"http://{matches[-1].strip()}"
        time.sleep(0.05)
    raise RuntimeError("Phrust server did not print its listening address")


def resolve_php_fpm(args: argparse.Namespace, out_dir: Path, docroot: Path | None) -> ManagedTarget:
    if args.php_url:
        return ManagedTarget(
            HttpTarget("php-fpm", args.php_url.rstrip("/"), args.host_header),
            [],
            {"kind": "remote", "php_version": args.php_version, "opcache": "operator asserted"},
        )
    if docroot is None:
        raise EnvironmentError("missing WordPress docroot for PHP-FPM")
    if shutil.which("docker") is None:
        raise EnvironmentError("Docker is unavailable and --php-url was not supplied")
    if not docker_image_exists(args.php_fpm_image):
        raise EnvironmentError(
            f"missing reference image {args.php_fpm_image}; run just wordpress-reference-image"
        )
    if not docker_image_exists(args.nginx_image):
        raise EnvironmentError(f"missing nginx image {args.nginx_image}; pull it before the benchmark")
    suffix = f"{os.getpid()}-{int(time.time())}"
    use_host_network = sys.platform.startswith("linux") and not args.docker_network
    network = "host" if use_host_network else (args.docker_network or f"phrust-perf-{suffix}")
    created_network = not use_host_network and not args.docker_network
    fpm_name = f"phrust-fpm-{suffix}"
    nginx_name = f"phrust-nginx-{suffix}"
    cleanup: list[list[str]] = []
    if created_network:
        checked(["docker", "network", "create", network])
        cleanup.append(["docker", "network", "rm", network])
    fpm_port = available_port() if use_host_network else 9000
    nginx_port = available_port() if use_host_network else 80
    fpm_config = out_dir / "reference-php-fpm.conf"
    fpm_listen_host = "127.0.0.1" if use_host_network else "0.0.0.0"
    fpm_config.write_text(
        render_fpm_config(
            fpm_listen_host,
            fpm_port,
            max(concurrency_levels(args.concurrency)),
        ),
        encoding="utf-8",
    )
    nginx_config = out_dir / "reference-nginx.conf"
    upstream = f"127.0.0.1:{fpm_port}" if use_host_network else f"{fpm_name}:9000"
    nginx_config.write_text(render_nginx_config(docroot, upstream, nginx_port), encoding="utf-8")
    mount = f"{docroot}:{docroot}"
    fpm_command = [
        "docker", "run", "--detach", "--rm", "--name", fpm_name,
        "--network", network, "--volume", mount,
        "--volume", f"{fpm_config}:/tmp/phrust-reference-fpm.conf:ro",
        args.php_fpm_image,
        "php-fpm", "-F", "-y", "/tmp/phrust-reference-fpm.conf",
        "-d", "opcache.enable=1", "-d", "opcache.validate_timestamps=1",
        "-d", "opcache.revalidate_freq=0", "-d", "opcache.jit=0",
    ]
    checked(fpm_command)
    cleanup.insert(0, ["docker", "rm", "--force", fpm_name])
    nginx_command = [
        "docker", "run", "--detach", "--rm", "--name", nginx_name,
        "--network", network,
        "--volume", mount,
        "--volume", f"{nginx_config}:/etc/nginx/conf.d/default.conf:ro",
        args.nginx_image,
    ]
    if not use_host_network:
        nginx_command[8:8] = ["--publish", "127.0.0.1::80"]
    checked(nginx_command)
    cleanup.insert(0, ["docker", "rm", "--force", nginx_name])
    if use_host_network:
        base_url = f"http://127.0.0.1:{nginx_port}"
    else:
        port_text = checked(["docker", "port", nginx_name, "80/tcp"]).strip()
        base_url = f"http://127.0.0.1:{port_text.rsplit(':', 1)[-1]}"
    wait_for_http(HttpTarget("php-fpm", base_url, args.host_header), args.timeout_seconds)
    fpm_pid = int(checked(["docker", "inspect", "--format", "{{.State.Pid}}", fpm_name]).strip())
    nginx_pid = int(checked(["docker", "inspect", "--format", "{{.State.Pid}}", nginx_name]).strip())
    identity_text = checked(
        [
            "docker", "exec", fpm_name, "php",
            "-d", "opcache.enable=1",
            "-d", "opcache.validate_timestamps=1",
            "-d", "opcache.revalidate_freq=0",
            "-d", "opcache.jit=0",
            "-r",
            "echo json_encode(['php_version'=>PHP_VERSION,'opcache_version'=>phpversion('Zend OPcache')?:'missing','opcache_enable'=>ini_get('opcache.enable'),'opcache_validate_timestamps'=>ini_get('opcache.validate_timestamps'),'opcache_revalidate_freq'=>ini_get('opcache.revalidate_freq'),'opcache_jit'=>ini_get('opcache.jit')]);",
        ]
    )
    php_configuration = json.loads(identity_text)
    identity = {
        "kind": "docker_php_fpm_nginx",
        "php_version": php_configuration["php_version"],
        "opcache": php_configuration,
        "php_fpm_image": args.php_fpm_image,
        "php_fpm_image_id": checked(["docker", "image", "inspect", "--format", "{{.Id}}", args.php_fpm_image]).strip(),
        "nginx_image": args.nginx_image,
        "network": network,
        "fpm_children": max(concurrency_levels(args.concurrency)),
    }
    if args.strict and identity["php_version"] != TARGET_PHP_VERSION:
        for command in cleanup:
            subprocess.run(command, check=False, capture_output=True, text=True)
        raise RuntimeError(
            f"reference PHP version is {identity['php_version']}; expected {TARGET_PHP_VERSION}"
        )
    return ManagedTarget(
        HttpTarget("php-fpm", base_url, args.host_header, (fpm_pid, nginx_pid)),
        fpm_command + ["&&"] + nginx_command,
        identity,
        cleanup_commands=cleanup,
        artifacts={"nginx_config": rel(nginx_config), "fpm_config": rel(fpm_config)},
    )


def render_nginx_config(docroot: Path, upstream: str, listen_port: int) -> str:
    return f"""server {{
    listen {listen_port};
    server_name _;
    root {docroot};
    index index.php index.html;
    location / {{ try_files $uri $uri/ /index.php?$args; }}
    location ~ \\.php$ {{
        try_files $uri =404;
        include fastcgi_params;
        fastcgi_param SCRIPT_FILENAME $document_root$fastcgi_script_name;
        fastcgi_param DOCUMENT_ROOT $document_root;
        fastcgi_param SERVER_NAME $host;
        fastcgi_pass {upstream};
    }}
}}
"""


def render_fpm_config(listen_host: str, listen_port: int, children: int) -> str:
    return f"""[global]
daemonize = no
error_log = /proc/self/fd/2

[www]
listen = {listen_host}:{listen_port}
user = {os.getuid()}
group = {os.getgid()}
pm = static
pm.max_children = {children}
catch_workers_output = yes
clear_env = no
security.limit_extensions = .php
"""


def available_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener:
        listener.bind(("127.0.0.1", 0))
        return int(listener.getsockname()[1])


def collect_clean(
    args: argparse.Namespace,
    out_dir: Path,
    phrust: ManagedTarget,
    php: ManagedTarget,
    docroot: Path | None,
) -> dict[str, Any]:
    observables = parse_observables(args.observable)
    failures: list[str] = []
    for target in (phrust.target, php.target):
        for _ in range(args.warmups):
            http_get(target, args.path, args.timeout_seconds)
    before = {
        target.name: collect_observables(target, observables, args.timeout_seconds)
        for target in (phrust.target, php.target)
    }
    benchmark_before = {
        target.name: http_get(target, args.path, args.timeout_seconds)
        for target in (phrust.target, php.target)
    }
    failures.extend(compare_observables(before["phrust"], before["php-fpm"]))
    failures.extend(
        compare_observables(
            {"benchmark_path": benchmark_before["phrust"]},
            {"benchmark_path": benchmark_before["php-fpm"]},
        )
    )
    failures.extend(validate_benchmark_path_status(benchmark_before, strict=args.strict))
    if failures:
        return {
            "schema_version": 2,
            "status": "fail",
            "mode": "clean",
            "timing_eligible": False,
            "measurement_model": {
                "warmups_per_engine": args.warmups,
                "samples_per_concurrency": args.samples,
                "concurrency": concurrency_levels(args.concurrency),
                "instrumentation": "external_process_sampling_only",
                "vm_instrumentation": "disabled",
                "latency_percentile": "nearest-rank",
                "process_sampling_hz": 20,
            },
            "environment": environment_identity(docroot, args.database_identity),
            "engines": {
                managed.target.name: {
                    "command": managed.command,
                    "identity": managed.identity,
                    "base_url": managed.target.base_url,
                    "curves": [],
                }
                for managed in (phrust, php)
            },
            "correctness": {
                "before": before,
                "benchmark_before": benchmark_before,
                "after": {},
                "failures": failures,
            },
            "ratios": [],
            "baseline": baseline_summary(load_json(args.baseline)),
            "artifacts": artifact_paths(out_dir, phrust, php),
        }
    engines: dict[str, Any] = {}
    host_checks: list[dict[str, Any]] = []
    for managed in (phrust, php):
        monitor = HostIdleMonitor(
            f"{managed.target.name}-timing",
            managed.target.pids,
            allow_docker_runtime=True,
        ) if args.require_idle_host else None
        if monitor is not None:
            monitor.start()
        try:
            curves = [
                sample_curve(
                    managed.target,
                    args.path,
                    concurrency,
                    args.samples,
                    args.timeout_seconds,
                )
                for concurrency in concurrency_levels(args.concurrency)
            ]
        finally:
            if monitor is not None:
                host_checks.extend(monitor.stop())
        engines[managed.target.name] = {
            "command": managed.command,
            "identity": managed.identity,
            "base_url": managed.target.base_url,
            "curves": curves,
        }
        failures.extend(
            validate_curves(
                managed.target.name,
                curves,
                args.samples,
                benchmark_before[managed.target.name],
            )
        )
    after = {
        target.name: collect_observables(target, observables, args.timeout_seconds)
        for target in (phrust.target, php.target)
    }
    engines["phrust"]["post_run_metrics"] = fetch_metrics(
        phrust.target, args.timeout_seconds, args.metrics_token
    )
    failures.extend(compare_observables(after["phrust"], after["php-fpm"]))
    ratios = build_ratios(engines["phrust"]["curves"], engines["php-fpm"]["curves"])
    baseline = load_json(args.baseline)
    measurement_model = {
        "warmups_per_engine": args.warmups,
        "samples_per_concurrency": args.samples,
        "concurrency": concurrency_levels(args.concurrency),
        "instrumentation": "external_process_sampling_only",
        "vm_instrumentation": "disabled",
        "latency_percentile": "nearest-rank",
        "process_sampling_hz": 20,
    }
    environment = environment_identity(docroot, args.database_identity)
    comparison_failures = compare_baseline_identity(
        baseline,
        measurement_model,
        environment,
        engines["php-fpm"]["identity"],
    )
    baseline_comparisons, baseline_failures = compare_baseline(
        engines["phrust"]["curves"],
        baseline,
        args.max_latency_regression_pct,
        args.min_c1_p50_improvement_pct,
    )
    failures.extend(baseline_failures)
    php_control_comparisons, php_control_failures = compare_php_control(
        engines["php-fpm"]["curves"],
        baseline,
        args.max_php_control_p50_delta_pct,
    )
    host_control_failures = host_check_failures(host_checks)
    control_failures = comparison_failures + php_control_failures + host_control_failures
    decision = build_performance_decision(
        baseline,
        control_failures,
        failures,
    )
    status = "inconclusive" if control_failures else "fail" if failures else "pass"
    return {
        "schema_version": 2,
        "status": status,
        "mode": "clean",
        "timing_eligible": not control_failures,
        "measurement_model": measurement_model,
        "environment": environment,
        "engines": engines,
        "baseline_comparisons": baseline_comparisons,
        "php_control_comparisons": php_control_comparisons,
        "host_checks": host_checks,
        "performance_decision": decision,
        "correctness": {
            "before": before,
            "benchmark_before": benchmark_before,
            "after": after,
            "failures": failures,
        },
        "control_failures": control_failures,
        "ratios": ratios,
        "baseline": baseline_summary(baseline),
        "artifacts": artifact_paths(out_dir, phrust, php),
    }


def collect_diagnostics(
    args: argparse.Namespace,
    out_dir: Path,
    phrust: ManagedTarget,
    docroot: Path | None,
) -> dict[str, Any]:
    for _ in range(args.warmups):
        http_get(phrust.target, args.path, args.timeout_seconds)
    sample = http_get(
        phrust.target,
        args.path,
        args.timeout_seconds,
        {"X-Phrust-Request-Profile": "1"},
    )
    metrics = fetch_metrics(phrust.target, args.timeout_seconds, args.metrics_token)
    profiles = sorted((out_dir / "request-profiles").glob("*.json"))
    profile = json.loads(profiles[-1].read_text(encoding="utf-8")) if profiles else {}
    failures = [] if sample["status"] < 500 else [f"diagnostic request returned HTTP {sample['status']}"]
    return {
        "schema_version": 2,
        "status": "fail" if failures else "pass",
        "mode": "diagnostic",
        "timing_eligible": False,
        "warning": "instrumented samples are excluded from clean latency comparisons",
        "environment": environment_identity(docroot, args.database_identity),
        "engine": {"command": phrust.command, "identity": phrust.identity},
        "sample": sample,
        "metrics": metrics,
        "profile": profile,
        "correctness": {"failures": failures},
        "artifacts": artifact_paths(out_dir, phrust),
    }


def collect_observables(
    target: HttpTarget,
    observables: list[tuple[str, str]],
    timeout_seconds: float,
) -> dict[str, Any]:
    return {name: http_get(target, path, timeout_seconds) for name, path in observables}


def compare_observables(left: dict[str, Any], right: dict[str, Any]) -> list[str]:
    failures: list[str] = []
    for name in sorted(set(left) | set(right)):
        if name not in left or name not in right:
            failures.append(f"observable {name!r} is missing from one engine")
            continue
        for observable_field in ("status", "headers", "body_sha256"):
            if left[name].get(observable_field) != right[name].get(observable_field):
                failures.append(
                    f"observable {name!r} {observable_field} differs between Phrust and PHP-FPM"
                )
    return failures


def validate_benchmark_path_status(
    samples: dict[str, dict[str, Any]], *, strict: bool
) -> list[str]:
    if not strict:
        return []
    return [
        f"{name} benchmark path returned HTTP {sample.get('status')}; "
        "strict clean timing requires HTTP 200"
        for name, sample in samples.items()
        if sample.get("status") != 200
    ]


def validate_curves(
    name: str,
    curves: list[dict[str, Any]],
    requested: int,
    expected: dict[str, Any],
) -> list[str]:
    failures: list[str] = []
    for curve in curves:
        concurrency = curve["concurrency"]
        if curve["completed_samples"] != requested:
            failures.append(
                f"{name} concurrency {concurrency} completed {curve['completed_samples']} of {requested} samples"
            )
        failures.extend(f"{name} concurrency {concurrency}: {failure}" for failure in curve["failures"])
        for index, sample in enumerate(curve["samples"]):
            if sample["status"] >= 500:
                failures.append(f"{name} concurrency {concurrency} sample {index} returned HTTP {sample['status']}")
            for sample_field in ("status", "headers", "body_sha256"):
                if sample.get(sample_field) != expected.get(sample_field):
                    failures.append(
                        f"{name} concurrency {concurrency} sample {index} {sample_field} "
                        "differs from the warmed correctness sample"
                    )
    return failures


def build_ratios(phrust_curves: list[dict[str, Any]], php_curves: list[dict[str, Any]]) -> list[dict[str, Any]]:
    php_by_concurrency = {curve["concurrency"]: curve for curve in php_curves}
    ratios = []
    for phrust in phrust_curves:
        php = php_by_concurrency[phrust["concurrency"]]
        phrust_walls = [float(sample["wall_ms"]) for sample in phrust["samples"]]
        php_walls = [float(sample["wall_ms"]) for sample in php["samples"]]
        concurrency = int(phrust["concurrency"])
        ratios.append(
            {
                "concurrency": concurrency,
                "phrust_to_php_p50_latency": safe_ratio(phrust["latency_ms"]["p50"], php["latency_ms"]["p50"]),
                "phrust_to_php_p50_latency_ci95": bootstrap_percentile_ratio_ci(
                    phrust_walls, php_walls, 50, seed=concurrency * 107 + len(phrust_walls)
                ),
                "phrust_to_php_p95_latency": safe_ratio(phrust["latency_ms"]["p95"], php["latency_ms"]["p95"]),
                "phrust_to_php_p95_latency_ci95": bootstrap_percentile_ratio_ci(
                    phrust_walls, php_walls, 95, seed=concurrency * 109 + len(phrust_walls)
                ),
                "phrust_to_php_requests_per_second": safe_ratio(phrust["requests_per_second"], php["requests_per_second"]),
                "phrust_to_php_cpu_seconds": safe_ratio(phrust["process"]["cpu_seconds"], php["process"]["cpu_seconds"]),
                "phrust_to_php_peak_rss": safe_ratio(phrust["process"]["peak_rss_bytes"], php["process"]["peak_rss_bytes"]),
            }
        )
    return ratios


def safe_ratio(left: Any, right: Any) -> float | None:
    if not isinstance(left, (int, float)) or not isinstance(right, (int, float)) or right == 0:
        return None
    return left / right


def compare_baseline(
    curves: list[dict[str, Any]],
    baseline: dict[str, Any] | None,
    p95_regression_threshold: float,
    min_c1_p50_improvement_pct: float | None = None,
) -> tuple[list[dict[str, Any]], list[str]]:
    if baseline is None:
        return [], []
    previous_curves = (((baseline.get("engines") or {}).get("phrust") or {}).get("curves") or [])
    previous = {curve.get("concurrency"): curve for curve in previous_curves}
    comparisons = []
    failures = []
    for curve in curves:
        old = previous.get(curve["concurrency"])
        if not old:
            continue
        concurrency = int(curve["concurrency"])
        current_p50 = curve["latency_ms"]["p50"]
        old_p50 = (old.get("latency_ms") or {}).get("p50")
        current_p95 = curve["latency_ms"]["p95"]
        old_p95 = (old.get("latency_ms") or {}).get("p95")
        p50_improvement = None
        if isinstance(current_p50, (int, float)) and isinstance(old_p50, (int, float)) and old_p50 > 0:
            p50_improvement = (old_p50 - current_p50) / old_p50 * 100.0
        current_walls = [float(sample["wall_ms"]) for sample in curve.get("samples", [])]
        old_walls = [float(sample["wall_ms"]) for sample in old.get("samples", [])]
        comparisons.append(
            {
                "concurrency": concurrency,
                "phrust_p50_improvement_pct": p50_improvement,
                "baseline_to_current_p50_latency": safe_ratio(old_p50, current_p50),
                "baseline_to_current_p50_latency_ci95": bootstrap_percentile_ratio_ci(
                    old_walls,
                    current_walls,
                    50,
                    seed=concurrency * 137 + len(current_walls),
                ) if old_walls and current_walls else None,
            }
        )
        if concurrency == 1 and min_c1_p50_improvement_pct is not None:
            if p50_improvement is None:
                failures.append("Phrust concurrency-1 p50 improvement is unavailable")
            elif p50_improvement < min_c1_p50_improvement_pct:
                failures.append(
                    "Phrust p50 at concurrency 1 improved by "
                    f"{p50_improvement:.1f}%; required {min_c1_p50_improvement_pct:.1f}%"
                )
        if isinstance(current_p95, (int, float)) and isinstance(old_p95, (int, float)) and old_p95 > 0:
            regression = (current_p95 - old_p95) / old_p95 * 100.0
            if regression > p95_regression_threshold:
                failures.append(
                    f"Phrust p95 at concurrency {curve['concurrency']} regressed by {regression:.1f}%"
                )
    if min_c1_p50_improvement_pct is not None and 1 not in previous:
        failures.append("performance baseline is missing concurrency-1 Phrust results")
    elif min_c1_p50_improvement_pct is not None and not any(
        int(curve.get("concurrency", 0)) == 1 for curve in curves
    ):
        failures.append("current benchmark is missing concurrency-1 Phrust results")
    return comparisons, failures


def compare_php_control(
    curves: list[dict[str, Any]],
    baseline: dict[str, Any] | None,
    max_c1_p50_delta_pct: float | None,
) -> tuple[list[dict[str, Any]], list[str]]:
    if baseline is None:
        return [], []
    previous_curves = (((baseline.get("engines") or {}).get("php-fpm") or {}).get("curves") or [])
    previous = {curve.get("concurrency"): curve for curve in previous_curves}
    comparisons: list[dict[str, Any]] = []
    failures: list[str] = []
    for curve in curves:
        old = previous.get(curve.get("concurrency"))
        if not old:
            continue
        concurrency = int(curve["concurrency"])
        current_p50 = (curve.get("latency_ms") or {}).get("p50")
        old_p50 = (old.get("latency_ms") or {}).get("p50")
        delta_pct = None
        if (
            isinstance(current_p50, (int, float))
            and isinstance(old_p50, (int, float))
            and old_p50 > 0
        ):
            delta_pct = (current_p50 - old_p50) / old_p50 * 100.0
        comparisons.append(
            {
                "concurrency": concurrency,
                "baseline_p50_ms": old_p50,
                "current_p50_ms": current_p50,
                "php_p50_delta_pct": delta_pct,
            }
        )
        if concurrency == 1 and max_c1_p50_delta_pct is not None:
            if delta_pct is None:
                failures.append("PHP-FPM concurrency-1 p50 control comparison is unavailable")
            elif abs(delta_pct) > max_c1_p50_delta_pct:
                failures.append(
                    "PHP-FPM p50 control at concurrency 1 drifted by "
                    f"{delta_pct:+.1f}%; allowed +/-{max_c1_p50_delta_pct:.1f}%"
                )
    if max_c1_p50_delta_pct is not None and 1 not in previous:
        failures.append("performance baseline is missing concurrency-1 PHP-FPM control results")
    elif max_c1_p50_delta_pct is not None and not any(
        int(curve.get("concurrency", 0)) == 1 for curve in curves
    ):
        failures.append("current benchmark is missing concurrency-1 PHP-FPM control results")
    return comparisons, failures


def compare_baseline_identity(
    baseline: dict[str, Any] | None,
    measurement_model: dict[str, Any],
    environment: dict[str, Any],
    php_identity: dict[str, Any],
) -> list[str]:
    if baseline is None:
        return []
    failures: list[str] = []
    if baseline.get("schema_version") != 2 or baseline.get("mode") != "clean":
        failures.append("baseline is not a schema-version-2 clean benchmark")
    previous_model = baseline.get("measurement_model") or {}
    for model_field in (
        "warmups_per_engine",
        "samples_per_concurrency",
        "concurrency",
        "latency_percentile",
        "process_sampling_hz",
    ):
        if previous_model.get(model_field) != measurement_model.get(model_field):
            failures.append(f"baseline measurement model differs for {model_field}")
    previous_environment = baseline.get("environment") or {}
    for environment_field in ("platform", "available_cpus", "database_identity"):
        if previous_environment.get(environment_field) != environment.get(environment_field):
            failures.append(f"baseline environment differs for {environment_field}")
    previous_wordpress = previous_environment.get("wordpress") or {}
    current_wordpress = environment.get("wordpress") or {}
    for wordpress_field in ("version", "git_commit", "tree_sha256", "file_count"):
        if previous_wordpress.get(wordpress_field) != current_wordpress.get(wordpress_field):
            failures.append(f"baseline WordPress identity differs for {wordpress_field}")
    previous_php = (((baseline.get("engines") or {}).get("php-fpm") or {}).get("identity") or {})
    for php_field in ("php_version", "php_fpm_image_id", "opcache"):
        if previous_php.get(php_field) != php_identity.get(php_field):
            failures.append(f"baseline PHP-FPM identity differs for {php_field}")
    return failures


def fetch_metrics(target: HttpTarget, timeout: float, token: str) -> dict[str, float]:
    headers = {"Authorization": f"Bearer {token}"} if token else None
    sample = raw_http_text(target, "/__phrust/metrics", timeout, headers)
    values: dict[str, float] = {}
    if sample["status"] != 200:
        return values
    for line in sample["text"].splitlines():
        if not line or line.startswith("#"):
            continue
        name, _, value = line.partition(" ")
        try:
            values[name] = float(value)
        except ValueError:
            continue
    return values


def wait_for_native_readiness(
    target: HttpTarget, timeout: float, token: str
) -> dict[str, float]:
    deadline = time.monotonic() + timeout
    required = (
        "phrust_server_script_cache_ready",
        "phrust_server_native_prewarm_complete",
        "phrust_server_native_compile_queue_empty",
    )
    last: dict[str, float] = {}
    while time.monotonic() < deadline:
        try:
            last = fetch_metrics(target, min(timeout, 2.0), token)
        except OSError:
            time.sleep(0.05)
            continue
        if all(last.get(metric) == 1.0 for metric in required):
            return {
                metric: last[metric]
                for metric in (
                    *required,
                    "phrust_server_native_code_cache_generation",
                    "phrust_server_native_prewarm_entries_total",
                    "phrust_server_native_prewarm_nanos_total",
                )
                if metric in last
            }
        time.sleep(0.05)
    raise RuntimeError(f"Phrust native readiness did not quiesce: {last}")


def raw_http_text(target: HttpTarget, path: str, timeout: float, headers: dict[str, str] | None) -> dict[str, Any]:
    # Reuse the standard library client but retain the body for metrics parsing.
    import http.client
    from urllib.parse import urlparse

    parsed = urlparse(target.base_url)
    connection = http.client.HTTPConnection(parsed.hostname, parsed.port or 80, timeout=timeout)
    request_headers = {"Host": target.host_header or parsed.netloc}
    request_headers.update(headers or {})
    try:
        connection.request("GET", path, headers=request_headers)
        response = connection.getresponse()
        text = response.read().decode("utf-8", errors="replace")
        return {"status": response.status, "text": text}
    finally:
        connection.close()


def wait_for_http(target: HttpTarget, timeout: float) -> None:
    deadline = time.monotonic() + min(timeout, 30.0)
    last_error = "not ready"
    while time.monotonic() < deadline:
        try:
            http_get(target, "/", min(timeout, 2.0))
            return
        except Exception as error:
            last_error = str(error)
            time.sleep(0.1)
    raise RuntimeError(f"reference HTTP server did not become ready: {last_error}")


def is_release_binary(path: Path) -> bool:
    parts = {part.lower() for part in path.resolve().parts}
    return "release" in parts or "profiling" in parts


def binary_identity(path: Path) -> dict[str, Any]:
    return {
        "kind": "local_binary",
        "path": str(path.resolve()),
        "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
        "bytes": path.stat().st_size,
        "release_path": is_release_binary(path),
    }


def rust_host_triple() -> str | None:
    verbose = command_output(["rustc", "-vV"], REPO_ROOT)
    if verbose is None:
        return None
    for line in verbose.splitlines():
        if line.startswith("host:"):
            return line.split(":", 1)[1].strip()
    return None


def cranelift_dependency_version() -> str | None:
    lock_path = REPO_ROOT / "Cargo.lock"
    if not lock_path.is_file():
        return None
    match = re.search(
        r'\[\[package\]\]\s*\nname = "cranelift-codegen"\s*\nversion = "([^"]+)"',
        lock_path.read_text(encoding="utf-8"),
    )
    return match.group(1) if match else None


def native_source_abi_identity() -> dict[str, int | str | None]:
    abi_path = REPO_ROOT / "crates/php_jit/src/abi.rs"
    if not abi_path.is_file():
        return {"version": None, "hash": None, "hash_hex": None}
    source = abi_path.read_text(encoding="utf-8")
    version_match = re.search(r"JIT_RUNTIME_ABI_VERSION:\s*u32\s*=\s*(\d+)", source)
    hash_match = re.search(
        r"JIT_RUNTIME_ABI_HASH:\s*u64\s*=\s*(0x[0-9a-fA-F_]+)", source
    )
    hash_hex = hash_match.group(1).replace("_", "") if hash_match else None
    return {
        "version": int(version_match.group(1)) if version_match else None,
        "hash": int(hash_hex, 16) if hash_hex else None,
        "hash_hex": hash_hex,
    }


def cpu_identity() -> dict[str, Any]:
    values: dict[str, str] = {}
    cpuinfo = Path("/proc/cpuinfo")
    if cpuinfo.is_file():
        first = cpuinfo.read_text(encoding="utf-8", errors="replace").split("\n\n", 1)[0]
        for line in first.splitlines():
            key, separator, value = line.partition(":")
            if separator:
                values[key.strip().lower()] = value.strip()
    features = sorted(
        set((values.get("flags") or values.get("features") or "").split())
    )
    feature_material = "\n".join(features).encode("utf-8")
    return {
        "vendor": values.get("vendor_id") or values.get("cpu implementer"),
        "model_name": values.get("model name") or platform.processor() or None,
        "family": values.get("cpu family"),
        "model": values.get("model"),
        "stepping": values.get("stepping"),
        "feature_count": len(features),
        "feature_fingerprint_sha256": hashlib.sha256(feature_material).hexdigest(),
    }


def environment_identity(docroot: Path | None, database_identity: str) -> dict[str, Any]:
    return {
        "platform": sys.platform,
        "platform_machine": platform.machine(),
        "rust_target_triple": rust_host_triple(),
        "cpu": cpu_identity(),
        "cranelift_version": cranelift_dependency_version(),
        "native_runtime_abi": native_source_abi_identity(),
        "logical_cpus": os.cpu_count(),
        "available_cpus": available_cpus(),
        "repository_commit": command_output(["git", "rev-parse", "HEAD"], REPO_ROOT),
        "repository_dirty": bool(command_output(["git", "status", "--porcelain"], REPO_ROOT)),
        "wordpress": wordpress_identity(docroot),
        "database_identity": database_identity or None,
    }


def wordpress_identity(docroot: Path | None) -> dict[str, Any]:
    if docroot is None:
        return {
            "docroot": None,
            "version": None,
            "git_commit": None,
            "tree_sha256": None,
            "file_count": None,
        }
    version_file = docroot / "wp-includes/version.php"
    version = None
    if version_file.is_file():
        match = re.search(r"\$wp_version\s*=\s*'([^']+)'", version_file.read_text(encoding="utf-8", errors="replace"))
        version = match.group(1) if match else None
    resolved_docroot = docroot.resolve()
    git_root = command_output(["git", "-C", str(docroot), "rev-parse", "--show-toplevel"])
    git_commit = None
    if git_root is not None and Path(git_root).resolve() == resolved_docroot:
        git_commit = command_output(["git", "-C", str(docroot), "rev-parse", "HEAD"])
    tree_sha256, file_count = directory_identity(resolved_docroot)
    return {
        "docroot": str(resolved_docroot),
        "version": version,
        "git_commit": git_commit,
        "tree_sha256": tree_sha256,
        "file_count": file_count,
    }


def directory_identity(root: Path) -> tuple[str, int]:
    digest = hashlib.sha256()
    file_count = 0
    for path in sorted(root.rglob("*")):
        relative = path.relative_to(root)
        if ".git" in relative.parts or not path.is_file():
            continue
        relative_bytes = relative.as_posix().encode("utf-8", errors="surrogateescape")
        digest.update(len(relative_bytes).to_bytes(8, "big"))
        digest.update(relative_bytes)
        with path.open("rb") as source:
            for chunk in iter(lambda: source.read(1024 * 1024), b""):
                digest.update(chunk)
        file_count += 1
    return digest.hexdigest(), file_count


def command_output(command: list[str], cwd: Path | None = None) -> str | None:
    completed = subprocess.run(command, cwd=cwd, text=True, capture_output=True, check=False)
    return completed.stdout.strip() if completed.returncode == 0 else None


def checked(command: list[str]) -> str:
    completed = subprocess.run(command, cwd=REPO_ROOT, text=True, capture_output=True, check=False)
    if completed.returncode != 0:
        detail = completed.stderr.strip() or completed.stdout.strip()
        raise RuntimeError(f"command failed ({' '.join(command)}): {detail}")
    return completed.stdout


def docker_image_exists(image: str) -> bool:
    return subprocess.run(
        ["docker", "image", "inspect", image], capture_output=True, text=True, check=False
    ).returncode == 0


def load_json(path_text: str) -> dict[str, Any] | None:
    path = repo_path(path_text) if path_text else None
    if path is None or not path.is_file():
        return None
    return json.loads(path.read_text(encoding="utf-8"))


def baseline_summary(baseline: dict[str, Any] | None) -> dict[str, Any]:
    return {"configured": baseline is not None}


def unavailable_report(blockers: list[str], args: argparse.Namespace, out_dir: Path) -> dict[str, Any]:
    return {
        "schema_version": 2,
        "status": "fail" if args.strict else "skip",
        "mode": args.mode,
        "timing_eligible": False,
        "reason": "environment",
        "failures": blockers,
        "artifacts": artifact_paths(out_dir),
    }


def failure_report(failures: list[str], args: argparse.Namespace, out_dir: Path) -> dict[str, Any]:
    return {
        "schema_version": 2,
        "status": "fail",
        "mode": args.mode,
        "timing_eligible": False,
        "failures": failures,
        "artifacts": artifact_paths(out_dir),
    }


def inconclusive_report(
    failures: list[str],
    args: argparse.Namespace,
    out_dir: Path,
    host_checks: list[dict[str, Any]],
) -> dict[str, Any]:
    reasons = failures + host_check_failures(host_checks)
    return {
        "schema_version": 2,
        "status": "inconclusive",
        "mode": args.mode,
        "timing_eligible": False,
        "control_failures": reasons,
        "host_checks": host_checks,
        "performance_decision": {
            "eligible": False,
            "status": "inconclusive",
            "reasons": reasons,
        },
        "artifacts": artifact_paths(out_dir),
    }


def artifact_paths(out_dir: Path, *targets: ManagedTarget) -> dict[str, str]:
    artifacts = {
        "summary_json": rel(out_dir / "summary.json"),
        "summary_markdown": rel(out_dir / "summary.md"),
    }
    for target in targets:
        artifacts.update({f"{target.target.name}_{key}": value for key, value in target.artifacts.items()})
    return artifacts


def write_json(value: dict[str, Any], path: Path) -> None:
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def write_markdown(report: dict[str, Any], path: Path) -> None:
    lines = ["# WordPress PHP-FPM Performance Gate", "", f"Status: `{report['status']}`", "", f"Mode: `{report['mode']}`", ""]
    if report.get("mode") == "diagnostic":
        lines.extend(["Instrumented diagnostic data; not eligible for latency comparison.", ""])
    for failure in report.get("failures", []) or (report.get("correctness") or {}).get("failures", []):
        lines.append(f"- {failure}")
    for failure in report.get("control_failures", []):
        lines.append(f"- {failure}")
    decision = report.get("performance_decision")
    if decision:
        lines.extend(
            [
                "",
                "## Performance decision",
                "",
                f"- eligible: `{str(decision['eligible']).lower()}`",
                f"- status: `{decision['status']}`",
            ]
        )
        for reason in decision.get("reasons", []):
            lines.append(f"- reason: {reason}")
        lines.append("")
    if "engines" in report:
        lines.extend(["## Clean timing", ""])
        for name, engine in report["engines"].items():
            lines.append(f"### {name}")
            lines.append("")
            for curve in engine["curves"]:
                latency = curve["latency_ms"]
                lines.append(
                    f"- concurrency {curve['concurrency']}: {curve['requests_per_second']:.2f} req/s, "
                    f"p50 {latency['p50']:.3f} ms, p95 {latency['p95']:.3f} ms"
                )
            lines.append("")
        lines.extend(["## Phrust / PHP ratios", ""])
        for ratio in report["ratios"]:
            lines.append(
                f"- concurrency {ratio['concurrency']}: p50 latency "
                f"{format_ratio(ratio['phrust_to_php_p50_latency'])} "
                f"(95% bootstrap {format_interval(ratio['phrust_to_php_p50_latency_ci95'])}), p95 latency "
                f"{format_ratio(ratio['phrust_to_php_p95_latency'])} "
                f"(95% bootstrap {format_interval(ratio['phrust_to_php_p95_latency_ci95'])}), throughput "
                f"{format_ratio(ratio['phrust_to_php_requests_per_second'])}"
            )
        lines.append("")
        if report.get("baseline_comparisons"):
            lines.extend(["## Phrust baseline comparison", ""])
            for comparison in report["baseline_comparisons"]:
                improvement = comparison["phrust_p50_improvement_pct"]
                improvement_text = "unavailable" if improvement is None else f"{improvement:.2f}%"
                lines.append(
                    f"- concurrency {comparison['concurrency']}: p50 improvement "
                    f"{improvement_text}, baseline/current latency "
                    f"{format_ratio(comparison['baseline_to_current_p50_latency'])} "
                    f"(95% bootstrap "
                    f"{format_interval(comparison['baseline_to_current_p50_latency_ci95'])})"
                )
            lines.append("")
        if report.get("php_control_comparisons"):
            lines.extend(["## PHP-FPM load control", ""])
            for comparison in report["php_control_comparisons"]:
                delta = comparison["php_p50_delta_pct"]
                delta_text = "unavailable" if delta is None else f"{delta:+.2f}%"
                lines.append(
                    f"- concurrency {comparison['concurrency']}: p50 "
                    f"{format_milliseconds(comparison['baseline_p50_ms'])} -> "
                    f"{format_milliseconds(comparison['current_p50_ms'])} ({delta_text})"
                )
            lines.append("")
    if report.get("host_checks"):
        lines.extend(["## Host idle evidence", ""])
        for snapshot in report["host_checks"]:
            lines.append(
                f"- `{snapshot['label']}`: load {snapshot['load_average']}; "
                f"blockers {len(snapshot['blockers']) + len(snapshot.get('interval_cpu_blockers', []))}; "
                f"ambient observations {len(snapshot.get('ambient_cpu_observations', []))}"
            )
        lines.append("")
    lines.extend(["## Artifacts", ""])
    for name, value in report.get("artifacts", {}).items():
        lines.append(f"- `{name}`: `{value}`")
    path.write_text("\n".join(lines).rstrip() + "\n", encoding="utf-8")


def write_feedback_ab_markdown(report: dict[str, Any], path: Path) -> None:
    lines = [
        "# WordPress persistent-feedback A/B",
        "",
        f"Status: `{report['status']}`",
        "",
        "Ratios above 1.0 favor persistent feedback: off/on latency and on/off throughput.",
        "",
    ]
    for failure in report.get("failures", []):
        lines.append(f"- {failure}")
    for comparison in report.get("comparison", []):
        lines.append(
            f"- concurrency {comparison['concurrency']}: p50 latency "
            f"{format_ratio(comparison['off_to_on_p50_latency'])}, p95 latency "
            f"{format_ratio(comparison['off_to_on_p95_latency'])} "
            f"(95% bootstrap {format_interval(comparison['off_to_on_p95_latency_ci95'])}), "
            f"throughput {format_ratio(comparison['on_to_off_requests_per_second'])}"
        )
    if not report.get("comparison") and not report.get("failures"):
        lines.append("- No comparable clean timing curves were produced.")
    lines.extend(["", "## Arms", ""])
    for arm, details in report.get("arms", {}).items():
        lines.append(
            f"- feedback {arm}: {details['status']} "
            f"(`{details['summary_markdown']}`)"
        )
    path.write_text("\n".join(lines).rstrip() + "\n", encoding="utf-8")


def format_ratio(value: float | None) -> str:
    return "unsupported" if value is None else f"{value:.3f}x"


def format_milliseconds(value: float | None) -> str:
    return "unavailable" if value is None else f"{value:.3f} ms"


def format_interval(value: list[float] | None) -> str:
    if value is None:
        return "unsupported"
    return f"{value[0]:.3f}x–{value[1]:.3f}x"


def rel(path: Path) -> str:
    try:
        return str(path.relative_to(REPO_ROOT))
    except ValueError:
        return str(path)


def self_test() -> int:
    assert parse_cpu_time("1326:47.79") == 79607.79
    assert parse_cpu_time("1-02:03:04.50") == 93784.5
    idle_process = {
        "pid": 7,
        "ppid": 1,
        "cpu_pct": 90.0,
        "cpu_seconds": 10.0,
        "memory_pct": 1.0,
        "elapsed": "00:10",
        "command": "background-worker",
    }
    host_before = {
        "label": "preflight-before",
        "captured_monotonic_seconds": 1.0,
        "blockers": [],
        "cpu_excluded_pids": [],
        "interval_cpu_blockers": [],
        "processes": [idle_process],
        "top_processes": [idle_process],
    }
    host_after = copy.deepcopy(host_before)
    host_after.update(label="preflight-after", captured_monotonic_seconds=2.0)
    assert not host_check_failures([host_before, host_after])
    host_after["processes"][0]["cpu_seconds"] = 10.3
    assert "pid 7 at 30.0% interval CPU" in " ".join(
        host_check_failures([host_before, host_after])
    )
    next_arm = copy.deepcopy(host_after)
    host_after["label"] = "phrust-timing-after"
    next_arm.update(label="php-fpm-timing-before", captured_monotonic_seconds=3.0)
    next_arm["processes"][0]["cpu_seconds"] = 11.0
    assert not host_check_failures([host_after, next_arm])
    ambient_after = copy.deepcopy(host_after)
    ambient_after["captured_monotonic_seconds"] = 3.0
    ambient_after["processes"][0].update(
        command=(
            "/System/Library/PrivateFrameworks/SkyLight.framework/Resources/"
            "WindowServer -daemon"
        ),
        ppid=1,
        cpu_seconds=11.0,
    )
    host_after["processes"][0]["command"] = ambient_after["processes"][0]["command"]
    assert not host_check_failures([host_after, ambient_after])
    assert len(ambient_after["ambient_cpu_observations"]) == 1
    assert percentile([1.0, 2.0, 3.0, 4.0], 50) == 2.0
    assert percentile(list(map(float, range(1, 101))), 95) == 95.0
    interval = bootstrap_percentile_ci([1.0, 2.0, 3.0, 4.0], 50, seed=7, iterations=100)
    assert interval is not None and interval[0] <= 2.0 <= interval[1]
    assert "fastcgi_param SERVER_NAME $host;" in render_nginx_config(
        Path("/tmp/wordpress"), "127.0.0.1:9000", 8080
    )
    assert "listen = 0.0.0.0:9000" in render_fpm_config("0.0.0.0", 9000, 4)
    assert normalize_headers([("Date", "today"), ("Content-Type", " text/html ")]) == [["content-type", "text/html"]]
    (REPO_ROOT / "target").mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(dir=REPO_ROOT / "target") as temporary:
        wordpress = Path(temporary)
        (wordpress / "wp-includes").mkdir()
        (wordpress / "wp-includes/version.php").write_text(
            "<?php $wp_version = '8.5-test';\n", encoding="utf-8"
        )
        first_identity = wordpress_identity(wordpress)
        assert first_identity["version"] == "8.5-test"
        assert first_identity["git_commit"] is None
        assert first_identity["file_count"] == 1
        (wordpress / "index.php").write_text("<?php echo 'changed';\n", encoding="utf-8")
        second_identity = wordpress_identity(wordpress)
        assert second_identity["tree_sha256"] != first_identity["tree_sha256"]
        assert second_identity["file_count"] == 2
    mismatch = compare_observables(
        {"root": {"status": 200, "headers": [], "body_sha256": "a"}},
        {"root": {"status": 200, "headers": [], "body_sha256": "b"}},
    )
    assert mismatch == ["observable 'root' body_sha256 differs between Phrust and PHP-FPM"]
    redirect_samples = {
        "phrust": {"status": 301},
        "php-fpm": {"status": 301},
    }
    assert not validate_benchmark_path_status(redirect_samples, strict=False)
    assert validate_benchmark_path_status(redirect_samples, strict=True) == [
        "phrust benchmark path returned HTTP 301; strict clean timing requires HTTP 200",
        "php-fpm benchmark path returned HTTP 301; strict clean timing requires HTTP 200",
    ]
    strict_missing = parse_args(["--strict", "--phrust-url", "http://127.0.0.1:1"])
    assert "missing reference PHP-FPM" in " ".join(validate_configuration(strict_missing))
    insufficient = parse_args(["--strict", "--samples", "29"])
    assert "at least 30" in " ".join(validate_configuration(insufficient))
    assert not is_release_binary(REPO_ROOT / "target/debug/phrust-server")
    assert is_release_binary(REPO_ROOT / "target/release/phrust-server")
    diagnostic = parse_args(["--mode", "diagnostic", "--samples", "1", "--concurrency", "1"])
    assert not validate_configuration(diagnostic)
    invalid_ab = parse_args(["--mode", "diagnostic", "--feedback-ab"])
    assert "requires --mode clean" in " ".join(validate_configuration(invalid_ab))
    assert cranelift_dependency_version()
    source_abi = native_source_abi_identity()
    assert source_abi["version"] == 18
    assert source_abi["hash"] == 0x0DC1_A818_0000_0027
    host_cpu = cpu_identity()
    assert len(host_cpu["feature_fingerprint_sha256"]) == 64
    ab_off = {
        "engines": {
            "phrust": {
                "curves": [{
                    "concurrency": 1,
                    "latency_ms": {"p50": 10.0, "p95": 20.0},
                    "requests_per_second": 100.0,
                    "samples": [{"wall_ms": 10.0}, {"wall_ms": 20.0}],
                }]
            }
        }
    }
    ab_on = copy.deepcopy(ab_off)
    ab_on["engines"]["phrust"]["curves"][0].update(
        latency_ms={"p50": 5.0, "p95": 10.0}, requests_per_second=200.0
    )
    ab_on["engines"]["phrust"]["curves"][0]["samples"] = [
        {"wall_ms": 5.0},
        {"wall_ms": 10.0},
    ]
    ab_comparison = build_feedback_ab_ratios(ab_off, ab_on)
    assert ab_comparison[0]["off_to_on_p95_latency"] == 2.0
    assert ab_comparison[0]["on_to_off_requests_per_second"] == 2.0
    tranche_baseline = {
        "engines": {
            "phrust": {
                "curves": [{
                    "concurrency": 1,
                    "latency_ms": {"p50": 100.0, "p95": 120.0},
                    "samples": [{"wall_ms": 100.0}, {"wall_ms": 120.0}],
                }]
            }
        }
    }
    tranche_current = [{
        "concurrency": 1,
        "latency_ms": {"p50": 96.0, "p95": 121.0},
        "samples": [{"wall_ms": 96.0}, {"wall_ms": 121.0}],
    }]
    tranche_comparisons, tranche_failures = compare_baseline(
        tranche_current, tranche_baseline, 20.0, 3.0
    )
    assert not tranche_failures
    assert tranche_comparisons[0]["phrust_p50_improvement_pct"] == 4.0
    tranche_current[0]["latency_ms"]["p50"] = 98.0
    _, tranche_failures = compare_baseline(
        tranche_current, tranche_baseline, 20.0, 3.0
    )
    assert tranche_failures == [
        "Phrust p50 at concurrency 1 improved by 2.0%; required 3.0%"
    ]
    tranche_baseline["engines"]["php-fpm"] = {
        "curves": [{"concurrency": 1, "latency_ms": {"p50": 40.0}}]
    }
    php_control, php_failures = compare_php_control(
        [{"concurrency": 1, "latency_ms": {"p50": 43.0}}],
        tranche_baseline,
        10.0,
    )
    assert not php_failures
    assert php_control[0]["php_p50_delta_pct"] == 7.5
    _, php_failures = compare_php_control(
        [{"concurrency": 1, "latency_ms": {"p50": 48.0}}],
        tranche_baseline,
        10.0,
    )
    assert php_failures == [
        "PHP-FPM p50 control at concurrency 1 drifted by +20.0%; allowed +/-10.0%"
    ]
    assert build_performance_decision(tranche_baseline, php_failures, ["too slow"]) == {
        "eligible": False,
        "status": "inconclusive",
        "reasons": php_failures,
    }
    assert build_performance_decision(tranche_baseline, [], ["too slow"])["status"] == "revert"
    assert build_performance_decision(tranche_baseline, [], [])["status"] == "keep"
    assert build_performance_decision(None, [], [])["status"] == "baseline_only"
    invalid_tranche_gate = validate_configuration(
        parse_args(["--min-c1-p50-improvement-pct", "3"])
    )
    assert "requires --baseline" in " ".join(invalid_tranche_gate)
    invalid_php_control = validate_configuration(
        parse_args(["--max-php-control-p50-delta-pct", "10"])
    )
    assert "requires --baseline" in " ".join(invalid_php_control)
    previous_trace = os.environ.get("PHRUST_PERF_TRACE")
    os.environ["PHRUST_PERF_TRACE"] = "target/forbidden-clean-trace.jsonl"
    try:
        instrumented_clean = validate_configuration(parse_args(["--samples", "30"]))
        assert "clean timing rejects Phrust instrumentation environment" in " ".join(
            instrumented_clean
        )
    finally:
        if previous_trace is None:
            os.environ.pop("PHRUST_PERF_TRACE", None)
        else:
            os.environ["PHRUST_PERF_TRACE"] = previous_trace
    skipped = unavailable_report(["missing WordPress"], parse_args([]), REPO_ROOT / "target")
    assert skipped["status"] == "skip"
    strict_skip = unavailable_report(["missing WordPress"], parse_args(["--strict"]), REPO_ROOT / "target")
    assert strict_skip["status"] == "fail"
    print("[pass] wordpress_root_benchmark self-test")
    return 0


if __name__ == "__main__":
    sys.exit(main())
