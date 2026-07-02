#!/usr/bin/env python3
"""Run real WordPress smoke phases through phrust."""

from __future__ import annotations

import argparse
import json
import os
import signal
import subprocess
import sys
import time
from pathlib import Path
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.parse import urlencode
from urllib.request import Request, urlopen

SCRIPT_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPT_DIR))

from common import (  # noqa: E402
    DB_PHASES,
    REPO_ROOT,
    classify_failure,
    environment_failure,
    excerpt,
    extract_diagnostics,
    json_dump,
    line_for_byte_offset,
    now_run_id,
    owner_suggestion,
    repo_path,
    runtime_stack,
    span_source_path,
    span_start,
)
from preflight import build_report as build_preflight_report  # noqa: E402


PHASES = (
    "syntax-scan",
    "cli-bootstrap",
    "web-frontpage",
    "web-install-page",
    "db-install",
    "admin-login-page",
    "post-install-frontpage",
)

WEB_PHASES = {
    "web-frontpage",
    "web-install-page",
    "db-install",
    "admin-login-page",
    "post-install-frontpage",
}


def main() -> int:
    args = parse_args()
    out_dir = output_dir(args)
    out_dir.mkdir(parents=True, exist_ok=True)
    report, first_failure = run_smoke(args, out_dir)
    json_dump(report, out_dir / "wordpress-smoke-report.json")
    json_dump(first_failure or {}, out_dir / "first-failure.json")
    print(json.dumps(report["summary"], indent=2, sort_keys=True))
    return 0 if report["summary"]["status"] in {"pass", "skip"} else 1


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--wordpress-dir", default=os.environ.get("PHRUST_WORDPRESS_DIR", ""))
    parser.add_argument("--docroot", default=os.environ.get("PHRUST_WORDPRESS_DOCROOT", ""))
    parser.add_argument("--out", default="")
    parser.add_argument("--reference-php", default=os.environ.get("REFERENCE_PHP", ""))
    parser.add_argument("--phrust-binary", default=os.environ.get("PHP_VM_CLI", "target/debug/php-vm"))
    parser.add_argument("--phrust-server", default=os.environ.get("PHRUST_SERVER", "target/debug/phrust-server"))
    parser.add_argument("--db-dsn-env", default="PHRUST_MYSQL_TEST_DSN")
    parser.add_argument("--stop-on-fail", action="store_true")
    parser.add_argument("--timeout-seconds", type=int, default=int(os.environ.get("PHRUST_WORDPRESS_TIMEOUT_SECONDS", "30")))
    parser.add_argument("--phase", choices=PHASES, action="append", default=[])
    return parser.parse_args()


def output_dir(args: argparse.Namespace) -> Path:
    if args.out:
        return repo_path(args.out) or Path(args.out)
    phase = "-".join(args.phase or ["web-frontpage"])
    return REPO_ROOT / "target" / "wordpress-real" / now_run_id(phase)


