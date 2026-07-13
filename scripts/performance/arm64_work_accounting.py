#!/usr/bin/env python3
"""Orchestrate external ARM64 work-accounting samples for warm WordPress."""

from __future__ import annotations

import argparse
import hashlib
import http.client
import json
import os
import platform
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Any

SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parents[1]
sys.path.insert(0, str(SCRIPT_DIR))

from arm64_sample_parser import require_arm64, write_json  # noqa: E402
from arm64_sampler import run as run_sampler  # noqa: E402
from wordpress_root_benchmark import (  # noqa: E402
    HostIdleMonitor,
    host_check_failures,
)


FORBIDDEN_ENV = (
    "PHRUST_PERF_TRACE",
    "PHRUST_SERVER_PERF_TRACE_VM_COUNTERS",
    "PHRUST_REQUEST_PROFILE",
    "PHRUST_REQUEST_PROFILE_VM_COUNTERS",
    "PHRUST_REQUEST_PROFILE_SOURCE_ATTRIBUTION",
    "PHRUST_PERF_ABLATION",
    "PHRUST_SERVER_DEBUG",
    "PHRUST_SERVER_DEBUG_LOG",
)
PERFORMANCE_ENV = {
    "PHRUST_JIT_COPY_PATCH": "1",
    "PHRUST_INCLUDE_REVALIDATE_MS": "2000",
    "PHRUST_WORKER_SYMBOL_EPOCH": "1",
    "PHRUST_PERSISTENT_FEEDBACK": "1",
}


def command_output(command: list[str], *, check: bool = True) -> str:
    completed = subprocess.run(
        command,
        cwd=REPO_ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    if check and completed.returncode != 0:
        raise RuntimeError(f"command failed: {' '.join(command)}\n{completed.stderr}")
    return completed.stdout.strip()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def git_identity() -> dict[str, Any]:
    status = command_output(["git", "status", "--short"])
    patch = subprocess.run(
        ["git", "diff", "--binary", "HEAD"],
        cwd=REPO_ROOT,
        capture_output=True,
        check=False,
    ).stdout
    return {
        "commit": command_output(["git", "rev-parse", "HEAD"]),
        "status_short": status.splitlines(),
        "tracked_patch_sha256": hashlib.sha256(patch).hexdigest() if patch else None,
    }


def sysctl_value(name: str) -> str | None:
    completed = subprocess.run(
        ["sysctl", "-n", name], text=True, capture_output=True, check=False
    )
    value = completed.stdout.strip()
    return value or None


def relevant_environment(process_env: dict[str, str]) -> dict[str, str]:
    prefixes = ("PHRUST_", "RUST", "MALLOC", "DYLD_", "LLVM_")
    return {
        key: value
        for key, value in sorted(process_env.items())
        if key.startswith(prefixes)
    }


def identity(args: argparse.Namespace, binary: Path, process_env: dict[str, str]) -> dict[str, Any]:
    wordpress = args.wordpress_dir.resolve()
    version_file = wordpress / "wp-includes/version.php"
    return {
        "schema_version": 1,
        "git": git_identity(),
        "host": {
            "system": platform.system(),
            "release": platform.release(),
            "version": platform.version(),
            "machine": platform.machine(),
            "hardware_model": sysctl_value("hw.model"),
            "logical_cpu_count": os.cpu_count(),
            "physical_cpu_count": sysctl_value("hw.physicalcpu"),
            "kernel": command_output(["uname", "-a"]),
        },
        "wordpress": {
            "version": "6.8.3",
            "path": str(wordpress),
            "version_file_sha256": sha256_file(version_file) if version_file.is_file() else None,
            "database_identity": args.database_identity,
            "host_header": args.host_header,
            "request_path": args.path,
        },
        "phrust": {
            "binary": str(binary),
            "binary_sha256": sha256_file(binary),
            "build_profile": "profiling",
            "features": ["jit-copy-patch"],
            "deployment_mode": "immutable",
            "engine_preset": "default",
            "cpu_execution_limit": 1,
        },
        "environment": relevant_environment(process_env),
        "tooling": {
            path.name: sha256_file(path)
            for path in (
                Path(__file__).resolve(),
                SCRIPT_DIR / "arm64_sampler.py",
                SCRIPT_DIR / "arm64_sample_parser.py",
            )
        },
    }


def build_binary() -> Path:
    command = [
        "cargo",
        "build",
        "--profile",
        "profiling",
        "-p",
        "php_server",
        "--bin",
        "phrust-server",
        "--no-default-features",
        "--features",
        "jit-copy-patch",
    ]
    subprocess.run(command, cwd=REPO_ROOT, check=True)
    binary = REPO_ROOT / "target/profiling/phrust-server"
    if not binary.is_file():
        raise RuntimeError(f"profiling build did not create {binary}")
    if platform.system() == "Darwin" and shutil.which("dsymutil"):
        subprocess.run(["dsymutil", str(binary)], cwd=REPO_ROOT, check=False)
    return binary


def clean_process_env() -> dict[str, str]:
    process_env = os.environ.copy()
    active = [key for key in FORBIDDEN_ENV if process_env.get(key)]
    if active:
        raise RuntimeError("diagnostic/debug environment must be disabled: " + ", ".join(active))
    process_env.update(PERFORMANCE_ENV)
    for key in FORBIDDEN_ENV:
        process_env.pop(key, None)
    return process_env


def start_server(
    binary: Path,
    args: argparse.Namespace,
    process_env: dict[str, str],
    log_path: Path,
) -> tuple[subprocess.Popen[str], Any, str]:
    log = log_path.open("w+", encoding="utf-8")
    command = [
        str(binary),
        "--listen",
        "127.0.0.1:0",
        "--docroot",
        str(args.wordpress_dir),
        "--front-controller",
        "index.php",
        "--deployment-mode",
        "immutable",
        "--engine-preset",
        "default",
        "--cpu-execution-limit",
        "1",
    ]
    process = subprocess.Popen(
        command,
        cwd=REPO_ROOT,
        env=process_env,
        text=True,
        stdout=log,
        stderr=subprocess.STDOUT,
    )
    for _ in range(300):
        if process.poll() is not None:
            log.flush()
            log.seek(0)
            raise RuntimeError(f"Phrust exited during startup:\n{log.read()}")
        log.flush()
        log.seek(0)
        matches = re.findall(r"^listening http://(.+)$", log.read(), flags=re.MULTILINE)
        if matches:
            return process, log, f"http://{matches[-1].strip()}{args.path}"
        time.sleep(0.05)
    process.terminate()
    raise RuntimeError("Phrust did not report its listen address")


def request(url: str, host_header: str, timeout_seconds: float) -> int:
    match = re.match(r"^http://([^:/]+):(\d+)(/.*)$", url)
    if not match:
        raise ValueError(f"unsupported local URL {url}")
    connection = http.client.HTTPConnection(match.group(1), int(match.group(2)), timeout=timeout_seconds)
    connection.request("GET", match.group(3), headers={"Host": host_header})
    response = connection.getresponse()
    response.read()
    connection.close()
    return response.status


def stop_server(process: subprocess.Popen[str], log: Any) -> None:
    if process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)
    log.close()


