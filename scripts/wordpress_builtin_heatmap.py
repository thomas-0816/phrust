#!/usr/bin/env python3
"""Build a WordPress bring-up builtin/class/callable failure heatmap."""

from __future__ import annotations

import argparse
import json
import re
import tempfile
from collections import Counter
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable


DEFAULT_INPUT_GLOB = "target/runtime-semantics/**/runtime-semantics-diff-report.json"
DEFAULT_OUT = "target/wordpress-bringup"


@dataclass
class HeatmapItem:
    category: str
    name: str
    owner: str
    first_source_location: str
    first_stack_frame: str | None
    count: int
    diagnostic_id: str | None
    recommended_fixture_path: str
    phpt_coverage_exists: bool
    examples: list[str]


def main() -> int:
    args = parse_args()
    if args.self_test:
        self_test()
        return 0
    reports = [Path(item) for item in args.input] if args.input else sorted(Path().glob(DEFAULT_INPUT_GLOB))
    items = build_heatmap(reports, Path(args.symbol_manifest))
    write_outputs(items, reports, Path(args.out))
    return 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", action="append", default=[], help="runtime-semantics diff report JSON")
    parser.add_argument("--out", default=DEFAULT_OUT, help="output directory")
    parser.add_argument(
        "--symbol-manifest",
        default="tests/phpt/manifests/php-src-symbols.jsonl",
        help="php-src symbol manifest used for owner and PHPT coverage hints",
    )
    parser.add_argument("--self-test", action="store_true", help="run script self-test")
    return parser.parse_args()


def build_heatmap(reports: Iterable[Path], symbol_manifest: Path) -> list[HeatmapItem]:
    owners, phpt_symbols = load_symbol_index(symbol_manifest)
    grouped: dict[tuple[str, str, str | None], dict] = {}
    for report_path in reports:
        if not report_path.is_file():
            continue
        report = json.loads(report_path.read_text(encoding="utf-8"))
        for result in report.get("results", []):
            if result.get("status") not in {"fail", "known_gap"}:
                continue
            text = result_text(result)
            category, name = classify(text)
            if category is None:
                continue
            diagnostic_id = first_diagnostic_id(text)
            key = (category, name, diagnostic_id)
            entry = grouped.setdefault(
                key,
                {
                    "count": 0,
                    "examples": [],
                    "first_source_location": result.get("file") or "",
                    "first_stack_frame": first_stack_frame(result),
                },
            )
            entry["count"] += 1
            if len(entry["examples"]) < 3:
                entry["examples"].append(result.get("file") or str(report_path))

    items = []
    for (category, name, diagnostic_id), entry in sorted(
        grouped.items(), key=lambda item: (-item[1]["count"], item[0])
    ):
        owner = owners.get(name.lower(), owner_from_category(category))
        items.append(
            HeatmapItem(
                category=category,
                name=name,
                owner=owner,
                first_source_location=entry["first_source_location"],
                first_stack_frame=entry["first_stack_frame"],
                count=entry["count"],
                diagnostic_id=diagnostic_id,
                recommended_fixture_path=recommended_fixture_path(category, name),
                phpt_coverage_exists=name.lower() in phpt_symbols,
                examples=entry["examples"],
            )
        )
    return items


def load_symbol_index(path: Path) -> tuple[dict[str, str], set[str]]:
    owners: dict[str, str] = {}
    covered: set[str] = set()
    if not path.is_file():
        return owners, covered
    for line in path.read_text(encoding="utf-8").splitlines():
        try:
            item = json.loads(line)
        except json.JSONDecodeError:
            continue
        name = str(item.get("php_name") or "").lower()
        if not name:
            continue
        owners.setdefault(name, str(item.get("module") or "unknown"))
        if str(item.get("path") or "").endswith(".phpt") or "/tests/" in str(item.get("path") or ""):
            covered.add(name)
    return owners, covered


def result_text(result: dict) -> str:
    parts = [
        result.get("message"),
        result.get("failure_category"),
        nested(result, "rust", "stderr_normalized"),
        nested(result, "rust", "stderr"),
        nested(result, "rust", "stdout"),
        nested(result, "reference", "stderr_normalized"),
        nested(result, "reference", "stderr"),
        nested(result, "reference", "stdout"),
    ]
    return "\n".join(str(part) for part in parts if part)


def nested(mapping: dict, *keys: str) -> object | None:
    current: object = mapping
    for key in keys:
        if not isinstance(current, dict):
            return None
        current = current.get(key)
    return current


def classify(text: str) -> tuple[str | None, str]:
    lowered = text.lower()
    if "callable" in lowered or "call_user_func" in lowered:
        return "callable_resolution_failure", extract_name(text, "callable")
    if "undefined function" in lowered or "function " in lowered and "not found" in lowered:
        return "missing_function", extract_name(text, "function")
    if "undefined constant" in lowered or "constant" in lowered and "not defined" in lowered:
        return "missing_constant", extract_name(text, "constant")
    if "class " in lowered and ("not found" in lowered or "does not exist" in lowered or "not defined" in lowered):
        return "missing_class", extract_name(text, "class")
    if "extension" in lowered and ("not loaded" in lowered or "missing" in lowered):
        return "missing_extension", extract_name(text, "extension")
    if "arity" in lowered or "argumentcounterror" in lowered or "too few" in lowered or "too many" in lowered:
        return "wrong_arity", extract_name(text, "arity")
    if "typeerror" in lowered or "must be of type" in lowered or "wrong type" in lowered:
        return "wrong_type", extract_name(text, "type")
    if "return" in lowered and "mismatch" in lowered:
        return "wrong_return", extract_name(text, "return")
    if "warning:" in lowered or "fatal error:" in lowered or "stderr_normalized" in lowered:
        return "wrong_warning_or_error", extract_name(text, "diagnostic")
    return None, "unknown"