def run_smoke(args: argparse.Namespace, out_dir: Path) -> tuple[dict[str, Any], dict[str, Any] | None]:
    phases = args.phase or ["web-frontpage"]
    db_enabled = any(phase in DB_PHASES for phase in phases)
    preflight_args = argparse.Namespace(
        wordpress_dir=args.wordpress_dir,
        docroot=args.docroot,
        reference_php=args.reference_php,
        require_reference=any(phase == "syntax-scan" for phase in phases),
        phrust_binary=args.phrust_binary,
        phrust_server=args.phrust_server,
        db_enabled=db_enabled,
        db_dsn_env=args.db_dsn_env,
        listen="127.0.0.1:0",
        out="",
    )
    preflight = build_preflight_report(preflight_args)
    json_dump(preflight, out_dir / "preflight.json")

    transcript_path = out_dir / "http-transcript.jsonl"
    transcript_path.write_text("", encoding="utf-8")
    server_log = out_dir / "server.log"
    server_log.write_text("", encoding="utf-8")

    if preflight["status"] != "ok":
        first_failure = environment_failure(preflight["environment_blockers"], preflight["inputs"])
        report = base_report("skip", phases[0], "environment", preflight, [], first_failure, out_dir)
        return report, first_failure

    results: list[dict[str, Any]] = []
    first_failure: dict[str, Any] | None = None
    shared_server: dict[str, Any] | None = None
    try:
        for phase in phases:
            if phase in WEB_PHASES and shared_server is None:
                shared_server = start_server(args, server_log)
            result = run_phase(phase, args, out_dir, transcript_path, server_log, shared_server)
            results.append(result)
            if result["status"] == "fail" and first_failure is None:
                first_failure = result["first_failure"]
                if args.stop_on_fail:
                    break
    except TimeoutError as error:
        phase = next((phase for phase in phases if phase in WEB_PHASES), phases[0])
        first_failure = command_failure(phase, None, None, None, "", str(error), [], timed_out=True)
        results.append({"phase": phase, "status": "fail", "first_failure": first_failure})
    except (OSError, URLError) as error:
        phase = next((phase for phase in phases if phase in WEB_PHASES), phases[0])
        first_failure = command_failure(phase, None, None, None, "", str(error), [])
        first_failure["first_failure_class"] = "web"
        first_failure["candidate_owner_layer"] = "php_server"
        results.append({"phase": phase, "status": "fail", "first_failure": first_failure})
    finally:
        if shared_server is not None:
            stop_server(shared_server["process"])

    if first_failure is None:
        report = base_report("pass", phases[-1], None, preflight, results, None, out_dir)
    else:
        report = base_report(
            "fail",
            first_failure.get("phase", phases[0]),
            first_failure.get("first_failure_class", "runtime"),
            preflight,
            results,
            first_failure,
            out_dir,
        )
    return report, first_failure


def base_report(
    status: str,
    phase: str,
    first_failure_class: str | None,
    preflight: dict[str, Any],
    results: list[dict[str, Any]],
    first_failure: dict[str, Any] | None,
    out_dir: Path,
) -> dict[str, Any]:
    return {
        "summary": {
            "status": status,
            "phase": phase,
            "first_failure_class": first_failure_class,
        },
        "preflight": preflight,
        "results": results,
        "first_failure": first_failure,
        "artifacts": {
            "report": str(out_dir / "wordpress-smoke-report.json"),
            "first_failure": str(out_dir / "first-failure.json"),
            "server_log": str(out_dir / "server.log"),
            "http_transcript": str(out_dir / "http-transcript.jsonl"),
        },
    }


def run_phase(
    phase: str,
    args: argparse.Namespace,
    out_dir: Path,
    transcript_path: Path,
    server_log: Path,
    shared_server: dict[str, Any] | None,
) -> dict[str, Any]:
    if phase == "syntax-scan":
        return run_syntax_scan(args, out_dir)
    if phase == "cli-bootstrap":
        return run_cli_bootstrap(args, out_dir)
    assert shared_server is not None
    return run_web_phase(phase, args, transcript_path, server_log, shared_server)


def run_syntax_scan(args: argparse.Namespace, out_dir: Path) -> dict[str, Any]:
    reference_php = repo_path(args.reference_php)
    wordpress_dir = repo_path(args.wordpress_dir)
    assert reference_php is not None
    assert wordpress_dir is not None
    failures = []
    checked = 0
    for path in sorted(wordpress_dir.rglob("*.php")):
        checked += 1
        try:
            completed = subprocess.run(
                [str(reference_php), "-l", str(path)],
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                timeout=args.timeout_seconds,
            )
        except subprocess.TimeoutExpired as error:
            first_failure = command_failure("syntax-scan", None, None, None, "", str(error), [], timed_out=True)
            return {"phase": "syntax-scan", "status": "fail", "first_failure": first_failure}
        if completed.returncode != 0:
            diagnostics = extract_diagnostics(completed.stderr, completed.stdout)
            first_failure = command_failure(
                "syntax-scan",
                completed.returncode,
                None,
                None,
                completed.stdout,
                completed.stderr,
                diagnostics,
            )
            first_failure["source_path"] = str(path)
            failures.append(first_failure)
            break
    return {"phase": "syntax-scan", "status": "pass", "checked_files": checked, "out": str(out_dir)} if not failures else {"phase": "syntax-scan", "status": "fail", "first_failure": failures[0]}


