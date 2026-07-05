#!/usr/bin/env python3
"""Optional real WordPress root benchmark/profile gate."""

from __future__ import annotations

import argparse
import hashlib
import http.client
import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path
from typing import Any
from urllib.parse import urlparse

WORDPRESS_SCRIPT_DIR = Path(__file__).resolve().parents[1] / "wordpress"
sys.path.insert(0, str(WORDPRESS_SCRIPT_DIR))

from common import REPO_ROOT, now_run_id, repo_path, wordpress_shape_blockers  # noqa: E402

DEFAULT_OUT_DIR = REPO_ROOT / "target/performance/wordpress-root"
DEFAULT_CONTROLS = (
    ("root", "/"),
    ("static_asset", "/wp-includes/css/buttons.css"),
    ("simple_php", "/wp-login.php"),
    ("db_smoke", "/phrust-db-smoke.php"),
    ("metrics", "/__phrust/metrics"),
)
COUNTER_KEYS = (
    "vm_value_clones",
    "vm_array_handle_clones",
    "vm_function_calls",
    "vm_method_calls",
    "vm_internal_function_dispatches",
    "vm_include_rich_instructions_executed",
    "vm_bytecode_instructions_executed",
    "vm_jit_compiled",
    "vm_jit_executed",
)


def main() -> int:
    args = parse_args()
    if args.self_test:
        return self_test()
    out_dir = output_dir(args)
    out_dir.mkdir(parents=True, exist_ok=True)
    report = run(args, out_dir)
    write_json(report, out_dir / "summary.json")
    write_markdown(report, out_dir / "summary.md")
    print(f"[{report['status']}] wordpress root benchmark wrote {rel(out_dir / 'summary.md')}")
    if report["status"] == "skip":
        return 0
    if report["status"] == "fail":
        return 1 if args.strict else 0
    return 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", default=os.environ.get("PHRUST_WORDPRESS_URL", ""))
    parser.add_argument(
        "--host-header",
        default=os.environ.get("PHRUST_WORDPRESS_HOST_HEADER", ""),
        help="override the HTTP Host header for port-mapped URL targets",
    )
    parser.add_argument("--wordpress-dir", default=os.environ.get("PHRUST_WORDPRESS_DIR", ""))
    parser.add_argument("--docroot", default=os.environ.get("PHRUST_WORDPRESS_DOCROOT", ""))
    parser.add_argument(
        "--server", default=os.environ.get("PHRUST_SERVER", "target/debug/phrust-server")
    )
    parser.add_argument("--out-dir", default="")
    parser.add_argument("--baseline", default=os.environ.get("PHRUST_WORDPRESS_ROOT_BASELINE", ""))
    parser.add_argument(
        "--samples",
        type=int,
        default=int(os.environ.get("PHRUST_WORDPRESS_ROOT_SAMPLES", "3")),
    )
    parser.add_argument(
        "--warmups",
        type=int,
        default=int(os.environ.get("PHRUST_WORDPRESS_ROOT_WARMUPS", "2")),
    )
    parser.add_argument(
        "--timeout-seconds",
        type=float,
        default=float(os.environ.get("PHRUST_WORDPRESS_ROOT_TIMEOUT_SECONDS", "30")),
    )
    parser.add_argument("--metrics-token", default=os.environ.get("PHRUST_METRICS_TOKEN", ""))
    parser.add_argument("--strict", action="store_true")
    parser.add_argument("--max-latency-regression-pct", type=float, default=20.0)
    parser.add_argument("--self-test", action="store_true")
    return parser.parse_args()


def output_dir(args: argparse.Namespace) -> Path:
    if args.out_dir:
        return repo_path(args.out_dir) or Path(args.out_dir).expanduser()
    return DEFAULT_OUT_DIR / now_run_id("root")


def run(args: argparse.Namespace, out_dir: Path) -> dict[str, Any]:
    baseline = load_baseline(args.baseline)
    process: subprocess.Popen[str] | None = None
    try:
        target = resolve_target(args, out_dir)
        if target["status"] == "skip":
            target["baseline"] = baseline_summary(baseline)
            return target
        process = target.pop("process", None)
        report = collect_benchmark(args, out_dir, target, baseline)
        return report
    except Exception as error:
        return {
            "status": "fail",
            "error": str(error),
            "baseline": baseline_summary(baseline),
            "artifacts": artifact_paths(out_dir),
        }
    finally:
        if process is not None:
            stop_server(process)


