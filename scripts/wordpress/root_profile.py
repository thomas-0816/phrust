#!/usr/bin/env python3
"""Collect a request-profile JSON file for a local real WordPress root page."""

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

SCRIPT_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPT_DIR))

from common import REPO_ROOT, now_run_id, repo_path, wordpress_shape_blockers  # noqa: E402


def main() -> int:
    args = parse_args()
    if args.self_test:
        return self_test()
    out_dir = output_dir(args)
    out_dir.mkdir(parents=True, exist_ok=True)
    report = run(args, out_dir)
    write_json(report, out_dir / "summary.json")
    write_markdown(report, out_dir / "summary.md")
    print(
        f"[{report['status']}] wordpress root profile wrote {rel(out_dir / 'summary.md')}"
    )
    return 0 if report["status"] in {"pass", "skip"} else 1


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--wordpress-dir", default=os.environ.get("PHRUST_WORDPRESS_DIR", "")
    )
    parser.add_argument(
        "--docroot", default=os.environ.get("PHRUST_WORDPRESS_DOCROOT", "")
    )
    parser.add_argument(
        "--server",
        default=os.environ.get("PHRUST_SERVER", "target/debug/phrust-server"),
    )
    parser.add_argument("--out", default="")
    parser.add_argument(
        "--warmups",
        type=int,
        default=int(os.environ.get("PHRUST_WORDPRESS_PROFILE_WARMUPS", "1")),
    )
    parser.add_argument(
        "--timeout-seconds",
        type=float,
        default=float(os.environ.get("PHRUST_WORDPRESS_PROFILE_TIMEOUT_SECONDS", "30")),
    )
    parser.add_argument("--self-test", action="store_true")
    return parser.parse_args()


def output_dir(args: argparse.Namespace) -> Path:
    if args.out:
        return repo_path(args.out) or Path(args.out).expanduser()
    return (
        REPO_ROOT
        / "target"
        / "performance"
        / "wordpress-root-profile"
        / now_run_id("root")
    )


def run(args: argparse.Namespace, out_dir: Path) -> dict[str, Any]:
    wordpress_dir = repo_path(args.wordpress_dir)
    docroot = repo_path(args.docroot) or wordpress_dir
    server = repo_path(args.server)
    blockers = wordpress_shape_blockers(docroot)
    if blockers:
        return skip_report("environment", blockers, args, out_dir)
    if server is None or not server.is_file():
        return skip_report("environment", ["missing_phrust_server"], args, out_dir)

    log_path = out_dir / "server.log"
    trace_path = out_dir / "perf-trace.jsonl"
    process: subprocess.Popen[str] | None = None
    try:
        process, address = start_server(
            server,
            docroot,
            out_dir,
            log_path,
            trace_path,
            max_execution_ms=max(1, int(args.timeout_seconds * 1000)),
        )
        for _ in range(max(args.warmups, 0)):
            request_root(address, args.timeout_seconds, profile=False)
        sample = request_root(address, args.timeout_seconds, profile=True)
    except Exception as error:
        return fail_report(str(error), args, out_dir, log_path, trace_path)
    finally:
        if process is not None:
            stop_server(process)

    profiles = sorted(
        (path for path in out_dir.glob("*.json") if path.name != "summary.json"),
        key=lambda path: path.stat().st_mtime,
    )
    if not profiles:
        return fail_report(
            "server completed the request but wrote no request-profile JSON",
            args,
            out_dir,
            log_path,
            trace_path,
        )
    profile_path = profiles[-1]
    profile = json.loads(profile_path.read_text(encoding="utf-8"))
    return {
        "status": "pass" if int(sample["status"]) < 500 else "fail",
        "inputs": inputs(args),
        "http": sample,
        "artifacts": {
            "profile": rel(profile_path),
            "trace": rel(trace_path),
            "server_log": rel(log_path),
            "summary_json": rel(out_dir / "summary.json"),
            "summary_markdown": rel(out_dir / "summary.md"),
        },
        "summary": summarize_profile(profile),
    }


