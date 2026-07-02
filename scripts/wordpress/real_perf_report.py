#!/usr/bin/env python3
"""Optional local real WordPress performance report."""

from __future__ import annotations

import http.client
import json
import os
import re
import statistics
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parents[2]


def rel(path: Path) -> str:
    try:
        return str(path.relative_to(REPO_ROOT))
    except ValueError:
        return str(path)


def start_server(server: Path, docroot: Path, out_dir: Path) -> tuple[subprocess.Popen[str], str, Path, Path]:
    log_path = out_dir / "phrust-server.log"
    trace_path = out_dir / "perf-trace.jsonl"
    log = log_path.open("w+", encoding="utf-8")
    command = [
        str(server),
        "--listen",
        "127.0.0.1:0",
        "--docroot",
        str(docroot),
        "--front-controller",
        "index.php",
        "--perf-trace",
        str(trace_path),
    ]
    process = subprocess.Popen(command, cwd=REPO_ROOT, text=True, stdout=log, stderr=subprocess.STDOUT)
    for _ in range(240):
        if process.poll() is not None:
            log.seek(0)
            raise RuntimeError(f"server exited early:\n{log.read()}")
        log.flush()
        log.seek(0)
        matches = re.findall(r"^listening http://(.+)$", log.read(), flags=re.MULTILINE)
        if matches:
            return process, matches[-1].strip(), log_path, trace_path
        time.sleep(0.05)
    process.terminate()
    raise RuntimeError("server did not print listening address")


def stop_server(process: subprocess.Popen[str]) -> None:
    if process.poll() is not None:
        return
    process.terminate()
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=5)


def request(address: str, timeout: float) -> dict[str, Any]:
    host, port_text = address.rsplit(":", 1)
    started = time.perf_counter_ns()
    conn = http.client.HTTPConnection(host, int(port_text), timeout=timeout)
    conn.request("GET", "/")
    response = conn.getresponse()
    body = response.read()
    conn.close()
    return {
        "status": response.status,
        "body_bytes": len(body),
        "wall_ms": (time.perf_counter_ns() - started) / 1_000_000.0,
        "body_prefix": body[:240].decode("utf-8", errors="replace"),
    }


def metrics(address: str, timeout: float) -> dict[str, float]:
    host, port_text = address.rsplit(":", 1)
    conn = http.client.HTTPConnection(host, int(port_text), timeout=timeout)
    conn.request("GET", "/__phrust/metrics")
    response = conn.getresponse()
    text = response.read().decode("utf-8", errors="replace")
    conn.close()
    parsed: dict[str, float] = {}
    for line in text.splitlines():
        if not line or line.startswith("#"):
            continue
        name, _, value = line.partition(" ")
        parsed[name] = float(value)
    return parsed


def percentile(values: list[float], pct: float) -> float:
    if not values:
        return 0.0
    ordered = sorted(values)
    index = min(len(ordered) - 1, max(0, round((len(ordered) - 1) * pct)))
    return ordered[index]


def main() -> int:
    wordpress_dir = os.environ.get("PHRUST_WORDPRESS_DIR", "")
    if not wordpress_dir:
        print("[skip] PHRUST_WORDPRESS_DIR is not set")
        return 0
    docroot = Path(os.environ.get("PHRUST_WORDPRESS_DOCROOT", wordpress_dir)).expanduser()
    server = Path(os.environ.get("PHRUST_SERVER", "target/debug/phrust-server"))
    if not server.is_absolute():
        server = REPO_ROOT / server
    if not docroot.is_dir():
        print(f"[skip] WordPress docroot is not a directory: {docroot}")
        return 0
    if not server.exists():
        print(f"[skip] phrust-server is missing: {rel(server)}")
        return 0

    run_id = time.strftime("wp-real-%Y%m%d-%H%M%S")
    out_dir = REPO_ROOT / "target/wordpress-real" / run_id
    out_dir.mkdir(parents=True, exist_ok=True)
    warmups = int(os.environ.get("PHRUST_WORDPRESS_PERF_WARMUPS", "2"))
    measurements = int(os.environ.get("PHRUST_WORDPRESS_PERF_MEASUREMENTS", "5"))
    timeout = float(os.environ.get("PHRUST_WORDPRESS_PERF_TIMEOUT", "20.0"))
    process: subprocess.Popen[str] | None = None
    samples: list[dict[str, Any]] = []
    failures: list[dict[str, Any]] = []
    before: dict[str, float] = {}
    after: dict[str, float] = {}
    log_path = out_dir / "phrust-server.log"
    trace_path = out_dir / "perf-trace.jsonl"
    try:
        process, address, log_path, trace_path = start_server(server, docroot, out_dir)
        before = metrics(address, timeout)
        for _ in range(max(warmups, 0)):
            request(address, timeout)
        for _ in range(max(measurements, 1)):
            sample = request(address, timeout)
            samples.append(sample)
            if sample["status"] >= 500 and not failures:
                failures.append(sample)
        after = metrics(address, timeout)
    finally:
        if process is not None:
            stop_server(process)

    wall = [float(sample["wall_ms"]) for sample in samples]
    report = {
        "status": "fail" if failures else "pass",
        "run_id": run_id,
        "inputs": {
            "wordpress_dir": str(Path(wordpress_dir).expanduser()),
            "docroot": str(docroot),
            "server": rel(server),
            "mysql_dsn_present": bool(os.environ.get("PHRUST_MYSQL_TEST_DSN", "")),
        },
        "latency_ms": {
            "min": min(wall) if wall else 0.0,
            "p50": statistics.median(wall) if wall else 0.0,
            "p95": percentile(wall, 0.95),
            "max": max(wall) if wall else 0.0,
        },
        "metrics_before": before,
        "metrics_after": after,
        "first_failure": failures[0] if failures else None,
        "artifacts": {"log": rel(log_path), "trace": rel(trace_path)},
    }
    out = out_dir / "report.json"
    out.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"[{report['status']}] wordpress real perf report wrote {rel(out)}")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
