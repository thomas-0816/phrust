#!/usr/bin/env python3
"""Reusable clean HTTP sampling and Linux process measurement helpers.

This module deliberately has no WordPress or Phrust knowledge.  Application
benchmarks provide URLs and correctness observations; this module supplies the
request sampler and optional local-process CPU/RSS accounting.
"""

from __future__ import annotations

import hashlib
import http.client
import math
import os
import random
import threading
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable
from urllib.parse import urlparse


VOLATILE_RESPONSE_HEADERS = frozenset(
    {
        "connection",
        "content-length",
        "date",
        "keep-alive",
        "server",
        "transfer-encoding",
        "x-powered-by",
    }
)


@dataclass(frozen=True)
class HttpTarget:
    name: str
    base_url: str
    host_header: str = ""
    pids: tuple[int, ...] = ()


def http_get(
    target: HttpTarget,
    path: str,
    timeout_seconds: float,
    extra_headers: dict[str, str] | None = None,
) -> dict[str, Any]:
    parsed = urlparse(target.base_url)
    if parsed.scheme != "http":
        raise ValueError(f"only http:// benchmark targets are supported: {target.base_url}")
    host = parsed.hostname or "127.0.0.1"
    port = parsed.port or 80
    base_path = parsed.path.rstrip("/")
    request_path = f"{base_path}/{path.lstrip('/')}" or "/"
    headers = {"Host": target.host_header or parsed.netloc or host}
    headers.update(extra_headers or {})
    started_ns = time.perf_counter_ns()
    connection = http.client.HTTPConnection(host, port, timeout=timeout_seconds)
    response_started_ns = started_ns
    try:
        connection.request("GET", request_path, headers=headers)
        response = connection.getresponse()
        response_started_ns = time.perf_counter_ns()
        body = response.read()
        response_headers = normalize_headers(response.getheaders())
    finally:
        connection.close()
    finished_ns = time.perf_counter_ns()
    return {
        "path": request_path,
        "status": response.status,
        "headers": response_headers,
        "body_bytes": len(body),
        "body_sha256": hashlib.sha256(body).hexdigest(),
        "ttfb_ms": (response_started_ns - started_ns) / 1_000_000.0,
        "wall_ms": (finished_ns - started_ns) / 1_000_000.0,
    }


def normalize_headers(headers: Iterable[tuple[str, str]]) -> list[list[str]]:
    normalized = [
        [name.strip().lower(), " ".join(value.strip().split())]
        for name, value in headers
        if name.strip().lower() not in VOLATILE_RESPONSE_HEADERS
    ]
    normalized.sort()
    return normalized


def sample_curve(
    target: HttpTarget,
    path: str,
    concurrency: int,
    samples: int,
    timeout_seconds: float,
) -> dict[str, Any]:
    if concurrency < 1:
        raise ValueError("concurrency must be at least 1")
    if samples < concurrency:
        raise ValueError(
            f"samples ({samples}) must be at least concurrency ({concurrency})"
        )
    monitor = ProcessMonitor(target.pids)
    monitor.start()
    started_ns = time.perf_counter_ns()
    results: list[dict[str, Any]] = []
    failures: list[str] = []
    try:
        with ThreadPoolExecutor(max_workers=concurrency) as executor:
            futures = [
                executor.submit(http_get, target, path, timeout_seconds)
                for _ in range(samples)
            ]
            for future in as_completed(futures):
                try:
                    results.append(future.result())
                except Exception as error:  # individual failures belong in the report
                    failures.append(str(error))
    finally:
        elapsed_seconds = (time.perf_counter_ns() - started_ns) / 1_000_000_000.0
        process = monitor.stop()
    walls = sorted(float(sample["wall_ms"]) for sample in results)
    ttfbs = sorted(float(sample["ttfb_ms"]) for sample in results)
    return {
        "concurrency": concurrency,
        "requested_samples": samples,
        "completed_samples": len(results),
        "failures": failures,
        "elapsed_seconds": elapsed_seconds,
        "requests_per_second": len(results) / elapsed_seconds if elapsed_seconds else 0.0,
        "latency_ms": {
            "p50": percentile(walls, 50),
            "p95": percentile(walls, 95),
            "p99": percentile(walls, 99),
            "p50_ci95": bootstrap_percentile_ci(walls, 50, seed=concurrency * 101 + samples),
            "p95_ci95": bootstrap_percentile_ci(walls, 95, seed=concurrency * 103 + samples),
            "min": walls[0] if walls else None,
            "max": walls[-1] if walls else None,
        },
        "ttfb_ms": {
            "p50": percentile(ttfbs, 50),
            "p95": percentile(ttfbs, 95),
        },
        "process": process,
        "samples": results,
    }