def resolve_target(args: argparse.Namespace, out_dir: Path) -> dict[str, Any]:
    if args.url:
        return {"status": "ready", "mode": "url", "base_url": args.url.rstrip("/")}

    wordpress_dir = repo_path(args.wordpress_dir)
    docroot = repo_path(args.docroot) or wordpress_dir
    blockers = wordpress_shape_blockers(docroot)
    if blockers:
        return skip_report("environment", blockers, args, out_dir)

    server = repo_path(args.server)
    if server is None or not server.is_file():
        return skip_report("environment", ["missing_phrust_server"], args, out_dir)

    trace_path = out_dir / "perf-trace.jsonl"
    profile_dir = out_dir / "request-profiles"
    log_path = out_dir / "server.log"
    process, base_url = start_server(server, docroot, profile_dir, trace_path, log_path)
    return {
        "status": "ready",
        "mode": "local_server",
        "base_url": base_url,
        "process": process,
        "docroot": str(docroot),
        "server": str(server),
        "trace_path": trace_path,
        "profile_dir": profile_dir,
        "log_path": log_path,
    }


def start_server(
    server: Path,
    docroot: Path,
    profile_dir: Path,
    trace_path: Path,
    log_path: Path,
) -> tuple[subprocess.Popen[str], str]:
    profile_dir.mkdir(parents=True, exist_ok=True)
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
        str(profile_dir),
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
        matches = re.findall(r"^listening http://(.+)$", log.read(), flags=re.MULTILINE)
        if matches:
            return process, f"http://{matches[-1].strip()}"
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


def collect_benchmark(
    args: argparse.Namespace,
    out_dir: Path,
    target: dict[str, Any],
    baseline: dict[str, Any] | None,
) -> dict[str, Any]:
    base_url = target["base_url"]
    host_header = args.host_header.strip()
    failures: list[str] = []
    for _ in range(max(args.warmups, 0)):
        http_get(base_url, "/", args.timeout_seconds, args.metrics_token, host_header=host_header)

    before_metrics = fetch_metrics(
        base_url, args.timeout_seconds, args.metrics_token, host_header=host_header
    )
    root_samples = [
        http_get(base_url, "/", args.timeout_seconds, args.metrics_token, host_header=host_header)
        for _ in range(max(args.samples, 1))
    ]
    after_metrics = fetch_metrics(
        base_url, args.timeout_seconds, args.metrics_token, host_header=host_header
    )
    controls = {
        name: http_get(
            base_url,
            path,
            args.timeout_seconds,
            args.metrics_token,
            allow_error=True,
            host_header=host_header,
        )
        for name, path in DEFAULT_CONTROLS
    }

    root_failures = validate_root_samples(root_samples, baseline)
    failures.extend(root_failures)
    metrics_delta = subtract_metrics(after_metrics, before_metrics)
    latest_profile = load_latest_profile(target.get("profile_dir"))
    latest_trace = load_latest_trace(target.get("trace_path"))
    summary = summarize(root_samples, controls, metrics_delta, latest_profile, latest_trace)
    failures.extend(compare_baseline(summary, baseline, args.max_latency_regression_pct))
    status = "fail" if failures else "pass"
    return {
        "status": status,
        "mode": target["mode"],
        "inputs": input_summary(args, target),
        "correctness": {"failures": failures},
        "samples": root_samples,
        "controls": controls,
        "metrics_delta": metrics_delta,
        "summary": summary,
        "baseline": baseline_summary(baseline),
        "artifacts": artifact_paths(out_dir, target),
    }


def http_get(
    base_url: str,
    path: str,
    timeout_seconds: float,
    metrics_token: str,
    allow_error: bool = False,
    host_header: str = "",
) -> dict[str, Any]:
    parsed = urlparse(base_url)
    host = parsed.hostname or "127.0.0.1"
    port = parsed.port or (443 if parsed.scheme == "https" else 80)
    full_path = path if path.startswith("/") else f"/{path}"
    headers = {"Host": host_header or request_host_header(parsed)}
    if full_path == "/__phrust/metrics" and metrics_token:
        headers["Authorization"] = f"Bearer {metrics_token}"
    started = time.perf_counter_ns()
    connection = http.client.HTTPConnection(host, port, timeout=timeout_seconds)
    try:
        connection.request("GET", full_path, headers=headers)
        response = connection.getresponse()
        response_started = time.perf_counter_ns()
        body = response.read()
    finally:
        connection.close()
    sample = {
        "path": full_path,
        "status": response.status,
        "body_bytes": len(body),
        "body_sha256": hashlib.sha256(body).hexdigest(),
        "ttfb_ms": (response_started - started) / 1_000_000.0,
        "wall_ms": (time.perf_counter_ns() - started) / 1_000_000.0,
    }
    if not allow_error and response.status >= 500:
        sample["error"] = f"server returned {response.status}"
    return sample


