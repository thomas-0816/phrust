#!/usr/bin/env python3
"""Runtime-semantics differential harness."""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Iterable


CATEGORIES = (
    "refs",
    "cow",
    "arrays",
    "strings",
    "foreach",
    "functions",
    "closures",
    "callables",
    "objects",
    "traits",
    "enums",
    "magic",
    "properties",
    "property_hooks",
    "clone_with",
    "void_cast",
    "const_expr",
    "conversions",
    "comparisons",
    "pipe",
    "types",
    "generators",
    "fibers",
    "reflection",
    "errors",
    "destructors",
    "gc",
    "includes",
    "include_eval_autoload",
    "constants",
    "namespaces",
    "globals",
    "superglobals",
    "variables",
    "statics",
    "real_world",
    "wordpress_blockers",
    "wp_language_vm",
    "wp_autoload_stdlib",
    "regressions",
    "known_gaps",
)

EXPECTATIONS = {"pass", "fail", "skip", "known_gap"}
FIXTURE_DATA_DIRS = {"_data"}


@dataclass
class Fixture:
    path: Path
    category: str
    expect: str = "pass"
    known_gap_id: str | None = None
    php_ref_required: bool = False
    php_ref_optional_reason: str | None = None
    args: list[str] = field(default_factory=list)
    metadata: dict[str, str] = field(default_factory=dict)


def main() -> int:
    try:
        report = run(parse_args())
    except HarnessError as error:
        print(f"[error] {error}", file=sys.stderr)
        return 2

    if report["summary"]["fail"]:
        print(
            f"[fail] runtime-semantics diff failures={report['summary']['fail']} "
            f"report={report['report_path']}",
            file=sys.stderr,
        )
        return 1

    print(
        "[ok] runtime-semantics diff report: "
        f"total={report['summary']['total']} "
        f"pass={report['summary']['pass']} "
        f"fail={report['summary']['fail']} "
        f"skip={report['summary']['skip']} "
        f"known_gap={report['summary']['known_gap']} "
        f"path={report['report_path']}"
    )
    return 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run PHP runtime-semantics fixtures against REFERENCE_PHP and php_vm_cli."
    )
    parser.add_argument("--fixtures", default="fixtures/runtime_semantics", help="runtime-semantics fixture root")
    parser.add_argument("--out", default="target/runtime-semantics/diff", help="output directory")
    parser.add_argument("--rust-vm", default=os.environ.get("PHP_VM_CLI", "target/debug/php-vm"))
    parser.add_argument("--file", action="append", default=[], help="single PHP file to compare")
    parser.add_argument("--dir", action="append", default=[], help="directory of PHP files")
    parser.add_argument(
        "--stop-on-fail",
        action="store_true",
        help="stop after the first failing fixture and still write the JSON report",
    )
    parser.add_argument(
        "--category",
        action="append",
        choices=CATEGORIES,
        default=[],
        help="runtime-semantics category to select",
    )
    parser.add_argument("paths", nargs="*", help="fixture files or directories to compare")
    return parser.parse_args()


def run(args: argparse.Namespace) -> dict:
    fixtures_root = Path(args.fixtures)
    out_dir = Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)

    fixtures = discover_fixtures(fixtures_root, args.file, args.dir, args.category, args.paths)
    results = []
    stopped_early = False
    for fixture in fixtures:
        result_item = compare_fixture(fixture, Path(args.rust_vm))
        results.append(result_item)
        if args.stop_on_fail and result_item["status"] == "fail":
            stopped_early = True
            break
    summary = summarize(results)
    report = {
        "fixtures_root": str(fixtures_root),
        "categories": list(CATEGORIES),
        "selected": len(fixtures),
        "stopped_early": stopped_early,
        "summary": summary,
        "results": results,
    }
    report_path = out_dir / "runtime-semantics-diff-report.json"
    report["report_path"] = str(report_path)
    report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return report