def start_server(
    server: Path,
    docroot: Path,
    out_dir: Path,
    log_path: Path,
    trace_path: Path,
    *,
    max_execution_ms: int,
) -> tuple[subprocess.Popen[str], str]:
    log = log_path.open("w+", encoding="utf-8")
    command = [
        str(server),
        "--listen",
        "127.0.0.1:0",
        "--docroot",
        str(docroot),
        "--front-controller",
        "index.php",
        "--max-execution-ms",
        str(max_execution_ms),
        "--native-cache",
        "read-write",
        "--native-cache-dir",
        str(out_dir / "native-cache"),
        "--perf-trace",
        str(trace_path),
        "--perf-trace-vm-counters",
        "--request-profile",
        str(out_dir),
        "--request-profile-vm-counters",
    ]
    process = subprocess.Popen(
        command, cwd=REPO_ROOT, text=True, stdout=log, stderr=subprocess.STDOUT
    )
    for _ in range(240):
        if process.poll() is not None:
            log.seek(0)
            raise RuntimeError(f"server exited early:\n{log.read()}")
        log.flush()
        log.seek(0)
        match = re.findall(r"^listening http://(.+)$", log.read(), flags=re.MULTILINE)
        if match:
            return process, match[-1].strip()
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


def request_root(
    address: str,
    timeout_seconds: float,
    *,
    profile: bool,
) -> dict[str, Any]:
    host, port_text = address.rsplit(":", 1)
    started = time.perf_counter_ns()
    connection = http.client.HTTPConnection(
        host, int(port_text), timeout=timeout_seconds
    )
    headers = {"Host": "127.0.0.1"}
    if profile:
        headers["x-phrust-request-profile"] = "1"
    connection.request("GET", "/", headers=headers)
    response = connection.getresponse()
    body = response.read()
    connection.close()
    return {
        "status": response.status,
        "body_bytes": len(body),
        "wall_ms": (time.perf_counter_ns() - started) / 1_000_000.0,
    }


def summarize_profile(profile: dict[str, Any]) -> dict[str, Any]:
    phases = as_dict(profile.get("phases_nanos"))
    native = as_dict(profile.get("native"))
    return {
        "schema_version": int(profile.get("schema_version", 0)),
        "phases_nanos": phases,
        "native_counters": {
            "compile_attempts": native.get("compile_attempts", 0),
            "compile_successes": native.get("compile_successes", 0),
            "compile_failures": native.get("compile_failures", 0),
            "compile_time_nanos": native.get("compile_time_nanos", 0),
            "cache_hits": native.get("cache_hits", 0),
            "cache_misses": native.get("cache_misses", 0),
            "cache_compile_waits": native.get("cache_compile_waits", 0),
            "cache_evictions": native.get("cache_evictions", 0),
            "execution_entries": native.get("execution_entries", 0),
            "region_side_exits": native.get("region_side_exits", 0),
            "runtime_helper_calls": native.get("runtime_helper_calls", 0),
            "runtime_helper_calls_by_id": as_dict(
                native.get("runtime_helper_calls_by_id")
            ),
            "runtime_helper_time_nanos": native.get(
                "runtime_helper_time_nanos", 0
            ),
            "runtime_helper_time_nanos_by_id": as_dict(
                native.get("runtime_helper_time_nanos_by_id")
            ),
            "execution_time_nanos": native.get("execution_time_nanos", 0),
            "call_direct": native.get("call_direct", 0),
            "call_dynamic": native.get("call_dynamic", 0),
            "transition_count": native.get("transition_count", 0),
            "transition_by_reason": as_dict(native.get("transition_by_reason")),
            "transition_time_nanos": native.get("transition_time_nanos", 0),
            "transition_time_nanos_by_reason": as_dict(
                native.get("transition_time_nanos_by_reason")
            ),
            "runtime_helper_object_release_fast_paths": native.get(
                "runtime_helper_object_release_fast_paths", 0
            ),
            "runtime_helper_object_release_root_scans": native.get(
                "runtime_helper_object_release_root_scans", 0
            ),
            "versions_published": native.get("versions_published", 0),
        },
    }


def as_dict(value: Any) -> dict[str, Any]:
    return value if isinstance(value, dict) else {}