def fetch_metrics(
    base_url: str,
    timeout_seconds: float,
    metrics_token: str,
    host_header: str = "",
) -> dict[str, float]:
    parsed = urlparse(base_url)
    host = parsed.hostname or "127.0.0.1"
    port = parsed.port or (443 if parsed.scheme == "https" else 80)
    headers = {"Host": host_header or request_host_header(parsed)}
    if metrics_token:
        headers["Authorization"] = f"Bearer {metrics_token}"
    connection = http.client.HTTPConnection(host, port, timeout=timeout_seconds)
    try:
        connection.request("GET", "/__phrust/metrics", headers=headers)
        response = connection.getresponse()
        text = response.read().decode("utf-8", errors="replace")
    finally:
        connection.close()
    values: dict[str, float] = {}
    if response.status != 200:
        return values
    for line in text.splitlines():
        if not line or line.startswith("#"):
            continue
        name, _, value = line.partition(" ")
        try:
            values[name] = float(value)
        except ValueError:
            continue
    return values


def request_host_header(parsed: Any) -> str:
    return parsed.netloc or parsed.hostname or "127.0.0.1"


def validate_root_samples(samples: list[dict[str, Any]], baseline: dict[str, Any] | None) -> list[str]:
    failures: list[str] = []
    if not samples:
        return ["no root samples collected"]
    first_hash = samples[0]["body_sha256"]
    for index, sample in enumerate(samples):
        if sample["status"] != 200:
            failures.append(f"root sample {index} returned HTTP {sample['status']}")
        if sample["body_bytes"] <= 0:
            failures.append(f"root sample {index} had an empty body")
        if sample["body_sha256"] != first_hash:
            failures.append(f"root sample {index} response hash changed within run")
    baseline_hash = (((baseline or {}).get("summary") or {}).get("root") or {}).get("body_sha256")
    if baseline_hash and first_hash != baseline_hash:
        failures.append("root response hash differs from configured baseline")
    return failures


def subtract_metrics(after: dict[str, float], before: dict[str, float]) -> dict[str, float]:
    keys = set(before) | set(after)
    return {key: after.get(key, 0.0) - before.get(key, 0.0) for key in sorted(keys)}


def load_latest_profile(profile_dir_value: Any) -> dict[str, Any]:
    if not profile_dir_value:
        return {}
    profile_dir = Path(str(profile_dir_value))
    profiles = sorted(
        (path for path in profile_dir.glob("*.json") if path.name != "summary.json"),
        key=lambda path: path.stat().st_mtime,
    )
    if not profiles:
        return {}
    return json.loads(profiles[-1].read_text(encoding="utf-8"))


def load_latest_trace(trace_path_value: Any) -> dict[str, Any]:
    if not trace_path_value:
        return {}
    trace_path = Path(str(trace_path_value))
    if not trace_path.exists():
        return {}
    lines = [line for line in trace_path.read_text(encoding="utf-8").splitlines() if line.strip()]
    if not lines:
        return {}
    return json.loads(lines[-1])


def summarize(
    samples: list[dict[str, Any]],
    controls: dict[str, dict[str, Any]],
    metrics_delta: dict[str, float],
    profile: dict[str, Any],
    trace: dict[str, Any],
) -> dict[str, Any]:
    root = summarize_root(samples)
    attribution = as_dict(profile.get("attribution"))
    summary_counters = as_dict(attribution.get("summary_counters"))
    clones = as_dict(attribution.get("clones"))
    includes = as_dict(attribution.get("includes"))
    calls = as_dict(attribution.get("calls"))
    arrays = as_dict(attribution.get("arrays"))
    objects = as_dict(attribution.get("objects"))
    native = as_dict(attribution.get("native"))
    phases = as_dict(profile.get("phases_nanos")) or as_dict(trace.get("phases_nanos"))
    return {
        "root": root,
        "controls": {
            name: {
                "status": sample["status"],
                "body_bytes": sample["body_bytes"],
                "wall_ms": sample["wall_ms"],
            }
            for name, sample in controls.items()
        },
        "phases_nanos": phases,
        "core_counters": {key: summary_counters.get(key, 0) for key in COUNTER_KEYS},
        "metrics_delta": selected_metrics(metrics_delta),
        "top": {
            "value_clone_by_reason": top_entries(clones.get("value_clone_by_reason")),
            "value_clone_by_source_family": top_entries(clones.get("value_clone_by_source_family")),
            "array_handle_clone_by_source_family": top_entries(
                clones.get("array_handle_clone_by_source_family")
            ),
            "cow_separation_by_source_family": top_entries(clones.get("cow_separation_by_source_family")),
            "reference_cell_creation_by_source_family": top_entries(
                clones.get("reference_cell_creation_by_source_family")
            ),
            "dense_include_entry_fallback_by_reason": top_entries(
                includes.get("dense_include_entry_fallback_by_reason")
            ),
            "rich_fallback_functions_by_name": top_entries(
                as_dict(attribution.get("execution")).get("rich_fallback_functions_by_name")
            ),
            "builtin_profiles_by_name": top_entries(calls.get("builtin_profiles_by_name")),
            "array_operation_profiles_by_family": top_entries(arrays.get("operation_profiles_by_family")),
            "object_operation_profiles_by_family": top_entries(objects.get("operation_profiles_by_family")),
            "native_eligibility_rejections_by_reason": top_entries(
                native.get("native_eligibility_rejections_by_reason")
            ),
        },
    }