def discover_fixtures(
    fixtures_root: Path,
    files: Iterable[str],
    dirs: Iterable[str],
    categories: Iterable[str],
    paths: Iterable[str],
) -> list[Fixture]:
    selected: list[Path] = []
    explicit = False

    for item in paths:
        explicit = True
        path = Path(item)
        if path.is_dir():
            selected.extend(discover_php_files(path))
        else:
            selected.append(path)
    for item in files:
        explicit = True
        selected.append(Path(item))
    for item in dirs:
        explicit = True
        selected.extend(discover_php_files(Path(item)))
    for category in categories:
        explicit = True
        category_dir = fixtures_root / category
        if category_dir.exists():
            selected.extend(discover_php_files(category_dir))

    if not explicit and fixtures_root.exists():
        selected.extend(discover_php_files(fixtures_root))

    fixtures = [load_fixture(path, fixtures_root) for path in sorted(set(selected))]
    return fixtures


def discover_php_files(root: Path) -> list[Path]:
    return [
        path
        for path in sorted(root.rglob("*.php"))
        if not any(part in FIXTURE_DATA_DIRS for part in path.parts)
    ]


def load_fixture(path: Path, fixtures_root: Path) -> Fixture:
    if not path.is_file():
        raise HarnessError(f"fixture is not a file: {path}")
    category = infer_category(path, fixtures_root)
    fixture = Fixture(path=path, category=category)
    if category == "known_gaps":
        fixture.expect = "known_gap"

    for line in path.read_text(encoding="utf-8", errors="replace").splitlines()[:8]:
        text = line.strip()
        prefix = None
        for candidate in ("// runtime-semantics:", "# runtime-semantics:"):
            if text.startswith(candidate):
                prefix = candidate
                break
        if prefix is None:
            continue
        for item in text[len(prefix) :].split():
            if "=" not in item:
                continue
            key, value = item.split("=", 1)
            value = value.strip('"')
            if key == "expect":
                if value not in EXPECTATIONS:
                    raise HarnessError(f"{path}: unsupported expectation {value!r}")
                fixture.expect = value
            elif key == "known_gap":
                fixture.known_gap_id = value
            elif key == "php_ref_required":
                fixture.php_ref_required = value in {"1", "true", "yes"}
            elif key == "php_ref_optional_reason":
                fixture.php_ref_optional_reason = value
            elif key == "args":
                fixture.args = [part for part in value.split(",") if part]
            else:
                fixture.metadata[key] = value
    validate_fixture_metadata(fixture)
    return fixture


def validate_fixture_metadata(fixture: Fixture) -> None:
    if fixture.category == "wp_language_vm":
        required = ("wordpress_error_class", "fixture_id", "wp_area")
        missing = [key for key in required if key not in fixture.metadata]
        if missing:
            raise HarnessError(
                f"{fixture.path}: wp_language_vm fixture missing metadata: {', '.join(missing)}"
            )
        supported_classes = {
            "frontend_lowering",
            "runtime_dispatch",
            "runtime_semantics",
        }
        error_class = fixture.metadata["wordpress_error_class"]
        if error_class not in supported_classes:
            raise HarnessError(
                f"{fixture.path}: unsupported wordpress_error_class {error_class!r}"
            )

    if fixture.category != "regressions":
        return
    required = ("regression_category", "reference_behavior", "regression_case")
    missing = [key for key in required if key not in fixture.metadata]
    if missing:
        raise HarnessError(
            f"{fixture.path}: regression fixture missing metadata: {', '.join(missing)}"
        )
    parts = fixture.path.parts
    if "known_gaps" in parts:
        if fixture.expect != "known_gap":
            raise HarnessError(
                f"{fixture.path}: known-gap regressions must declare expect=known_gap"
            )
        if not fixture.known_gap_id:
            raise HarnessError(f"{fixture.path}: known-gap regression must declare known_gap=<ID>")
    elif "pass" in parts and fixture.expect != "pass":
        raise HarnessError(f"{fixture.path}: pass regressions must declare expect=pass")


def infer_category(path: Path, fixtures_root: Path) -> str:
    try:
        relative = path.relative_to(fixtures_root)
    except ValueError:
        return "ad_hoc"
    if relative.parts and relative.parts[0] in CATEGORIES:
        return relative.parts[0]
    return "ad_hoc"


