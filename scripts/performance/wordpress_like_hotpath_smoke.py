#!/usr/bin/env python3
"""Exercise the deterministic WordPress-like server hot path."""

from __future__ import annotations

import argparse
import http.client
import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[2]
DEFAULT_SERVER = ROOT / "target/debug/phrust-server"
DEFAULT_DOCROOT = ROOT / "fixtures/server/apps/wordpress-like/public"
DEFAULT_OUT = ROOT / "target/performance/wordpress-like/report.json"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--server", type=Path, default=DEFAULT_SERVER)
    parser.add_argument("--docroot", type=Path, default=DEFAULT_DOCROOT)
    parser.add_argument("--out", type=Path, default=DEFAULT_OUT)
    parser.add_argument("--warmups", type=int, default=int(os.getenv("PHRUST_WORDPRESS_LIKE_WARMUPS", "2")))
    parser.add_argument("--measurements", type=int, default=int(os.getenv("PHRUST_WORDPRESS_LIKE_MEASUREMENTS", "2")))
    parser.add_argument("--timeout", type=float, default=float(os.getenv("PHRUST_WORDPRESS_LIKE_TIMEOUT", "10.0")))
    parser.add_argument("--self-test", action="store_true")
    return parser.parse_args()


def rel(path: Path) -> str:
    try:
        return str(path.relative_to(ROOT))
    except ValueError:
        return str(path)


def start_server(server: Path, docroot: Path, log_path: Path, trace_path: Path) -> tuple[subprocess.Popen[str], str]:
    log_path.parent.mkdir(parents=True, exist_ok=True)
    trace_path.parent.mkdir(parents=True, exist_ok=True)
    log = log_path.open("w+", encoding="utf-8")
    process = subprocess.Popen(
        [
            str(server),
            "--listen",
            "127.0.0.1:0",
            "--docroot",
            str(docroot),
            "--front-controller",
            "index.php",
            "--perf-trace",
            str(trace_path),
        ],
        cwd=ROOT,
        text=True,
        stdout=log,
        stderr=subprocess.STDOUT,
    )
    for _ in range(200):
        if process.poll() is not None:
            log.seek(0)
            raise RuntimeError(f"server exited early:\n{log.read()}")
        log.flush()
        log.seek(0)
        text = log.read()
        matches = re.findall(r"^listening http://(.+)$", text, flags=re.MULTILINE)
        if matches:
            return process, matches[-1].strip()
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


def request(address: str, path: str, timeout: float) -> dict[str, Any]:
    host, port_text = address.rsplit(":", 1)
    started = time.perf_counter_ns()
    conn = http.client.HTTPConnection(host, int(port_text), timeout=timeout)
    conn.request("GET", path, headers={"Cookie": "wp_like=cookie-hit"})
    response = conn.getresponse()
    body = response.read()
    headers = dict(response.getheaders())
    conn.close()
    return {
        "path": path,
        "status": response.status,
        "body": body.decode("utf-8", errors="replace"),
        "headers": headers,
        "wall_ms": (time.perf_counter_ns() - started) / 1_000_000.0,
    }


def metrics(address: str, timeout: float) -> dict[str, float]:
    host, port_text = address.rsplit(":", 1)
    conn = http.client.HTTPConnection(host, int(port_text), timeout=timeout)
    conn.request("GET", "/__phrust/metrics")
    response = conn.getresponse()
    text = response.read().decode("utf-8", errors="replace")
    conn.close()
    values: dict[str, float] = {}
    for line in text.splitlines():
        if not line or line.startswith("#"):
            continue
        name, _, value = line.partition(" ")
        values[name] = float(value)
    return values


def load_traces(path: Path) -> list[dict[str, Any]]:
    if not path.exists():
        return []
    traces = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.strip():
            traces.append(json.loads(line))
    return traces


def require_metric(report: dict[str, Any], key: str, minimum: float) -> None:
    value = report["metrics"].get(key, 0.0)
    if value < minimum:
        report["failures"].append(f"{key} expected >= {minimum}, got {value}")