def summarize_root(samples: list[dict[str, Any]]) -> dict[str, Any]:
    walls = [sample["wall_ms"] for sample in samples]
    ttfbs = [sample["ttfb_ms"] for sample in samples]
    return {
        "samples": len(samples),
        "status": samples[-1]["status"] if samples else 0,
        "body_bytes": samples[-1]["body_bytes"] if samples else 0,
        "body_sha256": samples[-1]["body_sha256"] if samples else "",
        "wall_ms_min": min(walls) if walls else 0.0,
        "wall_ms_avg": sum(walls) / len(walls) if walls else 0.0,
        "wall_ms_max": max(walls) if walls else 0.0,
        "ttfb_ms_avg": sum(ttfbs) / len(ttfbs) if ttfbs else 0.0,
    }


def selected_metrics(metrics_delta: dict[str, float]) -> dict[str, float]:
    selected = {}
    for key, value in metrics_delta.items():
        if (
            "request_phase" in key
            or "script_cache" in key
            or "include_compile" in key
            or "runtime_diagnostics" in key
        ):
            selected[key] = value
    return selected


def compare_baseline(
    summary: dict[str, Any],
    baseline: dict[str, Any] | None,
    max_latency_regression_pct: float,
) -> list[str]:
    if baseline is None:
        return []
    failures: list[str] = []
    current = summary["root"]["wall_ms_avg"]
    baseline_root = as_dict(as_dict(baseline.get("summary")).get("root"))
    previous = baseline_root.get("wall_ms_avg")
    if isinstance(previous, (int, float)) and previous > 0:
        delta_pct = ((current - previous) / previous) * 100.0
        if delta_pct > max_latency_regression_pct:
            failures.append(
                f"root latency regressed by {delta_pct:.1f}% "
                f"(current {current:.3f} ms, baseline {previous:.3f} ms)"
            )
    return failures


def top_entries(value: Any, limit: int = 10) -> list[dict[str, Any]]:
    if not isinstance(value, list):
        return []
    return [entry for entry in value if isinstance(entry, dict)][:limit]


def as_dict(value: Any) -> dict[str, Any]:
    return value if isinstance(value, dict) else {}


def load_baseline(path_text: str) -> dict[str, Any] | None:
    path = repo_path(path_text) if path_text else None
    if path is None or not path.exists():
        return None
    return json.loads(path.read_text(encoding="utf-8"))


def baseline_summary(baseline: dict[str, Any] | None) -> dict[str, Any]:
    if baseline is None:
        return {"configured": False}
    root = as_dict(as_dict(baseline.get("summary")).get("root"))
    return {
        "configured": True,
        "wall_ms_avg": root.get("wall_ms_avg"),
        "body_sha256": root.get("body_sha256"),
    }


def skip_report(reason: str, blockers: list[str], args: argparse.Namespace, out_dir: Path) -> dict[str, Any]:
    return {
        "status": "skip",
        "reason": reason,
        "blockers": blockers,
        "inputs": {
            "url": args.url,
            "docroot": args.docroot or args.wordpress_dir,
            "wordpress_dir": args.wordpress_dir,
            "server": args.server,
        },
        "artifacts": artifact_paths(out_dir),
    }


def input_summary(args: argparse.Namespace, target: dict[str, Any]) -> dict[str, str]:
    return {
        "mode": str(target["mode"]),
        "base_url": str(target["base_url"]),
        "docroot": str(target.get("docroot", args.docroot or args.wordpress_dir)),
        "host_header": args.host_header,
        "server": str(target.get("server", args.server)),
        "samples": str(args.samples),
        "warmups": str(args.warmups),
    }


