#!/usr/bin/env python3
"""Generate standard-library function/class/constant coverage reports."""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[2]
OUT_DIR = REPO_ROOT / "target/stdlib/function-coverage"
JSON_REPORT = OUT_DIR / "coverage.json"
DOC_REPORT = REPO_ROOT / "docs/stdlib/function-coverage.md"
RUST_DUMP = REPO_ROOT / "target/debug/dump_stdlib_registry"
EXTENSIONS = ["core", "standard", "json", "pcre", "date", "spl", "reflection", "tokenizer"]
KNOWN_GAP_BY_EXTENSION = {
    "standard": "STDLIB-GAP-FULL-PARITY",
    "json": "STDLIB-GAP-JSON-FLAGS-BYTE-PERFECT",
    "pcre": "STDLIB-GAP-PCRE-ADVANCED-FLAGS",
    "date": "STDLIB-GAP-DATE-TIMELIB-PARITY",
    "spl": "STDLIB-GAP-SPL-INTERFACE-METHOD-SURFACES",
    "reflection": "STDLIB-GAP-REFLECTION-INTERNAL-METHOD-SURFACE",
    "tokenizer": "STDLIB-GAP-TOKENIZER-ZEND-NUMERIC-IDS",
}


def main() -> int:
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    try:
        rust = load_rust_registry()
        reference, reference_status = load_reference_symbols()
        report = build_report(rust, reference, reference_status)
        JSON_REPORT.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        DOC_REPORT.write_text(render_markdown(report), encoding="utf-8")
    except Exception as error:  # noqa: BLE001 - top-level script boundary.
        print(f"standard-library function coverage error: {error}", file=sys.stderr)
        return 2
    print(f"[ok] wrote {relative(JSON_REPORT)}")
    print(f"[ok] wrote {relative(DOC_REPORT)}")
    return 0