def run(args: argparse.Namespace) -> dict[str, Any]:
    server = args.server if args.server.is_absolute() else ROOT / args.server
    docroot = args.docroot if args.docroot.is_absolute() else ROOT / args.docroot
    out = args.out if args.out.is_absolute() else ROOT / args.out
    log_path = out.parent / "phrust-server.log"
    trace_path = out.parent / "perf-trace.jsonl"
    report: dict[str, Any] = {
        "status": "pass",
        "inputs": {"server": rel(server), "docroot": rel(docroot)},
        "requests": [],
        "metrics": {},
        "traces": [],
        "artifacts": {"log": rel(log_path), "trace": rel(trace_path), "report": rel(out)},
        "failures": [],
    }
    process: subprocess.Popen[str] | None = None
    try:
        process, address = start_server(server, docroot, log_path, trace_path)
        for _ in range(max(args.warmups, 0)):
            request(address, "/posts/42?preview=1", args.timeout)
        for _ in range(max(args.measurements, 1)):
            sample = request(address, "/posts/42?preview=1", args.timeout)
            report["requests"].append({k: v for k, v in sample.items() if k != "body"})
            body = sample["body"]
            if sample["status"] != 200:
                report["failures"].append(f"warm request status {sample['status']}")
            if "wordpress-like|alpha=1|route=single" not in body:
                report["failures"].append("warm response body did not contain expected route output")
            if "beta=1" not in body:
                report["failures"].append("warm response body did not pass through filter chain")
        report["metrics"] = metrics(address, args.timeout)
    finally:
        if process is not None:
            stop_server(process)
    report["traces"] = load_traces(trace_path)

    require_metric(report, "phrust_server_script_cache_hits_total", 1.0)
    require_metric(report, "phrust_server_entry_script_source_reads_total", 1.0)
    require_metric(report, "phrust_server_include_compile_hits_total", 1.0)
    require_metric(report, "phrust_server_include_source_reads_total", 1.0)
    require_metric(report, 'phrust_server_request_phase_count{phase="body_read"}', 1.0)
    require_metric(report, 'phrust_server_request_phase_count{phase="vm_execution"}', 1.0)
    if report["metrics"].get("phrust_server_runtime_diagnostics_total", 0.0) != 0.0:
        report["failures"].append("runtime diagnostics were emitted")
    if not report["traces"]:
        report["failures"].append("perf trace JSONL was not written")
    else:
        last = report["traces"][-1]
        for field in ["phases_nanos", "counters", "runtime_diagnostics"]:
            if field not in last:
                report["failures"].append(f"perf trace missing {field}")

    if report["failures"]:
        report["status"] = "fail"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    markdown = out.with_suffix(".md")
    markdown.write_text(render_markdown(report), encoding="utf-8")
    print(f"[{report['status']}] wordpress-like hotpath wrote {rel(out)}")
    return report


def render_markdown(report: dict[str, Any]) -> str:
    lines = [
        "# WordPress-like Hotpath Smoke",
        "",
        f"Status: {report['status']}",
        "",
        "## Metrics",
    ]
    for key in sorted(report["metrics"]):
        if key.startswith("phrust_server_script_cache") or key.startswith("phrust_server_include") or "request_phase_count" in key:
            lines.append(f"- `{key}`: {report['metrics'][key]:.0f}")
    if report["failures"]:
        lines.extend(["", "## Failures"])
        lines.extend(f"- {failure}" for failure in report["failures"])
    lines.append("")
    return "\n".join(lines)


def run_self_test() -> int:
    report = {"metrics": {"x": 1.0}, "failures": []}
    require_metric(report, "x", 1.0)
    require_metric(report, "missing", 1.0)
    assert report["failures"]
    print("[pass] wordpress_like_hotpath_smoke self-test")
    return 0


def main() -> int:
    args = parse_args()
    if args.self_test:
        return run_self_test()
    report = run(args)
    return 1 if report["failures"] else 0


if __name__ == "__main__":
    sys.exit(main())
