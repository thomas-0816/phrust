#!/usr/bin/env python3
"""Profiler containment gate.

Runs three request batches against one long-running server process:
  A: unprofiled requests (no profile header)
  B: profiled requests (x-phrust-request-profile: 1)
  C: unprofiled requests again

The gate fails when batch C regresses more than the threshold against batch
A (profiling state leaked into unprofiled execution), or when the server
wrote request-profile artifacts for unprofiled batches.

Modes:
  - PHRUST_WORDPRESS_URL: drive an already-running server (started with
    --request-profile <dir> --request-profile-trigger-header). Profile-file
    accounting is skipped when the directory is not locally readable.
  - default: spawn a local phrust-server on the committed
    fixtures/server/apps/front-controller-hotpath app. This validates the
    containment mechanism itself; WordPress-level claims stay BLOCKED until
    the real root environment is used.
"""

from __future__ import annotations

import json
import os
import statistics
import subprocess
import sys
import time
import urllib.request
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
OUT_DIR = REPO_ROOT / "target" / "performance" / "profiler-overhead"
THRESHOLD = float(os.environ.get("PHRUST_PROFILER_OVERHEAD_THRESHOLD", "1.05"))
BATCH = int(os.environ.get("PHRUST_PROFILER_OVERHEAD_BATCH", "12"))
WARMUP = int(os.environ.get("PHRUST_PROFILER_OVERHEAD_WARMUPS", "4"))


def fetch(url: str, host_header: str | None, profile: bool, timeout: float = 300.0) -> float:
    request = urllib.request.Request(url)
    if host_header:
        request.add_header("Host", host_header)
    if profile:
        request.add_header("x-phrust-request-profile", "1")
    started = time.perf_counter()
    with urllib.request.urlopen(request, timeout=timeout) as response:
        response.read()
        if response.status != 200:
            raise RuntimeError(f"unexpected status {response.status}")
    return (time.perf_counter() - started) * 1000.0


def run_batch(url: str, host_header: str | None, profile: bool, count: int) -> list[float]:
    return [fetch(url, host_header, profile) for _ in range(count)]


def profile_file_count(profile_dir: Path | None) -> int | None:
    if profile_dir is None or not profile_dir.is_dir():
        return None
    return len(list(profile_dir.glob("*.json")))


def spawn_local_server(profile_dir: Path) -> tuple[subprocess.Popen, str]:
    server = os.environ.get(
        "PHRUST_SERVER",
        str(REPO_ROOT / "target" / "release" / "phrust-server"),
    )
    if not Path(server).exists():
        fallback = REPO_ROOT / "target" / "debug" / "phrust-server"
        if fallback.exists():
            server = str(fallback)
        else:
            raise FileNotFoundError(f"phrust-server binary not found: {server}")
    docroot = REPO_ROOT / "fixtures" / "server" / "apps" / "front-controller-hotpath" / "public"
    port = int(os.environ.get("PHRUST_PROFILER_OVERHEAD_PORT", "18099"))
    process = subprocess.Popen(
        [
            server,
            "--docroot",
            str(docroot),
            "--listen",
            f"127.0.0.1:{port}",
            "--request-profile",
            str(profile_dir),
            "--request-profile-trigger-header",
            "--request-profile-vm-counters",
        ],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        cwd=REPO_ROOT,
    )
    url = f"http://127.0.0.1:{port}/"
    for _ in range(100):
        try:
            fetch(url, None, False, timeout=5.0)
            return process, url
        except Exception:
            if process.poll() is not None:
                raise RuntimeError("phrust-server exited during startup")
            time.sleep(0.1)
    process.kill()
    raise RuntimeError("phrust-server did not become ready")


def main() -> int:
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    wordpress_url = os.environ.get("PHRUST_WORDPRESS_URL", "").strip()
    host_header = os.environ.get("PHRUST_WORDPRESS_HOST_HEADER", "").strip() or None

    process = None
    profile_dir: Path | None = None
    workload = "wordpress-root"
    if wordpress_url:
        url = wordpress_url.rstrip("/") + "/"
        raw_dir = os.environ.get("PHRUST_REQUEST_PROFILE_DIR", "").strip()
        profile_dir = Path(raw_dir) if raw_dir else None
    else:
        workload = "front-controller-fixture"
        profile_dir = OUT_DIR / "request-profiles"
        profile_dir.mkdir(parents=True, exist_ok=True)
        for stale in profile_dir.glob("*.json"):
            stale.unlink()
        try:
            process, url = spawn_local_server(profile_dir)
        except FileNotFoundError as error:
            print(f"[skip] profiler overhead gate: {error}")
            return 0

    try:
        for _ in range(WARMUP):
            fetch(url, host_header, False)
        files_before = profile_file_count(profile_dir)
        a = run_batch(url, host_header, False, BATCH)
        files_after_a = profile_file_count(profile_dir)
        b = run_batch(url, host_header, True, max(BATCH // 2, 3))
        files_after_b = profile_file_count(profile_dir)
        c = run_batch(url, host_header, False, BATCH)
        files_after_c = profile_file_count(profile_dir)
    finally:
        if process is not None:
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()

    a_median = statistics.median(a)
    b_median = statistics.median(b)
    c_median = statistics.median(c)
    failures: list[str] = []
    if c_median > a_median * THRESHOLD:
        failures.append(
            f"unprofiled-after-profiled median {c_median:.1f}ms exceeds "
            f"{THRESHOLD:.2f}x clean unprofiled median {a_median:.1f}ms"
        )
    if files_before is not None:
        if files_after_a != files_before:
            failures.append("profile files were written during unprofiled batch A")
        if files_after_c != files_after_b:
            failures.append("profile files were written during unprofiled batch C")
        if files_after_b == files_after_a:
            failures.append(
                "no profile files written during profiled batch B "
                "(gate cannot distinguish profiled and unprofiled phases)"
            )

    status = "fail" if failures else "pass"
    summary = {
        "status": status,
        "workload": workload,
        "threshold": THRESHOLD,
        "batch_medians_ms": {"a": a_median, "b": b_median, "c": c_median},
        "batches_ms": {"a": a, "b": b, "c": c},
        "profile_files": {
            "before": files_before,
            "after_a": files_after_a,
            "after_b": files_after_b,
            "after_c": files_after_c,
        },
        "failures": failures,
        "wordpress": bool(wordpress_url),
    }
    (OUT_DIR / "summary.json").write_text(json.dumps(summary, indent=1) + "\n")
    lines = [
        "# Profiler Overhead Gate",
        "",
        f"Status: `{status}`  (workload: {workload})",
        "",
        f"| batch | median ms |",
        f"| --- | ---: |",
        f"| A unprofiled | {a_median:.1f} |",
        f"| B profiled | {b_median:.1f} |",
        f"| C unprofiled | {c_median:.1f} |",
        "",
    ]
    lines.extend(f"- FAIL: {failure}" for failure in failures)
    if not wordpress_url:
        lines.append(
            "- note: WordPress root unavailable; containment validated on the "
            "committed fixture app. WordPress-level claims stay BLOCKED."
        )
    (OUT_DIR / "summary.md").write_text("\n".join(lines) + "\n")
    print(f"[{status}] profiler overhead gate: A={a_median:.1f}ms B={b_median:.1f}ms C={c_median:.1f}ms")
    for failure in failures:
        print(f"  FAIL: {failure}")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
