#!/usr/bin/env python3
"""Capture external macOS ARM64 sample windows for one Phrust process."""

from __future__ import annotations

import argparse
import http.client
import json
import os
import platform
import subprocess
import sys
import threading
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any
from urllib.parse import urlsplit

from arm64_sample_parser import parse_file, require_arm64, write_json


IDLE_SYMBOLS = (
    "semaphore_wait_trap",
    "__psynch_cvwait",
    "__psynch_mutexwait",
    "std::sync::mpmc",
    "Receiver::recv",
)


@dataclass
class SequentialLoad:
    url: str
    host_header: str
    timeout_seconds: float
    stop_event: threading.Event = field(default_factory=threading.Event)
    statuses: list[int] = field(default_factory=list)
    wall_ms: list[float] = field(default_factory=list)
    error: str | None = None
    thread: threading.Thread | None = None

    def start(self) -> None:
        self.thread = threading.Thread(target=self._run, name="arm64-sample-load", daemon=True)
        self.thread.start()

    def stop(self) -> None:
        self.stop_event.set()
        if self.thread is not None:
            self.thread.join(timeout=self.timeout_seconds + 2.0)
            if self.thread.is_alive():
                raise RuntimeError("sequential load thread did not stop")
        if self.error:
            raise RuntimeError(self.error)

    def _run(self) -> None:
        target = urlsplit(self.url)
        path = target.path or "/"
        if target.query:
            path += f"?{target.query}"
        try:
            while not self.stop_event.is_set():
                started = time.perf_counter_ns()
                connection = http.client.HTTPConnection(
                    target.hostname,
                    target.port,
                    timeout=self.timeout_seconds,
                )
                connection.request("GET", path, headers={"Host": self.host_header})
                response = connection.getresponse()
                response.read()
                connection.close()
                self.statuses.append(response.status)
                self.wall_ms.append((time.perf_counter_ns() - started) / 1_000_000.0)
        except Exception as error:  # surfaced synchronously by stop()
            self.error = f"sequential load failed: {error}"


def process_cpu_seconds(pid: int) -> float:
    completed = subprocess.run(
        ["ps", "-o", "time=", "-p", str(pid)],
        text=True,
        capture_output=True,
        check=False,
    )
    value = completed.stdout.strip()
    if completed.returncode != 0 or not value:
        raise RuntimeError(f"cannot read CPU time for pid {pid}")
    return parse_cpu_time(value)


def parse_cpu_time(value: str) -> float:
    days = 0
    if "-" in value:
        day_text, value = value.split("-", 1)
        days = int(day_text)
    fields = value.split(":")
    if len(fields) == 2:
        hours = 0
        minutes, seconds = fields
    elif len(fields) == 3:
        hours, minutes, seconds = fields
    else:
        raise ValueError(f"unsupported CPU time {value!r}")
    return days * 86400 + int(hours) * 3600 + int(minutes) * 60 + float(seconds)


def stack_view(report: dict[str, Any]) -> dict[str, Any]:
    active_worker = 0
    idle_worker = 0
    other_threads = 0
    unresolved = 0
    for stack in report["stacks"]:
        weight = int(stack["weight"])
        name = stack.get("thread_name") or ""
        folded = stack["folded"]
        if stack["unresolved"]:
            unresolved += weight
        if name == "php-worker-0":
            if any(symbol in folded for symbol in IDLE_SYMBOLS):
                idle_worker += weight
            else:
                active_worker += weight
        else:
            other_threads += weight
    return {
        "active_php_worker_0_samples": active_worker,
        "idle_php_worker_0_samples": idle_worker,
        "other_phrust_thread_samples": other_threads,
        "unresolved_samples": unresolved,
    }


