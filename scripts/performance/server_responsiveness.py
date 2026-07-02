#!/usr/bin/env python3
"""Measure integrated server responsiveness for deterministic PHP routes."""

from __future__ import annotations

import argparse
import hashlib
import http.client
import json
import os
import re
import socket
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from ratchet_schema import (
    ROOT,
    distribution_metrics,
    executable,
    make_report,
    rel,
    render_report_markdown,
    validate_report,
    write_json,
)


DEFAULT_SERVER = ROOT / "target/debug/phrust-server"
DEFAULT_DOCROOT = ROOT / "tests/fixtures/performance/server_responsiveness/public"
DEFAULT_OUT = ROOT / "target/performance/ratchet/server/current.json"


@dataclass(frozen=True)
class Scenario:
    id: str
    path: str
    expected_status: int
    expected_body: bytes


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--server", type=Path, default=DEFAULT_SERVER)
    parser.add_argument("--docroot", type=Path, default=DEFAULT_DOCROOT)
    parser.add_argument("--out", type=Path, default=DEFAULT_OUT)
    parser.add_argument("--markdown-out", type=Path)
    parser.add_argument("--iterations", type=int, default=int(os.getenv("PHRUST_RATCHET_ITERATIONS", "20")))
    parser.add_argument("--warmups", type=int, default=int(os.getenv("PHRUST_RATCHET_WARMUPS", "3")))
    parser.add_argument(
        "--concurrency",
        default=os.getenv("PHRUST_SERVER_RESPONSIVENESS_CONCURRENCY", "1,4,16"),
    )
    parser.add_argument("--timeout", type=float, default=float(os.getenv("PHRUST_SERVER_RESPONSIVENESS_TIMEOUT", "30.0")))
    parser.add_argument("--smoke", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    return parser.parse_args()


def scenarios() -> list[Scenario]:
    template = ",".join(f"<span>{i}</span>" for i in range(40)) + "\n"
    return [
        Scenario("server.front_controller", "/", 200, b"front:1\n"),
        Scenario("server.empty", "/empty.php", 200, b""),
        Scenario("server.template", "/template.php", 200, template.encode()),
        Scenario("server.include_chain", "/include_chain.php", 200, b"include:42\n"),
        Scenario("server.function_calls", "/function_calls.php", 200, b"calls:14950\n"),
        Scenario("server.arrays", "/arrays.php", 200, b"arrays:3240\n"),
    ]


def parse_concurrency(value: str, smoke: bool) -> list[int]:
    if smoke:
        return [1]
    levels = []
    for part in value.split(","):
        part = part.strip()
        if part:
            levels.append(max(int(part), 1))
    return levels or [1]


def start_server(server: Path, docroot: Path, log_path: Path) -> tuple[subprocess.Popen[str], str]:
    log_path.parent.mkdir(parents=True, exist_ok=True)
    log = log_path.open("w+", encoding="utf-8")
    process = subprocess.Popen(
        [str(server), "--listen", "127.0.0.1:0", "--docroot", str(docroot)],
        cwd=ROOT,
        text=True,
        stdout=log,
        stderr=subprocess.STDOUT,
    )
    address = ""
    for _ in range(200):
        if process.poll() is not None:
            log.seek(0)
            raise RuntimeError(f"server exited early:\n{log.read()}")
        log.flush()
        log.seek(0)
        text = log.read()
        matches = re.findall(r"^listening http://(.+)$", text, flags=re.MULTILINE)
        if matches:
            address = matches[-1].strip()
            break
        time.sleep(0.05)
    if not address:
        process.terminate()
        raise RuntimeError("server did not print listening address")
    return process, address


def stop_server(process: subprocess.Popen[str]) -> None:
    if process.poll() is not None:
        return
    process.terminate()
    try:
        process.wait(timeout=5.0)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=5.0)


def request_once(address: str, scenario: Scenario, timeout: float) -> dict[str, Any]:
    host, port_text = address.rsplit(":", 1)
    started = time.perf_counter_ns()
    error = ""
    status = 0
    body = b""
    ttfb_ms = 0.0
    try:
        conn = http.client.HTTPConnection(host, int(port_text), timeout=timeout)
        conn.request("GET", scenario.path)
        response = conn.getresponse()
        ttfb_ms = (time.perf_counter_ns() - started) / 1_000_000.0
        status = response.status
        body = response.read()
        conn.close()
    except (OSError, http.client.HTTPException, socket.timeout) as exc:
        error = str(exc)
    total_ms = (time.perf_counter_ns() - started) / 1_000_000.0
    return {
        "status_code": status,
        "body_sha256": hashlib.sha256(body).hexdigest(),
        "body_bytes": len(body),
        "ttfb_ms": ttfb_ms,
        "request_total_ms": total_ms,
        "error": error,
        "correct": status == scenario.expected_status and body == scenario.expected_body and not error,
    }