def markdown_summary(report: dict[str, Any]) -> str:
    lines = [
        "# ARM64 External Sampler",
        "",
        f"Status: `{report['status']}`",
        "",
        f"- source commit: `{report['identity']['git']['commit']}`",
        f"- binary SHA-256: `{report['identity']['phrust']['binary_sha256']}`",
        f"- active php-worker-0 samples: `{report['sampler']['totals']['active_php_worker_0_samples']}`",
        f"- requests sampled: `{report['sampler']['totals']['requests']}`",
        f"- process CPU seconds: `{report['sampler']['totals']['process_cpu_seconds']:.3f}`",
        f"- host contamination: `{bool(report['host_failures'])}`",
        "",
        "| window | requests | process CPU s | CPU ms/request | active worker | idle worker | other threads |",
        "| ---: | ---: | ---: | ---: | ---: | ---: | ---: |",
    ]
    for window in report["sampler"]["windows"]:
        view = window["stack_view"]
        lines.append(
            f"| {window['window']} | {window['requests']} | "
            f"{window['process_cpu_seconds_delta']:.3f} | "
            f"{window['process_cpu_ms_per_request']:.2f} | "
            f"{view['active_php_worker_0_samples']} | "
            f"{view['idle_php_worker_0_samples']} | "
            f"{view['other_phrust_thread_samples']} |"
        )
    if report["host_failures"]:
        lines.extend(["", "## Host failures", ""])
        lines.extend(f"- {failure}" for failure in report["host_failures"])
    return "\n".join(lines) + "\n"


