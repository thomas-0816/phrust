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
        process, address = start_server(server, docroot, out_dir, log_path, trace_path)
        for _ in range(max(args.warmups, 0)):
            request_root(address, args.timeout_seconds)
        sample = request_root(address, args.timeout_seconds)
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
        "--perf-trace",
        str(trace_path),
        "--perf-trace-vm-counters",
        "--request-profile",
        str(out_dir),
        "--request-profile-source-attribution",
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


def request_root(address: str, timeout_seconds: float) -> dict[str, Any]:
    host, port_text = address.rsplit(":", 1)
    started = time.perf_counter_ns()
    connection = http.client.HTTPConnection(
        host, int(port_text), timeout=timeout_seconds
    )
    connection.request("GET", "/", headers={"Host": "127.0.0.1"})
    response = connection.getresponse()
    body = response.read()
    connection.close()
    return {
        "status": response.status,
        "body_bytes": len(body),
        "wall_ms": (time.perf_counter_ns() - started) / 1_000_000.0,
    }


def summarize_profile(profile: dict[str, Any]) -> dict[str, Any]:
    attribution = as_dict(profile.get("attribution"))
    summary = as_dict(attribution.get("summary_counters"))
    phases = as_dict(profile.get("phases_nanos"))
    includes = as_dict(attribution.get("includes"))
    calls = as_dict(attribution.get("calls"))
    arrays = as_dict(attribution.get("arrays"))
    objects = as_dict(attribution.get("objects"))
    clones = as_dict(attribution.get("clones"))
    native = as_dict(attribution.get("native"))
    return {
        "schema_version": int(profile.get("schema_version", 0)),
        "phases_nanos": phases,
        "core_counters": {
            "vm_value_clones": summary.get("vm_value_clones", 0),
            "vm_array_handle_clones": summary.get("vm_array_handle_clones", 0),
            "vm_function_calls": summary.get("vm_function_calls", 0),
            "vm_method_calls": summary.get("vm_method_calls", 0),
            "vm_internal_function_dispatches": summary.get(
                "vm_internal_function_dispatches", 0
            ),
            "vm_include_rich_instructions_executed": summary.get(
                "vm_include_rich_instructions_executed", 0
            ),
            "vm_bytecode_instructions_executed": summary.get(
                "vm_bytecode_instructions_executed", 0
            ),
        },
        "top": {
            "value_clone_by_reason": top_entries(clones.get("value_clone_by_reason")),
            "value_clone_by_source_family": top_entries(
                clones.get("value_clone_by_source_family")
            ),
            "value_clone_by_kind": top_entries(clones.get("value_clone_by_kind")),
            "string_allocation_by_source_family": top_entries(
                clones.get("string_allocation_by_source_family")
            ),
            "array_handle_clone_by_source_family": top_entries(
                clones.get("array_handle_clone_by_source_family")
            ),
            "cow_separation_by_source_family": top_entries(
                clones.get("cow_separation_by_source_family")
            ),
            "reference_cell_creation_by_source_family": top_entries(
                clones.get("reference_cell_creation_by_source_family")
            ),
            "include_fallback_by_reason": top_entries(
                includes.get("include_fallback_by_reason")
            ),
            "dense_include_entry_fallback_by_reason": top_entries(
                includes.get("dense_include_entry_fallback_by_reason")
            ),
            "builtin_fast_stub_fallback_by_reason": top_entries(
                calls.get("builtin_fast_stub_fallback_by_reason")
            ),
            "array_fast_path_fallback_by_reason": top_entries(
                arrays.get("array_fast_path_fallback_by_reason")
            ),
            "property_ic_fallback_reasons": top_entries(
                objects.get("property_ic_fallback_reasons")
            ),
            "native_eligibility_rejections_by_reason": top_entries(
                native.get("native_eligibility_rejections_by_reason")
            ),
        },
        "exclusive_boundaries": top_exclusive_boundaries(calls, includes),
    }


def top_exclusive_boundaries(
    calls: dict[str, Any], includes: dict[str, Any], limit: int = 10
) -> list[dict[str, Any]]:
    entries: list[dict[str, Any]] = []
    for key in (
        "function_profiles_by_name",
        "method_profiles_by_name",
        "builtin_profiles_by_name",
    ):
        entries.extend(top_entries(calls.get(key), limit=1_000_000))
    entries.extend(
        top_entries(includes.get("include_profiles_by_path"), limit=1_000_000)
    )
    return sorted(
        entries,
        key=lambda entry: int(entry.get("exclusive_nanos", 0)),
        reverse=True,
    )[:limit]


def top_entries(value: Any, limit: int = 10) -> list[dict[str, Any]]:
    if not isinstance(value, list):
        return []
    entries = [entry for entry in value if isinstance(entry, dict)]
    return entries[:limit]


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
                "## Core Counters",
                "",
            ]
        )
        for key, value in report["summary"]["core_counters"].items():
            lines.append(f"- `{key}`: {value}")
        lines.extend(["", "## Top Attribution", ""])
        for family, entries in report["summary"]["top"].items():
            lines.append(f"### `{family}`")
            if entries:
                for entry in entries:
                    lines.append(
                        f"- `{entry.get('name', '')}`: {entry.get('count', 0)}"
                    )
            else:
                lines.append("- none")
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
        "schema_version": 2,
        "phases_nanos": {"php_vm_execution": 123},
        "attribution": {
            "summary_counters": {"vm_value_clones": 5, "vm_function_calls": 2},
            "clones": {
                "value_clone_by_reason": [{"name": "return_value", "count": 5}],
                "value_clone_by_source_family": [{"name": "return_value", "count": 5}],
                "value_clone_by_kind": [{"name": "array_handle", "count": 5}],
                "string_allocation_by_source_family": [
                    {"name": "return_value", "count": 2}
                ],
                "array_handle_clone_by_source_family": [
                    {"name": "array_element_read", "count": 3}
                ],
                "cow_separation_by_source_family": [
                    {"name": "array_element_write", "count": 2}
                ],
                "reference_cell_creation_by_source_family": [
                    {"name": "by_ref_argument_binding", "count": 1}
                ],
            },
            "includes": {
                "include_fallback_by_reason": [{"name": "unsupported", "count": 1}],
                "include_profiles_by_path": [
                    {"name": "plugin.php", "exclusive_nanos": 20}
                ],
            },
            "calls": {
                "function_profiles_by_name": [{"name": "render", "exclusive_nanos": 40}]
            },
            "arrays": {},
            "objects": {},
            "native": {},
        },
    }
    summary = summarize_profile(profile)
    assert summary["core_counters"]["vm_value_clones"] == 5
    assert summary["schema_version"] == 2
    assert summary["top"]["value_clone_by_reason"][0]["name"] == "return_value"
    assert summary["top"]["value_clone_by_source_family"][0]["name"] == "return_value"
    assert (
        summary["top"]["array_handle_clone_by_source_family"][0]["name"]
        == "array_element_read"
    )
    assert (
        summary["top"]["cow_separation_by_source_family"][0]["name"]
        == "array_element_write"
    )
    assert (
        summary["top"]["reference_cell_creation_by_source_family"][0]["name"]
        == "by_ref_argument_binding"
    )
    assert summary["exclusive_boundaries"][0]["name"] == "render"
    print("[pass] root_profile self-test")
    return 0


if __name__ == "__main__":
    sys.exit(main())