def compare_fixture(fixture: Fixture, rust_vm: Path) -> dict:
    if fixture.expect == "skip":
        return result(fixture, "skip", None, None, "fixture metadata requested skip")

    reference = run_reference(fixture)
    rust = run_rust(fixture, rust_vm)

    if fixture.expect == "known_gap":
        if not fixture.known_gap_id:
            return result(
                fixture,
                "fail",
                reference,
                rust,
                "known-gap fixture must declare runtime-semantics known_gap=<ID>",
            )
        return result(fixture, "known_gap", reference, rust, None)

    if reference["status"] == "skip":
        if not fixture.php_ref_required:
            reason = fixture.php_ref_optional_reason or reference["message"]
            return result(fixture, "skip", reference, rust, reason)
        return result(
            fixture,
            "fail",
            reference,
            rust,
            (
                f"{reference['message']}; runnable runtime-semantics fixtures "
                "with php_ref_required=1 require REFERENCE_PHP"
            ),
        )
    if reference["status"] == "error":
        return result(fixture, "fail", reference, rust, reference["message"])
    if rust["status"] == "error":
        return result(fixture, "fail", reference, rust, rust["message"])

    if fixture.expect == "fail":
        if rust["exit_code"] not in (None, 0):
            return result(fixture, "pass", reference, rust, None)
        return result(fixture, "fail", reference, rust, "fixture was expected to fail")

    differences = normalized_differences(reference, rust)
    status = "pass" if not differences else "fail"
    return result(fixture, status, reference, rust, "; ".join(differences) or None)


def run_reference(fixture: Fixture) -> dict:
    php = os.environ.get("REFERENCE_PHP")
    if not php:
        return {"status": "skip", "message": "REFERENCE_PHP is not set"}
    php_path = Path(php)
    if not php_path.is_file():
        return {"status": "error", "message": f"REFERENCE_PHP is not a file: {php}"}
    return run_process([str(php_path), str(fixture.path), *fixture.args], fixture.path, php_path)


def run_rust(fixture: Fixture, rust_vm: Path) -> dict:
    if rust_vm.is_file():
        command = [str(rust_vm), "run", str(fixture.path)]
        if fixture.args:
            command.extend(["--", *fixture.args])
        return run_process(command, fixture.path, None)
    command = ["cargo", "run", "-p", "php_vm_cli", "--", "run", str(fixture.path)]
    if fixture.args:
        command.extend(["--", *fixture.args])
    return run_process(command, fixture.path, None)


def run_process(command: list[str], fixture_path: Path, php_path: Path | None) -> dict:
    env = {
        "LC_ALL": "C",
        "LANG": "C",
        "NO_COLOR": "1",
        "PHP_INI_SCAN_DIR": "",
        "PATH": os.environ.get("PATH", ""),
    }
    try:
        completed = subprocess.run(command, check=False, capture_output=True, env=env, text=True)
    except OSError as error:
        return {"status": "error", "message": f"failed to execute {command[0]}: {error}"}
    stderr = normalize_stderr(completed.stderr, fixture_path, php_path)
    return {
        "status": "completed",
        "exit_code": completed.returncode,
        "stdout": completed.stdout,
        "stderr": completed.stderr,
        "stderr_normalized": stderr,
    }


def normalize_stderr(stderr: str, fixture_path: Path, php_path: Path | None) -> str:
    normalized = stderr.replace("\r\n", "\n").replace("\r", "\n")
    normalized = replace_path(normalized, fixture_path, "{file}")
    if php_path is not None:
        normalized = replace_path(normalized, php_path, "{php}")
    normalized = re.sub(r" on line \d+", " on line <line>", normalized)
    normalized = re.sub(r":\d+:\d+", ":<line>:<column>", normalized)
    return normalized


def replace_path(text: str, path: Path, replacement: str) -> str:
    output = text.replace(str(path), replacement)
    try:
        output = output.replace(str(path.resolve()), replacement)
    except OSError:
        pass
    return output


def normalized_differences(reference: dict, rust: dict) -> list[str]:
    differences = []
    for key in ("exit_code", "stdout", "stderr_normalized"):
        if reference.get(key) != rust.get(key):
            differences.append(f"{key} reference={reference.get(key)!r} rust={rust.get(key)!r}")
    return differences