def load_rust_registry() -> dict[str, dict[str, Any]]:
    if not RUST_DUMP.is_file():
        raise FileNotFoundError(f"missing {RUST_DUMP}; build php_std dump registry first")
    completed = subprocess.run(
        [str(RUST_DUMP)],
        cwd=REPO_ROOT,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    raw = json.loads(completed.stdout)
    extensions: dict[str, dict[str, Any]] = {}
    for extension in raw["extensions"]:
        name = normalize_extension(extension["name"])
        extensions[name] = {
            "functions": {
                symbol["name"].lower(): {
                    "name": symbol["name"],
                    "runtime_builtin": bool(symbol["runtime_builtin"]),
                }
                for symbol in extension["functions"]
            },
            "classes": {
                symbol["name"].lower(): {
                    "name": symbol["name"],
                    "kind": symbol["kind"],
                }
                for symbol in extension["classes"]
            },
            "constants": {
                symbol["name"]: {
                    "name": symbol["name"],
                    "has_value": symbol["value"] is not None,
                }
                for symbol in extension["constants"]
            },
        }
    return extensions


def load_reference_symbols() -> tuple[dict[str, dict[str, Any]], dict[str, str]]:
    reference_php = os.environ.get("REFERENCE_PHP", "")
    if not reference_php:
        return empty_reference(), {
            "status": "skipped",
            "reason": "REFERENCE_PHP is not set",
        }
    php = Path(reference_php)
    if not php.is_file():
        return empty_reference(), {
            "status": "skipped",
            "reason": f"REFERENCE_PHP does not point to a file: {reference_php}",
        }

    reference = empty_reference()
    scripts = {
        "functions": "list_reference_functions.php",
        "classes": "list_reference_classes.php",
        "constants": "list_reference_constants.php",
    }
    for kind, script in scripts.items():
        completed = subprocess.run(
            [str(php), str(REPO_ROOT / "scripts/stdlib" / script), *EXTENSIONS],
            cwd=REPO_ROOT,
            check=True,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        payload = json.loads(completed.stdout)
        merge_reference(reference, kind, payload)
    return reference, {
        "status": "available",
        "php": str(php),
    }


def empty_reference() -> dict[str, dict[str, Any]]:
    return {
        extension: {"functions": {}, "classes": {}, "constants": {}}
        for extension in EXTENSIONS
    }


def merge_reference(reference: dict[str, dict[str, Any]], kind: str, payload: dict[str, Any]) -> None:
    for extension, symbols in payload.items():
        extension = normalize_extension(extension)
        if extension not in reference:
            continue
        if kind == "classes":
            for symbol in symbols:
                reference[extension][kind][symbol["name"].lower()] = {
                    "name": symbol["name"],
                    "kind": symbol["kind"],
                }
        elif kind == "constants":
            for symbol in symbols:
                reference[extension][kind][symbol] = {"name": symbol}
        else:
            for symbol in symbols:
                reference[extension][kind][symbol.lower()] = {"name": symbol}


def build_report(
    rust: dict[str, dict[str, Any]],
    reference: dict[str, dict[str, Any]],
    reference_status: dict[str, str],
) -> dict[str, Any]:
    extension_reports = []
    totals = {status: 0 for status in ["implemented", "partial", "stub", "missing", "known_gap"]}
    for extension in EXTENSIONS:
        rust_extension = rust.get(extension, {"functions": {}, "classes": {}, "constants": {}})
        reference_extension = reference.get(extension, {"functions": {}, "classes": {}, "constants": {}})
        kinds = {}
        for kind in ["functions", "classes", "constants"]:
            rows = classify_symbols(
                extension=extension,
                kind=kind,
                rust_symbols=rust_extension[kind],
                reference_symbols=reference_extension[kind],
                reference_available=reference_status["status"] == "available",
            )
            summary = {status: 0 for status in totals}
            for row in rows:
                summary[row["status"]] += 1
                totals[row["status"]] += 1
            kinds[kind] = {"summary": summary, "symbols": rows}
        extension_reports.append({"name": extension, "kinds": kinds})
    return {
        "reference": reference_status,
        "extensions": extension_reports,
        "totals": totals,
        "status_definitions": {
            "implemented": "Reference and Rust registry both expose the symbol; functions also have a runtime builtin.",
            "partial": "Class-like symbol is registered but standard-library metadata/behavior is intentionally partial.",
            "stub": "Rust registry exposes the function but no runtime builtin is wired yet.",
            "missing": "Reference exposes the symbol and the Rust registry does not.",
            "known_gap": "Symbol comparison is skipped or covered by an explicit standard-library known gap.",
        },
    }


def classify_symbols(
    *,
    extension: str,
    kind: str,
    rust_symbols: dict[str, dict[str, Any]],
    reference_symbols: dict[str, dict[str, Any]],
    reference_available: bool,
) -> list[dict[str, str]]:
    keys = sorted(set(rust_symbols) | set(reference_symbols))
    rows: list[dict[str, str]] = []
    for key in keys:
        rust_symbol = rust_symbols.get(key)
        reference_symbol = reference_symbols.get(key)
        name = symbol_name(rust_symbol, reference_symbol)
        row = {
            "name": name,
            "status": "known_gap",
            "source": source_label(rust_symbol, reference_symbol),
            "reason": "",
        }
        if not reference_available and rust_symbol and kind == "functions":
            if rust_symbol.get("runtime_builtin"):
                row["status"] = "implemented"
                row["reason"] = "local Rust registry and runtime builtin are wired; reference comparison skipped"
            else:
                row["status"] = "stub"
                row["reason"] = "local Rust registry symbol has no runtime builtin; reference comparison skipped"
        elif not reference_available and rust_symbol and kind == "classes":
            row["status"] = "partial"
            row["reason"] = "local Rust registry class metadata is present; reference comparison skipped"
        elif not reference_available and rust_symbol:
            row["status"] = "implemented"
            row["reason"] = "local Rust registry constant is present; reference comparison skipped"
        elif not reference_available:
            row["status"] = "known_gap"
            row["reason"] = "REFERENCE_PHP unavailable; comparison skipped"
        elif rust_symbol and reference_symbol and kind == "functions":
            if rust_symbol.get("runtime_builtin"):
                row["status"] = "implemented"
                row["reason"] = "registered and runtime builtin is wired"
            else:
                row["status"] = "stub"
                row["reason"] = "registered in php_std but no php_runtime builtin"
        elif rust_symbol and reference_symbol and kind == "classes":
            row["status"] = "partial"
            row["reason"] = known_gap_reason(extension, "class metadata is intentionally partial")
        elif rust_symbol and reference_symbol:
            row["status"] = "implemented"
            row["reason"] = "registered in php_std"
        elif reference_symbol and not rust_symbol:
            gap = KNOWN_GAP_BY_EXTENSION.get(extension)
            row["status"] = "known_gap" if gap else "missing"
            row["reason"] = gap or "reference symbol is not registered"
        elif rust_symbol and not reference_symbol and kind == "functions":
            if rust_symbol.get("runtime_builtin"):
                row["status"] = "implemented"
                row["reason"] = "registered and runtime builtin is wired; reference mapping unavailable or language construct"
            else:
                row["status"] = "stub"
                row["reason"] = "Rust-only registry function has no runtime builtin"
        elif rust_symbol and not reference_symbol and kind == "classes":
            row["status"] = "partial"
            row["reason"] = known_gap_reason(
                extension,
                "local registry class metadata is present; reference extension mapping unavailable",
            )
        elif rust_symbol and not reference_symbol:
            row["status"] = "implemented"
            row["reason"] = "registered in php_std; reference mapping unavailable"
        rows.append(row)
    return rows


def render_markdown(report: dict[str, Any]) -> str:
    reference = report["reference"]
    lines = [
        "# Standard Library Function Coverage",
        "",
        "Generated by `scripts/stdlib/function_coverage.py` via `stdlib-coverage`.",
        "",
        f"Reference status: `{reference['status']}`"
        + (f" ({reference['reason']})" if "reason" in reference else ""),
        "",
        "| Status | Count |",
        "| --- | ---: |",
    ]
    for status, count in report["totals"].items():
        lines.append(f"| {status} | {count} |")
    lines.extend(["", "## Extension Summary", "", "| Extension | Kind | Implemented | Partial | Stub | Missing | Known gap |", "| --- | --- | ---: | ---: | ---: | ---: | ---: |"])
    for extension in report["extensions"]:
        for kind, payload in extension["kinds"].items():
            summary = payload["summary"]
            lines.append(
                f"| {extension['name']} | {kind} | {summary['implemented']} | {summary['partial']} | "
                f"{summary['stub']} | {summary['missing']} | {summary['known_gap']} |"
            )
    lines.extend(["", "## Notable Gaps", "", "| Extension | Kind | Symbol | Status | Reason |", "| --- | --- | --- | --- | --- |"])
    notable = 0
    for extension in report["extensions"]:
        for kind, payload in extension["kinds"].items():
            for symbol in payload["symbols"]:
                if symbol["status"] in {"missing", "stub", "known_gap"}:
                    lines.append(
                        f"| {extension['name']} | {kind} | `{symbol['name']}` | "
                        f"{symbol['status']} | {symbol['reason']} |"
                    )
                    notable += 1
                    if notable >= 80:
                        lines.append("| ... | ... | ... | ... | truncated; see JSON report |")
                        return "\n".join(lines) + "\n"
    if notable == 0:
        lines.append("| all | all | n/a | implemented | No notable gaps in selected extension set |")
    return "\n".join(lines) + "\n"


def symbol_name(rust_symbol: dict[str, Any] | None, reference_symbol: dict[str, Any] | None) -> str:
    if reference_symbol:
        return str(reference_symbol["name"])
    if rust_symbol:
        return str(rust_symbol["name"])
    return ""


def source_label(rust_symbol: dict[str, Any] | None, reference_symbol: dict[str, Any] | None) -> str:
    if rust_symbol and reference_symbol:
        return "reference+rust"
    if rust_symbol:
        return "rust"
    return "reference"


def known_gap_reason(extension: str, detail: str) -> str:
    gap = KNOWN_GAP_BY_EXTENSION.get(extension)
    if gap:
        return f"{detail}; tracked by {gap}"
    return detail


def normalize_extension(extension: str) -> str:
    extension = extension.lower()
    return {"core": "core", "spl": "spl"}.get(extension, extension)


def relative(path: Path) -> str:
    return path.relative_to(REPO_ROOT).as_posix()


if __name__ == "__main__":
    raise SystemExit(main())