def capture_window(args: argparse.Namespace, index: int) -> dict[str, Any]:
    stem = f"window-{index:02d}"
    raw_path = args.out_dir / f"{stem}.raw"
    folded_path = args.out_dir / f"{stem}.folded"
    load = SequentialLoad(args.url, args.host_header, args.timeout_seconds)
    cpu_before = process_cpu_seconds(args.pid)
    wall_before = time.monotonic()
    load.start()
    command = [
        "/usr/bin/sample",
        str(args.pid),
        str(args.duration_seconds),
        str(args.interval_milliseconds),
        "-file",
        str(raw_path),
    ]
    completed = subprocess.run(command, text=True, capture_output=True, check=False)
    wall_after = time.monotonic()
    cpu_after = process_cpu_seconds(args.pid)
    completed_requests = len(load.statuses)
    load.stop_event.set()
    load.stop()
    if completed.returncode != 0:
        raise RuntimeError(
            f"sample window {index} failed ({completed.returncode}): "
            f"{completed.stdout}{completed.stderr}"
        )
    if not raw_path.is_file():
        raise RuntimeError(f"sample did not create {raw_path}")
    parsed = parse_file(raw_path, args.binary)
    write_json(parsed, folded_path)
    view = stack_view(parsed)
    requests = completed_requests
    return {
        "window": index,
        "raw": str(raw_path),
        "folded": str(folded_path),
        "command": command,
        "wall_seconds": wall_after - wall_before,
        "process_cpu_seconds_before": cpu_before,
        "process_cpu_seconds_after": cpu_after,
        "process_cpu_seconds_delta": max(0.0, cpu_after - cpu_before),
        "requests": requests,
        "process_cpu_ms_per_request": (
            max(0.0, cpu_after - cpu_before) * 1000.0 / requests if requests else None
        ),
        "http_status_counts": {
            str(status): load.statuses[:requests].count(status)
            for status in sorted(set(load.statuses[:requests]))
        },
        "request_wall_ms": load.wall_ms[:requests],
        "stack_view": view,
        "raw_stack_weight_total": parsed["stack_weight_total"],
    }


def run(args: argparse.Namespace) -> dict[str, Any]:
    require_arm64()
    if platform.system() != "Darwin":
        raise RuntimeError(f"macOS sampling backend required; got {platform.system()}")
    if not Path("/usr/bin/sample").is_file():
        raise RuntimeError("/usr/bin/sample is unavailable")
    args.out_dir.mkdir(parents=True, exist_ok=True)
    windows = [capture_window(args, index) for index in range(1, args.windows + 1)]
    active = sum(window["stack_view"]["active_php_worker_0_samples"] for window in windows)
    return {
        "schema_version": 1,
        "backend": "macos-/usr/bin/sample",
        "pid": args.pid,
        "binary": str(args.binary),
        "interval_milliseconds": args.interval_milliseconds,
        "duration_seconds_per_window": args.duration_seconds,
        "windows": windows,
        "totals": {
            "active_php_worker_0_samples": active,
            "idle_php_worker_0_samples": sum(
                window["stack_view"]["idle_php_worker_0_samples"] for window in windows
            ),
            "other_phrust_thread_samples": sum(
                window["stack_view"]["other_phrust_thread_samples"] for window in windows
            ),
            "requests": sum(window["requests"] for window in windows),
            "process_cpu_seconds": sum(window["process_cpu_seconds_delta"] for window in windows),
        },
        "stable_sample_target_met": active >= args.minimum_active_samples,
    }


def self_test() -> int:
    assert parse_cpu_time("01:02") == 62.0
    assert parse_cpu_time("01:02:03") == 3723.0
    assert parse_cpu_time("2-01:02:03") == 176523.0
    fixture = Path(__file__).resolve().parent / "fixtures/arm64_sample/idle.sample"
    view = stack_view(parse_file(fixture))
    assert view["idle_php_worker_0_samples"] == 3
    assert view["active_php_worker_0_samples"] == 0
    print("arm64 sampler self-test: ok")
    return 0


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pid", type=int)
    parser.add_argument("--url")
    parser.add_argument("--host-header", default="wordpress.local")
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--out-dir", type=Path)
    parser.add_argument("--windows", type=int, default=3)
    parser.add_argument("--duration-seconds", type=int, default=20)
    parser.add_argument("--interval-milliseconds", type=int, default=1)
    parser.add_argument("--minimum-active-samples", type=int, default=10_000)
    parser.add_argument("--timeout-seconds", type=float, default=30.0)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)
    if not args.self_test:
        missing = [name for name in ("pid", "url", "binary", "out_dir") if getattr(args, name) is None]
        if missing:
            parser.error("required arguments: " + ", ".join(f"--{name.replace('_', '-')}" for name in missing))
    return args


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    if args.self_test:
        return self_test()
    report = run(args)
    write_json(report, args.out_dir / "sampler-summary.json")
    if not report["stable_sample_target_met"]:
        print(
            f"insufficient active samples: {report['totals']['active_php_worker_0_samples']}",
            file=sys.stderr,
        )
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
