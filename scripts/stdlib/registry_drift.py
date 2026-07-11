#!/usr/bin/env python3
"""Compare runtime builtin registration with php_std descriptor metadata."""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
ALLOWLIST = ROOT / "scripts/stdlib/registry_drift_allowlist.jsonl"
RUNTIME_BUILTIN_ROOTS = (
    ROOT / "crates/php_runtime/src/builtins/modules",
    ROOT / "crates/php_extensions/src",
)
STD_EXTENSIONS = ROOT / "crates/php_std/src/extensions.rs"
GENERATED_ARGINFO = ROOT / "crates/php_std/src/generated/arginfo.rs"
REPORT_DIR = ROOT / "target/stdlib/registry-drift"
REPORT_JSON = REPORT_DIR / "report.json"
REPORT_MD = REPORT_DIR / "report.md"

RUNTIME_RE = re.compile(r'BuiltinEntry::new\(\s*"([^"]+)"', re.MULTILINE)
STD_RE = re.compile(r'FunctionDescriptor::php\(\s*"([^"]+)"', re.MULTILINE)
GENERATED_RE = re.compile(r'GeneratedFunctionMetadata\s*\{\s*name:\s*"([^"]+)"', re.MULTILINE)
VM_BUILTIN_NAMES = {
    "call_user_func",
    "call_user_func_array",
    "class_alias",
    "class_exists",
    "class_implements",
    "class_parents",
    "clone",
    "compact",
    "constant",
    "debug_backtrace",
    "debug_print_backtrace",
    "define",
    "defined",
    "die",
    "enum_exists",
    "exit",
    "extension_loaded",
    "forward_static_call",
    "func_get_arg",
    "func_get_args",
    "func_num_args",
    "function_exists",
    "get_called_class",
    "get_class",
    "get_class_methods",
    "get_class_vars",
    "get_declared_classes",
    "get_declared_interfaces",
    "get_declared_traits",
    "get_defined_constants",
    "get_defined_functions",
    "get_defined_vars",
    "get_error_handler",
    "get_exception_handler",
    "get_extension_funcs",
    "get_included_files",
    "get_loaded_extensions",
    "get_mangled_object_vars",
    "get_object_vars",
    "get_parent_class",
    "get_required_files",
    "interface_exists",
    "is_a",
    "is_callable",
    "is_subclass_of",
    "method_exists",
    "phpversion",
    "property_exists",
    "trait_exists",
    "zend_version",
}


class DriftError(Exception):
    pass


def read_names(path: Path, pattern: re.Pattern[str]) -> set[str]:
    text = path.read_text(encoding="utf-8")
    names = {match.group(1).lower() for match in pattern.finditer(text)}
    if not names:
        raise DriftError(f"no function names found in {path.relative_to(ROOT)}")
    return names


def read_runtime_names() -> set[str]:
    names: set[str] = set()
    for root in RUNTIME_BUILTIN_ROOTS:
        for path in sorted(root.rglob("*.rs")):
            if root.name == "src" and path == root / "lib.rs":
                continue
            names.update(read_names_from_text(path.read_text(encoding="utf-8"), RUNTIME_RE))
    if not names:
        roots = ", ".join(str(root.relative_to(ROOT)) for root in RUNTIME_BUILTIN_ROOTS)
        raise DriftError(f"no runtime builtin names found under {roots}")
    names.update(VM_BUILTIN_NAMES)
    return names


def read_names_from_text(text: str, pattern: re.Pattern[str]) -> set[str]:
    return {match.group(1).lower() for match in pattern.finditer(text)}


def load_allowlist() -> set[tuple[str, str]]:
    allowed: set[tuple[str, str]] = set()
    for index, line in enumerate(ALLOWLIST.read_text(encoding="utf-8").splitlines(), start=1):
        stripped = line.strip()
        if not stripped:
            continue
        try:
            entry = json.loads(stripped)
        except json.JSONDecodeError as error:
            raise DriftError(f"allowlist line {index} is not valid JSON: {error}") from error
        missing = {"kind", "name", "reason"} - set(entry)
        if missing:
            raise DriftError(
                f"allowlist line {index} missing fields: {', '.join(sorted(missing))}"
            )
        allowed.add((entry["kind"], entry["name"].lower()))
    return allowed


def write_report(runtime_names: set[str], std_names: set[str], drift: list[dict]) -> None:
    REPORT_DIR.mkdir(parents=True, exist_ok=True)
    payload = {
        "runtime_function_count": len(runtime_names),
        "std_metadata_function_count": len(std_names),
        "drift": drift,
    }
    REPORT_JSON.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    lines = [
        "# Standard Library Registry Drift",
        "",
        f"- Runtime builtin functions: {len(runtime_names)}",
        f"- php_std metadata functions: {len(std_names)}",
        "",
        "| Kind | Name | Allowed |",
        "| --- | --- | --- |",
    ]
    for row in drift:
        lines.append(f"| {row['kind']} | `{row['name']}` | {row['allowed']} |")
    REPORT_MD.write_text("\n".join(lines) + "\n", encoding="utf-8")


def main() -> int:
    try:
        runtime_names = read_runtime_names()
        generated_names = read_names(GENERATED_ARGINFO, GENERATED_RE)
        std_names = read_names(STD_EXTENSIONS, STD_RE)
        allowed = load_allowlist()
    except (OSError, DriftError) as error:
        print(f"[fail] stdlib registry drift: {error}", file=sys.stderr)
        return 1

    drift: list[dict] = []
    for name in sorted(runtime_names - generated_names):
        kind = "runtime_without_std_metadata"
        drift.append({"kind": kind, "name": name, "allowed": (kind, name) in allowed})
    for name in sorted(std_names - runtime_names):
        kind = "std_descriptor_without_runtime"
        drift.append({"kind": kind, "name": name, "allowed": (kind, name) in allowed})
    write_report(runtime_names, std_names, drift)

    violations = [row for row in drift if not row["allowed"]]
    if violations:
        print("[fail] stdlib registry drift: unallowlisted entries:", file=sys.stderr)
        for row in violations[:80]:
            print(f"  - {row['kind']}: {row['name']}", file=sys.stderr)
        if len(violations) > 80:
            print(f"  ... {len(violations) - 80} more", file=sys.stderr)
        print(f"Report: {REPORT_MD.relative_to(ROOT)}", file=sys.stderr)
        return 1
    print(f"[ok] stdlib registry drift report written to {REPORT_MD.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