def extract_name(text: str, fallback: str) -> str:
    patterns = [
        r'function "([^"]+)"',
        r"undefined function ([A-Za-z_\\][A-Za-z0-9_\\]*)",
        r'Class "([^"]+)"',
        r"class ([A-Za-z_\\][A-Za-z0-9_\\]*)",
        r'constant "([^"]+)"',
        r"constant ([A-Za-z_\\][A-Za-z0-9_\\]*)",
        r"extension ([A-Za-z_][A-Za-z0-9_]*)",
    ]
    for pattern in patterns:
        match = re.search(pattern, text, re.IGNORECASE)
        if match:
            return match.group(1).strip("\\")
    return fallback


def first_diagnostic_id(text: str) -> str | None:
    match = re.search(r"\bE_[A-Z0-9_]+\b", text)
    return match.group(0) if match else None


def first_stack_frame(result: dict) -> str | None:
    stderr = str(nested(result, "rust", "stderr") or "")
    match = re.search(r"\n\s+at\s+([^\n]+)", stderr)
    return match.group(1).strip() if match else None


def owner_from_category(category: str) -> str:
    return {
        "missing_extension": "extension",
        "callable_resolution_failure": "zend.callables",
        "wrong_arity": "arginfo",
        "wrong_type": "arginfo",
    }.get(category, "unknown")


def recommended_fixture_path(category: str, name: str) -> str:
    slug = re.sub(r"[^a-z0-9]+", "-", name.lower()).strip("-") or "unknown"
    folder = {
        "callable_resolution_failure": "callables",
        "missing_class": "feature_detection",
        "missing_constant": "feature_detection",
        "missing_extension": "feature_detection",
        "missing_function": "core_builtins",
        "wrong_arity": "core_builtins",
        "wrong_type": "core_builtins",
        "wrong_return": "core_builtins",
        "wrong_warning_or_error": "core_builtins",
    }.get(category, "core_builtins")
    return f"fixtures/runtime_semantics/wp_autoload_stdlib/{folder}/{slug}.php"


def write_outputs(items: list[HeatmapItem], reports: list[Path], out_dir: Path) -> None:
    out_dir.mkdir(parents=True, exist_ok=True)
    payload = {
        "reports": [str(path) for path in reports if path.is_file()],
        "summary": {
            "total": sum(item.count for item in items),
            "unique": len(items),
            "by_category": dict(Counter(item.category for item in items)),
        },
        "items": [item.__dict__ for item in items],
    }
    (out_dir / "builtin-heatmap.json").write_text(
        json.dumps(payload, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    (out_dir / "builtin-heatmap.md").write_text(markdown(payload), encoding="utf-8")


def markdown(payload: dict) -> str:
    lines = [
        "# WordPress Bring-up Builtin Heatmap",
        "",
        f"Total observations: {payload['summary']['total']}",
        f"Unique rows: {payload['summary']['unique']}",
        "",
        "| Count | Category | Name | Owner | Diagnostic | Fixture | PHPT |",
        "| ---: | --- | --- | --- | --- | --- | --- |",
    ]
    for item in payload["items"]:
        lines.append(
            "| {count} | {category} | `{name}` | {owner} | `{diagnostic_id}` | `{recommended_fixture_path}` | {phpt} |".format(
                count=item["count"],
                category=item["category"],
                name=item["name"],
                owner=item["owner"],
                diagnostic_id=item["diagnostic_id"] or "",
                recommended_fixture_path=item["recommended_fixture_path"],
                phpt="yes" if item["phpt_coverage_exists"] else "no",
            )
        )
    lines.append("")
    return "\n".join(lines)


def self_test() -> None:
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        report = root / "runtime-semantics-diff-report.json"
        report.write_text(
            json.dumps(
                {
                    "results": [
                        {
                            "file": "fixtures/runtime_semantics/wp_autoload_stdlib/core_builtins/missing.php",
                            "status": "fail",
                            "message": "E_PHP_RUNTIME_UNDEFINED_FUNCTION: Call to undefined function missing_pack_b()",
                            "rust": {"stderr": ""},
                        }
                    ]
                }
            ),
            encoding="utf-8",
        )
        out = root / "out"
        items = build_heatmap([report], root / "missing-symbols.jsonl")
        assert len(items) == 1
        assert items[0].category == "missing_function"
        assert items[0].name == "missing_pack_b"
        write_outputs(items, [report], out)
        payload = json.loads((out / "builtin-heatmap.json").read_text(encoding="utf-8"))
        assert payload["summary"]["total"] == 1


if __name__ == "__main__":
    raise SystemExit(main())