def run_cli_bootstrap(args: argparse.Namespace, out_dir: Path) -> dict[str, Any]:
    phrust_binary = repo_path(args.phrust_binary)
    wordpress_dir = repo_path(args.wordpress_dir)
    assert phrust_binary is not None
    assert wordpress_dir is not None
    index = wordpress_dir / "index.php"
    try:
        completed = subprocess.run(
            [str(phrust_binary), "run", "--error-format=json", str(index)],
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=args.timeout_seconds,
            env=runtime_env(args),
        )
    except subprocess.TimeoutExpired as error:
        first_failure = command_failure("cli-bootstrap", None, None, None, "", str(error), [], timed_out=True)
        return {"phase": "cli-bootstrap", "status": "fail", "first_failure": first_failure}
    diagnostics = extract_diagnostics(completed.stderr, completed.stdout)
    if completed.returncode != 0:
        first_failure = command_failure(
            "cli-bootstrap",
            completed.returncode,
            None,
            None,
            completed.stdout,
            completed.stderr,
            diagnostics,
        )
        return {"phase": "cli-bootstrap", "status": "fail", "first_failure": first_failure}
    (out_dir / "cli-bootstrap.stdout").write_text(completed.stdout, encoding="utf-8")
    (out_dir / "cli-bootstrap.stderr").write_text(completed.stderr, encoding="utf-8")
    return {"phase": "cli-bootstrap", "status": "pass", "exit_code": completed.returncode}


def run_web_phase(
    phase: str,
    args: argparse.Namespace,
    transcript_path: Path,
    server_log: Path,
    server: dict[str, Any],
) -> dict[str, Any]:
    try:
        request = request_for_phase(phase)
        response = perform_request(server["base_url"], request, args.timeout_seconds)
        append_jsonl(transcript_path, {"phase": phase, "request": request, "response": response})
        log_text = server_log.read_text(encoding="utf-8", errors="replace")
        if response["http_status"] >= 500:
            diagnostics = extract_diagnostics(log_text, response["body_excerpt"])
            first_failure = command_failure(
                phase,
                None,
                response["http_status"],
                request,
                response["body_excerpt"],
                log_text,
                diagnostics,
            )
            return {"phase": phase, "status": "fail", "request": request, "response": response, "first_failure": first_failure}
        return {"phase": phase, "status": "pass", "request": request, "response": response}
    except TimeoutError as error:
        first_failure = command_failure(phase, None, None, None, "", str(error), [], timed_out=True)
        return {"phase": phase, "status": "fail", "first_failure": first_failure}
    except (OSError, URLError) as error:
        first_failure = command_failure(phase, None, None, None, "", str(error), [])
        first_failure["first_failure_class"] = "web"
        first_failure["candidate_owner_layer"] = "php_server"
        return {"phase": phase, "status": "fail", "first_failure": first_failure}


def start_server(args: argparse.Namespace, server_log: Path) -> dict[str, Any]:
    phrust_server = repo_path(args.phrust_server)
    docroot = repo_path(args.docroot) or repo_path(args.wordpress_dir)
    assert phrust_server is not None
    assert docroot is not None
    log_start = server_log.stat().st_size if server_log.exists() else 0
    command = [
        str(phrust_server),
        "--docroot",
        str(docroot),
        "--listen",
        "127.0.0.1:0",
        "--front-controller",
        "index.php",
        "--debug",
        "--error-format",
        "json",
        "--max-execution-ms",
        str(max(1, args.timeout_seconds) * 1000),
    ]
    log_handle = server_log.open("a", encoding="utf-8")
    process = subprocess.Popen(
        command,
        cwd=REPO_ROOT,
        text=True,
        stdout=log_handle,
        stderr=subprocess.STDOUT,
        env=runtime_env(args),
    )
    base_url = wait_for_server_address(server_log, process, args.timeout_seconds, log_start)
    log_handle.close()
    return {"process": process, "base_url": base_url}