def result(
    fixture: Fixture,
    status: str,
    reference: dict | None,
    rust: dict | None,
    message: str | None,
) -> dict:
    diagnostics = diagnostic_entries(rust)
    return {
        "file": str(fixture.path),
        "category": fixture.category,
        "expect": fixture.expect,
        "status": status,
        "failure_category": failure_category(fixture, status, reference, rust, message),
        "wordpress_error_class": fixture.metadata.get("wordpress_error_class"),
        "fixture_id": fixture.metadata.get("fixture_id"),
        "diagnostic_ids": [entry["id"] for entry in diagnostics if entry.get("id")],
        "primary_diagnostic": diagnostics[0] if diagnostics else None,
        "unsupported_operation": first_unsupported_operation(diagnostics, rust, message),
        "reduced_fixture": fixture.category == "wp_language_vm",
        "known_gap_id": fixture.known_gap_id,
        "metadata": fixture.metadata,
        "reference": reference,
        "rust": rust,
        "message": message,
    }


def failure_category(
    fixture: Fixture,
    status: str,
    reference: dict | None,
    rust: dict | None,
    message: str | None,
) -> str | None:
    if status not in {"fail", "known_gap"}:
        return None
    explicit = fixture.metadata.get("failure_category")
    if explicit:
        return explicit
    text = " ".join(
        str(value or "")
        for value in (
            message,
            rust.get("stderr_normalized") if rust else None,
            rust.get("stderr") if rust else None,
            reference.get("stderr_normalized") if reference else None,
            reference.get("stderr") if reference else None,
        )
    )
    if "E_PHP_PARSE" in text or "parser" in text.lower():
        return "parser"
    if "E_PHP_SEMANTIC" in text or "semantic" in text.lower():
        return "semantic_folding"
    if "E_PHP_IR" in text or "unsupported hir" in text.lower():
        return "ir_lowering"
    if "constant" in text.lower() or "defined" in text.lower():
        return "predefined_constant"
    if "error_reporting" in text or "Warning:" in text or "Fatal error:" in text:
        return "error_reporting"
    if "include" in text.lower() or "require" in text.lower() or "cache" in text.lower():
        return "include_cache_runaway"
    if rust and rust.get("status") == "error":
        return "vm_runtime"
    return "vm_runtime"


def diagnostic_entries(rust: dict | None) -> list[dict]:
    if not rust:
        return []
    entries: list[dict] = []
    text = "\n".join(
        str(value or "")
        for value in (rust.get("stderr_normalized"), rust.get("stderr"))
    )
    seen: set[str] = set()
    for line in text.splitlines():
        if "runtime-diagnostic:" not in line:
            continue
        payload = line.split("runtime-diagnostic:", 1)[1].strip()
        try:
            entry = json.loads(payload)
        except json.JSONDecodeError:
            continue
        key = json.dumps(entry, sort_keys=True)
        if key in seen:
            continue
        seen.add(key)
        entries.append(entry)
    for diagnostic_id in re.findall(r"\bE_PHP_[A-Z0-9_]+\b", text):
        if any(entry.get("id") == diagnostic_id for entry in entries):
            continue
        entries.append({"id": diagnostic_id})
    return entries


def first_unsupported_operation(
    diagnostics: list[dict],
    rust: dict | None,
    message: str | None,
) -> str | None:
    text = " ".join(
        str(value or "")
        for value in (
            message,
            *(entry.get("message") for entry in diagnostics),
            rust.get("stderr_normalized") if rust else None,
            rust.get("stderr") if rust else None,
        )
    )
    match = re.search(
        r"(unsupported [^.;\n]+|not implemented[^.;\n]*|unknown [^.;\n]+)",
        text,
        flags=re.IGNORECASE,
    )
    if match:
        return match.group(1).strip()
    return None


def summarize(results: list[dict]) -> dict:
    return {
        "total": len(results),
        "pass": sum(1 for item in results if item["status"] == "pass"),
        "fail": sum(1 for item in results if item["status"] == "fail"),
        "skip": sum(1 for item in results if item["status"] == "skip"),
        "known_gap": sum(1 for item in results if item["status"] == "known_gap"),
    }


class HarnessError(Exception):
    """Runtime-semantics harness configuration error."""


if __name__ == "__main__":
    raise SystemExit(main())