def artifact_paths(out_dir: Path, target: dict[str, Any] | None = None) -> dict[str, str]:
    artifacts = {
        "summary_json": rel(out_dir / "summary.json"),
        "summary_markdown": rel(out_dir / "summary.md"),
    }
    if target:
        if target.get("trace_path"):
            artifacts["trace"] = rel(Path(str(target["trace_path"])))
        if target.get("profile_dir"):
            artifacts["request_profiles"] = rel(Path(str(target["profile_dir"])))
        if target.get("log_path"):
            artifacts["server_log"] = rel(Path(str(target["log_path"])))
    return artifacts


def write_json(value: dict[str, Any], path: Path) -> None:
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def write_markdown(report: dict[str, Any], path: Path) -> None:
    lines = [
        "# WordPress Root Benchmark",
        "",
        f"Status: `{report['status']}`",
        "",
    ]
    if report["status"] == "skip":
        lines.extend(["## Skip Reason", "", ", ".join(report.get("blockers", [])), ""])
    elif report["status"] == "fail" and "error" in report:
        lines.extend(["## Error", "", f"```text\n{report['error']}\n```", ""])
    if "summary" in report:
        summary = report["summary"]
        root = summary["root"]
        lines.extend(
            [
                "## Root",
                "",
                f"- HTTP status: {root['status']}",
                f"- Body bytes: {root['body_bytes']}",
                f"- Body SHA-256: `{root['body_sha256']}`",
                f"- Wall avg: {root['wall_ms_avg']:.3f} ms",
                f"- Wall min/max: {root['wall_ms_min']:.3f} / {root['wall_ms_max']:.3f} ms",
                f"- TTFB avg: {root['ttfb_ms_avg']:.3f} ms",
                "",
                "## Core Counters",
                "",
            ]
        )
        for key, value in summary["core_counters"].items():
            lines.append(f"- `{key}`: {value}")
        lines.extend(["", "## Top Attribution", ""])
        for family, entries in summary["top"].items():
            lines.append(f"### `{family}`")
            if entries:
                for entry in entries:
                    metric = entry.get("count", entry.get("inclusive_nanos", 0))
                    lines.append(f"- `{entry.get('name', '')}`: {metric}")
            else:
                lines.append("- none")
            lines.append("")
        lines.extend(["## Controls", ""])
        for name, sample in summary["controls"].items():
            lines.append(
                f"- `{name}`: HTTP {sample['status']}, "
                f"{sample['body_bytes']} bytes, {sample['wall_ms']:.3f} ms"
            )
        lines.append("")
    failures = as_dict(report.get("correctness")).get("failures", [])
    if failures:
        lines.extend(["## Failures", ""])
        lines.extend(f"- {failure}" for failure in failures)
        lines.append("")
    lines.extend(["## Artifacts", ""])
    for key, value in report.get("artifacts", {}).items():
        lines.append(f"- `{key}`: `{value}`")
    path.write_text("\n".join(lines).rstrip() + "\n", encoding="utf-8")


def rel(path: Path) -> str:
    try:
        return str(path.relative_to(REPO_ROOT))
    except ValueError:
        return str(path)


def self_test() -> int:
    samples = [
        {
            "status": 200,
            "body_bytes": 12,
            "body_sha256": "abc",
            "wall_ms": 10.0,
            "ttfb_ms": 8.0,
        },
        {
            "status": 200,
            "body_bytes": 12,
            "body_sha256": "abc",
            "wall_ms": 12.0,
            "ttfb_ms": 9.0,
        },
    ]
    profile = {
        "phases_nanos": {"php_vm_execution": 1000},
        "attribution": {
            "summary_counters": {"vm_value_clones": 7},
            "clones": {
                "value_clone_by_source_family": [
                    {"name": "call_argument_snapshot", "count": 3}
                ]
            },
            "includes": {},
            "calls": {},
            "arrays": {},
            "objects": {},
            "native": {},
        },
    }
    summary = summarize(samples, {}, {}, profile, {})
    assert summary["root"]["wall_ms_avg"] == 11.0
    assert summary["core_counters"]["vm_value_clones"] == 7
    assert summary["top"]["value_clone_by_source_family"][0]["name"] == "call_argument_snapshot"
    failures = validate_root_samples(samples, {"summary": {"root": {"body_sha256": "abc"}}})
    assert failures == []
    assert request_host_header(urlparse("http://127.0.0.1:18081")) == "127.0.0.1:18081"
    regression = compare_baseline(
        summary,
        {"summary": {"root": {"wall_ms_avg": 5.0}}},
        max_latency_regression_pct=10.0,
    )
    assert regression
    print("[pass] wordpress_root_benchmark self-test")
    return 0


if __name__ == "__main__":
    sys.exit(main())