def write_markdown(report: dict[str, Any], path: Path) -> None:
    lines = [
        "# WordPress Root Request Profile",
        "",
        f"Status: `{report['status']}`",
        "",
    ]
    if report["status"] == "skip":
        lines.extend(["## Skip Reason", "", ", ".join(report["blockers"]), ""])
    elif report["status"] == "fail" and "error" in report:
        lines.extend(["## Error", "", f"```text\n{report['error']}\n```", ""])
    else:
        http = report["http"]
        lines.extend(
            [
                "## Request",
                "",
                f"- HTTP status: {http['status']}",
                f"- Body bytes: {http['body_bytes']}",
                f"- Wall time: {http['wall_ms']:.3f} ms",
                "",
                "## Native Counters",
                "",
            ]
        )
        for key, value in report["summary"]["native_counters"].items():
            lines.append(f"- `{key}`: {value}")
        lines.append("")
    lines.extend(["## Artifacts", ""])
    for key, value in report.get("artifacts", {}).items():
        lines.append(f"- `{key}`: `{value}`")
    path.write_text("\n".join(lines).rstrip() + "\n", encoding="utf-8")


def write_json(value: dict[str, Any], path: Path) -> None:
    path.write_text(
        json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )


def inputs(args: argparse.Namespace) -> dict[str, str]:
    return {
        "wordpress_dir": args.wordpress_dir,
        "docroot": args.docroot or args.wordpress_dir,
        "server": args.server,
    }


def skip_report(
    reason: str, blockers: list[str], args: argparse.Namespace, out_dir: Path
) -> dict[str, Any]:
    return {
        "status": "skip",
        "reason": reason,
        "blockers": blockers,
        "inputs": inputs(args),
        "artifacts": {
            "summary_json": rel(out_dir / "summary.json"),
            "summary_markdown": rel(out_dir / "summary.md"),
        },
    }


def fail_report(
    error: str,
    args: argparse.Namespace,
    out_dir: Path,
    log_path: Path,
    trace_path: Path,
) -> dict[str, Any]:
    return {
        "status": "fail",
        "error": error,
        "inputs": inputs(args),
        "artifacts": {
            "trace": rel(trace_path),
            "server_log": rel(log_path),
            "summary_json": rel(out_dir / "summary.json"),
            "summary_markdown": rel(out_dir / "summary.md"),
        },
    }


def rel(path: Path) -> str:
    try:
        return str(path.relative_to(REPO_ROOT))
    except ValueError:
        return str(path)


def self_test() -> int:
    profile = {
        "schema_version": 5,
        "phases_nanos": {"php_vm_execution": 123},
        "native": {
            "compile_attempts": 1,
            "compile_successes": 1,
            "compile_failures": 0,
            "compile_time_nanos": 99,
            "cache_hits": 2,
            "cache_misses": 1,
            "cache_compile_waits": 0,
            "cache_evictions": 0,
            "execution_entries": 1,
            "region_side_exits": 0,
            "runtime_helper_calls": 7,
            "runtime_helper_calls_by_id": {"binary": 4, "echo": 3},
            "runtime_helper_time_nanos": 70,
            "runtime_helper_time_nanos_by_id": {"binary": 50, "echo": 20},
            "execution_time_nanos": 123,
            "call_direct": 2,
            "call_dynamic": 1,
            "transition_count": 2,
            "transition_by_reason": {"same_unit": 2},
            "transition_time_nanos": 30,
            "transition_time_nanos_by_reason": {"same_unit": 30},
            "runtime_helper_object_release_fast_paths": 3,
            "runtime_helper_object_release_root_scans": 1,
            "versions_published": 1,
        },
    }
    summary = summarize_profile(profile)
    assert summary["schema_version"] == 5
    assert summary["native_counters"]["cache_hits"] == 2
    assert summary["native_counters"]["execution_entries"] == 1
    assert summary["native_counters"]["runtime_helper_calls"] == 7
    assert summary["native_counters"]["runtime_helper_calls_by_id"]["binary"] == 4
    assert summary["native_counters"]["runtime_helper_time_nanos_by_id"]["echo"] == 20
    print("[pass] root_profile self-test")
    return 0


if __name__ == "__main__":
    sys.exit(main())