def percentile(values: list[float], requested: float) -> float | None:
    """Return a deterministic nearest-rank percentile."""
    if not values:
        return None
    rank = max(1, math.ceil(requested * len(values) / 100.0))
    return values[min(rank - 1, len(values) - 1)]


def bootstrap_percentile_ci(
    values: list[float],
    requested: float,
    *,
    seed: int,
    iterations: int = 2_000,
) -> list[float] | None:
    """Return a deterministic non-parametric 95% bootstrap interval."""
    if not values:
        return None
    generator = random.Random(seed)
    estimates = []
    for _ in range(iterations):
        sample = sorted(generator.choice(values) for _ in values)
        estimate = percentile(sample, requested)
        assert estimate is not None
        estimates.append(estimate)
    estimates.sort()
    lower = percentile(estimates, 2.5)
    upper = percentile(estimates, 97.5)
    assert lower is not None and upper is not None
    return [lower, upper]


def bootstrap_percentile_ratio_ci(
    numerator: list[float],
    denominator: list[float],
    requested: float,
    *,
    seed: int,
    iterations: int = 2_000,
) -> list[float] | None:
    """Bootstrap an independent-sample percentile ratio at 95% confidence."""
    if not numerator or not denominator:
        return None
    generator = random.Random(seed)
    estimates = []
    for _ in range(iterations):
        left = sorted(generator.choice(numerator) for _ in numerator)
        right = sorted(generator.choice(denominator) for _ in denominator)
        left_value = percentile(left, requested)
        right_value = percentile(right, requested)
        if left_value is not None and right_value not in (None, 0):
            estimates.append(left_value / right_value)
    if not estimates:
        return None
    estimates.sort()
    lower = percentile(estimates, 2.5)
    upper = percentile(estimates, 97.5)
    assert lower is not None and upper is not None
    return [lower, upper]


class ProcessMonitor:
    """Poll a Linux process tree for CPU time and aggregate peak RSS."""

    def __init__(self, pids: Iterable[int]) -> None:
        self.pids = tuple(pid for pid in pids if Path(f"/proc/{pid}").exists())
        self._stop = threading.Event()
        self._thread: threading.Thread | None = None
        self._before_cpu = 0.0
        self._peak_rss = 0

    def start(self) -> None:
        if not self.pids:
            return
        self._before_cpu, rss = process_forest_sample(self.pids)
        self._peak_rss = rss
        self._thread = threading.Thread(target=self._poll, daemon=True)
        self._thread.start()

    def _poll(self) -> None:
        # A 20 Hz sampler catches sustained server RSS growth without turning
        # `/proc` traversal into a competing CPU workload during short curves.
        while not self._stop.wait(0.05):
            _, rss = process_forest_sample(self.pids)
            self._peak_rss = max(self._peak_rss, rss)

    def stop(self) -> dict[str, Any]:
        if not self.pids:
            return {
                "supported": False,
                "reason": "target is remote or /proc is unavailable",
                "cpu_seconds": None,
                "peak_rss_bytes": None,
            }
        self._stop.set()
        if self._thread is not None:
            self._thread.join(timeout=1)
        after_cpu, rss = process_forest_sample(self.pids)
        self._peak_rss = max(self._peak_rss, rss)
        return {
            "supported": True,
            "cpu_seconds": max(0.0, after_cpu - self._before_cpu),
            "peak_rss_bytes": self._peak_rss,
        }


def process_forest_sample(root_pids: Iterable[int]) -> tuple[float, int]:
    proc = Path("/proc")
    if not proc.is_dir():
        return 0.0, 0
    stats: dict[int, tuple[int, int, int]] = {}
    for entry in proc.iterdir():
        if not entry.name.isdigit():
            continue
        try:
            text = (entry / "stat").read_text(encoding="utf-8")
            # The command field may contain spaces and parentheses; fields
            # after the final ')' begin with state and parent PID.
            fields = text[text.rfind(")") + 2 :].split()
            pid = int(entry.name)
            ppid = int(fields[1])
            ticks = int(fields[11]) + int(fields[12])
            rss_pages = int(fields[21])
            stats[pid] = (ppid, ticks, rss_pages)
        except (OSError, ValueError, IndexError):
            continue
    selected = set(root_pids)
    changed = True
    while changed:
        changed = False
        for pid, (ppid, _, _) in stats.items():
            if ppid in selected and pid not in selected:
                selected.add(pid)
                changed = True
    ticks_per_second = os.sysconf("SC_CLK_TCK")
    page_size = os.sysconf("SC_PAGE_SIZE")
    cpu = sum(stats[pid][1] for pid in selected if pid in stats) / ticks_per_second
    rss = sum(stats[pid][2] for pid in selected if pid in stats) * page_size
    return cpu, rss