def wait_for_server_address(
    server_log: Path,
    process: subprocess.Popen[str],
    timeout_seconds: int,
    log_start: int = 0,
) -> str:
    deadline = time.time() + timeout_seconds
    while time.time() < deadline:
        if process.poll() is not None:
            raise OSError(f"phrust-server exited before listening with status {process.returncode}")
        with server_log.open("r", encoding="utf-8", errors="replace") as handle:
            handle.seek(log_start)
            text = handle.read()
        for line in text.splitlines():
            if line.startswith("listening http://"):
                return line.removeprefix("listening ").strip()
        time.sleep(0.05)
    raise TimeoutError("phrust-server did not print a listening address")


def stop_server(process: subprocess.Popen[str]) -> None:
    if process.poll() is not None:
        return
    process.send_signal(signal.SIGTERM)
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=5)


def request_for_phase(phase: str) -> dict[str, Any]:
    if phase == "web-install-page":
        return {"method": "GET", "path": "/wp-admin/install.php", "headers": {}, "body": None}
    if phase == "db-install":
        form = {
            "weblog_title": "Phrust Smoke",
            "user_name": "phrust_admin",
            "admin_password": "phrust-smoke-password-123",
            "admin_password2": "phrust-smoke-password-123",
            "admin_email": "phrust@example.test",
            "blog_public": "0",
            "Submit": "Install WordPress",
        }
        return {
            "method": "POST",
            "path": "/wp-admin/install.php?step=2",
            "headers": {"Content-Type": "application/x-www-form-urlencoded"},
            "body": urlencode(form),
        }
    if phase == "admin-login-page":
        return {"method": "GET", "path": "/wp-login.php", "headers": {}, "body": None}
    return {"method": "GET", "path": "/", "headers": {}, "body": None}


def perform_request(base_url: str, request: dict[str, Any], timeout_seconds: int) -> dict[str, Any]:
    body = request.get("body")
    data = body.encode("utf-8") if isinstance(body, str) else None
    http_request = Request(
        base_url.rstrip("/") + request["path"],
        data=data,
        headers=request.get("headers") or {},
        method=request["method"],
    )
    try:
        with urlopen(http_request, timeout=timeout_seconds) as response:
            response_body = response.read().decode("utf-8", errors="replace")
            return {
                "http_status": response.status,
                "headers": dict(response.headers.items()),
                "body_excerpt": excerpt(response_body),
            }
    except HTTPError as error:
        response_body = error.read().decode("utf-8", errors="replace")
        return {
            "http_status": error.code,
            "headers": dict(error.headers.items()),
            "body_excerpt": excerpt(response_body),
        }


def command_failure(
    phase: str,
    exit_code: int | None,
    http_status: int | None,
    request: dict[str, Any] | None,
    stdout: str,
    stderr: str,
    diagnostics: list[dict[str, Any]],
    timed_out: bool = False,
) -> dict[str, Any]:
    failure_class, owner = classify_failure(diagnostics, f"{stdout}\n{stderr}", timed_out)
    first_diag = diagnostics[0] if diagnostics else {}
    source_path = span_source_path(first_diag)
    line = line_for_byte_offset(source_path, span_start(first_diag))
    diagnostic_ids = [diag.get("id") for diag in diagnostics if isinstance(diag.get("id"), str)]
    first_id = diagnostic_ids[0] if diagnostic_ids else None
    return {
        "phase": phase,
        "first_failure_class": failure_class,
        "request": request,
        "exit_code": exit_code,
        "http_status": http_status,
        "diagnostic_ids": diagnostic_ids,
        "source_path": source_path,
        "line": line,
        "include_stack": [],
        "autoload_stack": [],
        "runtime_stack": runtime_stack(first_diag),
        "stdout_excerpt": excerpt(stdout),
        "stderr_excerpt": excerpt(stderr),
        "candidate_owner_layer": owner,
        "owner_suggestion": owner_suggestion(failure_class, first_id),
    }


def runtime_env(args: argparse.Namespace) -> dict[str, str]:
    env = os.environ.copy()
    dsn = os.environ.get(args.db_dsn_env)
    if dsn:
        env[args.db_dsn_env] = dsn
    env.setdefault("PHRUST_ERROR_FORMAT", "json")
    env.setdefault("PHRUST_NET_TESTS", "1")
    return env


def append_jsonl(path: Path, item: dict[str, Any]) -> None:
    with path.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(item, sort_keys=True) + "\n")


if __name__ == "__main__":
    raise SystemExit(main())