def run(args: argparse.Namespace) -> dict[str, Any]:
    require_arm64()
    if platform.system() != "Darwin":
        raise RuntimeError("P14 currently requires the completed macOS ARM64 backend")
    if not args.wordpress_dir.is_dir():
        raise RuntimeError(f"WordPress directory is missing: {args.wordpress_dir}")
    if not args.database_identity:
        raise RuntimeError("--database-identity is required")
    args.out_dir.mkdir(parents=True, exist_ok=True)
    sampler_dir = args.out_dir / "sampler"
    sampler_dir.mkdir(parents=True, exist_ok=True)
    binary = args.binary.resolve() if args.binary else build_binary()
    process_env = clean_process_env()
    run_identity = identity(args, binary, process_env)
    write_json(run_identity, args.out_dir / "identity.json")

    process: subprocess.Popen[str] | None = None
    log = None
    monitor: HostIdleMonitor | None = None
    try:
        process, log, url = start_server(
            binary, args, process_env, sampler_dir / "phrust-server.log"
        )
        warmup_statuses = [
            request(url, args.host_header, args.timeout_seconds) for _ in range(args.warmups)
        ]
        if any(status != 200 for status in warmup_statuses):
            raise RuntimeError(f"warmup returned non-200 statuses: {warmup_statuses}")
        monitor = HostIdleMonitor(
            "arm64-sampler",
            (process.pid,),
            allow_docker_runtime=True,
        )
        monitor.start()
        sampler_args = argparse.Namespace(
            pid=process.pid,
            url=url,
            host_header=args.host_header,
            binary=binary,
            out_dir=sampler_dir,
            windows=3,
            duration_seconds=args.duration_seconds,
            interval_milliseconds=1,
            minimum_active_samples=args.minimum_active_samples,
            timeout_seconds=args.timeout_seconds,
        )
        sampler = run_sampler(sampler_args)
        host_snapshots = monitor.stop()
        monitor = None
    finally:
        if monitor is not None:
            host_snapshots = monitor.stop()
        if process is not None and log is not None:
            stop_server(process, log)

    failures = host_check_failures(host_snapshots)
    status = "pass"
    if failures:
        status = "inconclusive"
    elif not sampler["stable_sample_target_met"]:
        status = "fail"
    elif any(set(window["http_status_counts"]) != {"200"} for window in sampler["windows"]):
        status = "fail"
    report = {
        "schema_version": 1,
        "status": status,
        "timing_eligible": False,
        "identity": run_identity,
        "warmup_statuses": warmup_statuses,
        "sampler": sampler,
        "host_snapshots": host_snapshots,
        "host_failures": failures,
    }
    write_json(report, sampler_dir / "sampler-summary.json")
    (sampler_dir / "sampler-summary.md").write_text(
        markdown_summary(report), encoding="utf-8"
    )
    return report


def self_test() -> int:
    with tempfile.TemporaryDirectory() as temporary:
        path = Path(temporary) / "identity.json"
        value = {
            "git": {"commit": "a" * 40, "tracked_patch_sha256": None},
            "host": {"machine": "arm64"},
            "phrust": {"features": ["jit-copy-patch"]},
        }
        write_json(value, path)
        assert json.loads(path.read_text(encoding="utf-8")) == value
    env = {"PHRUST_JIT_COPY_PATCH": "1", "RUST_LOG": "warn", "IGNORED": "x"}
    assert relevant_environment(env) == {
        "PHRUST_JIT_COPY_PATCH": "1",
        "RUST_LOG": "warn",
    }
    print("arm64 work accounting self-test: ok")
    return 0


def default_run_dir() -> Path:
    stamp = time.strftime("%Y%m%dT%H%M%S", time.gmtime())
    return REPO_ROOT / "target/performance/arm64-work-accounting" / stamp


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--wordpress-dir",
        type=Path,
        default=Path(os.environ.get("PHRUST_WORDPRESS_DIR", "target/wordpress-benchmark/wp-6.8.3")),
    )
    parser.add_argument("--database-identity", default=os.environ.get("PHRUST_WORDPRESS_DB_IDENTITY", ""))
    parser.add_argument("--host-header", default=os.environ.get("PHRUST_WORDPRESS_HOST_HEADER", "wordpress.local"))
    parser.add_argument("--path", default="/")
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--out-dir", type=Path, default=default_run_dir())
    parser.add_argument("--warmups", type=int, default=5)
    parser.add_argument("--duration-seconds", type=int, default=20)
    parser.add_argument("--minimum-active-samples", type=int, default=10_000)
    parser.add_argument("--timeout-seconds", type=float, default=30.0)
    parser.add_argument("--self-test", action="store_true")
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    if args.self_test:
        return self_test()
    args.wordpress_dir = (REPO_ROOT / args.wordpress_dir).resolve() if not args.wordpress_dir.is_absolute() else args.wordpress_dir.resolve()
    args.out_dir = (REPO_ROOT / args.out_dir).resolve() if not args.out_dir.is_absolute() else args.out_dir.resolve()
    report = run(args)
    print(f"[{report['status']}] wrote {args.out_dir / 'sampler/sampler-summary.md'}")
    return 0 if report["status"] == "pass" else 2


if __name__ == "__main__":
    sys.exit(main())