def measure_scenario(address: str, scenario: Scenario, iterations: int, warmups: int, concurrency: int, timeout: float) -> list[dict[str, Any]]:
    for _ in range(warmups):
        request_once(address, scenario, timeout)
    samples: list[dict[str, Any]] = []
    with ThreadPoolExecutor(max_workers=concurrency) as executor:
        futures = [
            executor.submit(request_once, address, scenario, timeout)
            for _ in range(iterations)
        ]
        for future in as_completed(futures):
            samples.append(future.result())
    return samples


def run(args: argparse.Namespace) -> dict[str, Any]:
    server = args.server if args.server.is_absolute() else ROOT / args.server
    docroot = args.docroot if args.docroot.is_absolute() else ROOT / args.docroot
    if not executable(server):
        raise SystemExit(f"server is not executable: {rel(server)}")
    iterations = 1 if args.smoke else max(args.iterations, 1)
    warmups = 1 if args.smoke else max(args.warmups, 0)
    out = args.out if args.out.is_absolute() else ROOT / args.out
    log_path = out.parent / "logs" / "phrust-server.log"
    process: subprocess.Popen[str] | None = None
    report_scenarios: list[dict[str, Any]] = []
    failures: list[str] = []
    try:
        process, address = start_server(server, docroot, log_path)
        for scenario in scenarios():
            for concurrency in parse_concurrency(args.concurrency, args.smoke):
                samples = measure_scenario(address, scenario, iterations, warmups, concurrency, args.timeout)
                bad = [sample for sample in samples if not sample["correct"]]
                correctness = "fail" if bad else "pass"
                if bad:
                    failures.append(f"{scenario.id} concurrency={concurrency}: {bad[0].get('error') or 'response mismatch'}")
                metrics = {
                    **distribution_metrics("ttfb_ms", [float(sample["ttfb_ms"]) for sample in samples]),
                    **distribution_metrics("request_total_ms", [float(sample["request_total_ms"]) for sample in samples]),
                    "status_code": float(samples[-1]["status_code"]) if samples else 0.0,
                    "body_bytes": float(samples[-1]["body_bytes"]) if samples else 0.0,
                    "errors": float(len(bad)),
                    "concurrency": float(concurrency),
                }
                if "request_total_ms.p50" in metrics:
                    metrics["external_wall_ms.p50"] = metrics["request_total_ms.p50"]
                    metrics["external_wall_ms.p95"] = metrics["request_total_ms.p95"]
                    metrics["external_wall_ms.p99"] = metrics["request_total_ms.p99"]
                report_scenarios.append(
                    {
                        "id": f"{scenario.id}.c{concurrency}",
                        "group": "server",
                        "kind": "request",
                        "correctness": correctness,
                        "metrics": metrics,
                        "phase_metrics": {},
                        "counter_highlights": {},
                        "artifacts": {
                            "path": scenario.path,
                            "body_sha256": samples[-1]["body_sha256"] if samples else "",
                            "log": rel(log_path),
                        },
                    }
                )
    finally:
        if process is not None:
            stop_server(process)
    return make_report(
        run_id="server-responsiveness-ratchet-smoke" if args.smoke else "server-responsiveness-ratchet",
        created_by="server_responsiveness.py",
        scenarios=report_scenarios,
        failures=failures,
    )


def run_self_test() -> int:
    levels = parse_concurrency("1,4,16", False)
    assert levels == [1, 4, 16]
    assert scenarios()[0].expected_body == b"front:1\n"
    print("[pass] server_responsiveness self-test")
    return 0


def main() -> int:
    args = parse_args()
    if args.self_test:
        return run_self_test()
    report = run(args)
    errors = validate_report(report)
    if errors:
        raise SystemExit("; ".join(errors))
    out = args.out if args.out.is_absolute() else ROOT / args.out
    markdown = args.markdown_out or out.with_suffix(".md")
    markdown = markdown if markdown.is_absolute() else ROOT / markdown
    write_json(out, report)
    markdown.parent.mkdir(parents=True, exist_ok=True)
    markdown.write_text(render_report_markdown(report, "Server Responsiveness Ratchet"), encoding="utf-8")
    print(f"[{'fail' if report['failures'] else 'pass'}] server responsiveness ratchet wrote {rel(out)}")
    return 1 if report["failures"] else 0


if __name__ == "__main__":
    sys.exit(main())
