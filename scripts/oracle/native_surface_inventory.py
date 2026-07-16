#!/usr/bin/env python3
"""Build the source-derived native PHP surface support inventory.

The inventory deliberately separates registration from demonstrated support.
Only differential probe or PHPT evidence may produce a supported class.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from collections import Counter, defaultdict
from pathlib import Path
from typing import Any, Iterable


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_API = ROOT / "target/oracle/api/php-source-api-symbols.jsonl"
DEFAULT_REGISTRY = ROOT / "target/debug/dump_stdlib_registry"
DEFAULT_PROBES = ROOT / "tests/oracle/manifests/generated-probes.jsonl"
DEFAULT_PHPT = ROOT / "tests/phpt/manifests/phpt-corpus.jsonl"
DEFAULT_PHPT_SYMBOLS = ROOT / "tests/phpt/manifests/php-src-symbols.jsonl"
DEFAULT_OUT = ROOT / "target/native-surface"
PHP_VERSION = "8.5.7"

SUPPORT_CLASSES = {
    "native_direct",
    "native_typed_runtime",
    "native_generic_builtin",
    "environmentally_unavailable_reference_identical",
    "registered_unprobed",
    "semantic_mismatch",
    "undefined_in_target",
    "native_compile_gap",
    "crash_or_timeout",
}
SUPPORTED_CLASSES = {
    "native_direct",
    "native_typed_runtime",
    "native_generic_builtin",
    "environmentally_unavailable_reference_identical",
}
API_CLASS_KINDS = {"class", "interface", "trait", "enum"}
CONSTANT_KINDS = {"constant", "class_constant"}
EXTERNAL_EXTENSIONS = {
    "curl": "network",
    "ftp": "network",
    "imap": "network",
    "ldap": "network",
    "mysqli": "database",
    "openssl": "crypto certificates",
    "pcntl": "process/IPC",
    "pdo": "database",
    "pdo_mysql": "database",
    "pdo_pgsql": "database",
    "pgsql": "database",
    "readline": "process/IPC",
    "sockets": "network",
    "ssh2": "network",
    "sysvmsg": "process/IPC",
    "sysvsem": "process/IPC",
    "sysvshm": "process/IPC",
}
EXTENSION_PRIORITY = {
    "core": 10,
    "standard": 10,
    "spl": 9,
    "reflection": 9,
    "date": 8,
    "json": 8,
    "pcre": 8,
    "mbstring": 8,
    "filesystem": 8,
    "streams": 8,
    "pdo": 7,
    "mysqli": 7,
    "curl": 7,
    "openssl": 7,
    "dom": 6,
    "simplexml": 6,
    "xml": 6,
    "intl": 6,
}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--api", type=Path, default=DEFAULT_API)
    parser.add_argument("--registry", type=Path, default=DEFAULT_REGISTRY)
    parser.add_argument("--probe-manifest", type=Path, default=DEFAULT_PROBES)
    parser.add_argument("--phpt-manifest", type=Path, default=DEFAULT_PHPT)
    parser.add_argument("--phpt-symbols", type=Path, default=DEFAULT_PHPT_SYMBOLS)
    parser.add_argument("--out", type=Path, default=DEFAULT_OUT)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--self-test-only", action="store_true")
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()

    try:
        if args.self_test or args.self_test_only:
            run_self_tests()
        if args.self_test_only:
            return 0
        rows = build_inventory(args)
        validate_inventory(rows, load_registry(args.registry), strict=args.check)
        write_inventory(args.out, rows)
    except Exception as error:  # noqa: BLE001 - command boundary.
        print(f"native surface inventory error: {error}", file=sys.stderr)
        return 1

    for name in [
        "functions.jsonl",
        "methods.jsonl",
        "classes.jsonl",
        "constants.jsonl",
        "language-operations.json",
        "summary.md",
    ]:
        print(f"[ok] wrote {relative(args.out / name)}")
    return 0


def build_inventory(args: argparse.Namespace) -> dict[str, list[dict[str, Any]]]:
    api_rows = load_jsonl(args.api, required=True)
    registry = load_registry(args.registry)
    probes = load_probe_evidence(args.probe_manifest)
    phpt = load_phpt_evidence(args.phpt_manifest, args.phpt_symbols)
    direct_functions = native_direct_function_names()
    capabilities = extension_capabilities()
    occurrences = fixture_occurrences()

    rows_by_key = {api_key(row): row for row in api_rows}
    for function in registry["functions"].values():
        key = ("function", "", function["name"].lower())
        rows_by_key.setdefault(
            key,
            {
                "kind": "function",
                "name": function["name"],
                "class": None,
                "extension": function["extension"],
                "source": "Rust extension registry",
                "signature": None,
                "status": "extractor_gap",
                "rust_registry": {},
            },
        )

    functions = []
    methods = []
    classes = []
    constants = []
    for api in sorted(rows_by_key.values(), key=api_sort_key):
        kind = api["kind"]
        if kind not in {"function", "method", *API_CLASS_KINDS, *CONSTANT_KINDS}:
            continue
        row = surface_row(
            api,
            registry=registry,
            probes=probes,
            phpt=phpt,
            direct_functions=direct_functions,
            capabilities=capabilities,
            occurrences=occurrences,
        )
        if kind == "function":
            functions.append(row)
        elif kind == "method":
            methods.append(row)
        elif kind in API_CLASS_KINDS:
            classes.append(row)
        else:
            constants.append(row)

    return {
        "functions": functions,
        "methods": methods,
        "classes": classes,
        "constants": constants,
        "language_operations": language_operations(probes, phpt),
    }


def surface_row(
    api: dict[str, Any],
    *,
    registry: dict[str, Any],
    probes: dict[tuple[str, str, str], list[dict[str, Any]]],
    phpt: dict[str, list[str]],
    direct_functions: set[str],
    capabilities: dict[str, list[str]],
    occurrences: Counter[str],
) -> dict[str, Any]:
    kind = api["kind"]
    name = str(api["name"])
    class_name = api.get("class")
    extension = str(api.get("extension") or "core").lower()
    normalized = name.lower()
    registry_row: dict[str, Any] | None
    if kind == "function":
        registry_row = registry["functions"].get(normalized)
        registry_available = registry_row is not None
        runtime_builtin = bool(registry_row and registry_row.get("runtime_builtin"))
    elif kind in API_CLASS_KINDS:
        registry_row = registry["classes"].get(normalized)
        registry_available = registry_row is not None
        runtime_builtin = registry_available
    elif kind == "method":
        registry_row = registry["classes"].get(str(class_name).lower())
        registry_available = registry_row is not None
        runtime_builtin = native_method_route(str(class_name), name)
    else:
        registry_row = registry["constants"].get(
            f"{class_name}::{name}" if class_name else name
        ) or registry["constants"].get(name)
        registry_available = registry_row is not None
        runtime_builtin = registry_available

    if kind == "function" and runtime_builtin:
        dispatch_route = (
            "native_direct" if normalized in direct_functions else "native_generic_builtin"
        )
    elif kind == "method" and runtime_builtin:
        dispatch_route = "native_class_method_dispatch"
    elif runtime_builtin:
        dispatch_route = "native_typed_runtime"
    else:
        dispatch_route = "unavailable"

    evidence_key = (kind, str(class_name or "").lower(), normalized)
    evidence = probes.get(evidence_key, [])
    # Older manifests identify methods as Class::method or omit their owner.
    if kind == "method" and not evidence:
        evidence = probes.get((kind, "", f"{str(class_name).lower()}::{normalized}"), [])
    probe_status, reference_result, target_result, first_difference = summarize_probes(evidence)
    support_class = classify_support(
        registry_available=registry_available,
        runtime_builtin=runtime_builtin,
        dispatch_route=dispatch_route,
        probe_status=probe_status,
        reference_result=reference_result,
        target_result=target_result,
        first_difference=first_difference,
    )
    phpt_paths = sorted(set(phpt.get(normalized, [])))
    occurrence = occurrences[normalized] + occurrences[str(class_name or "").lower()]
    priority = rank_gap(extension, len(phpt_paths), occurrence, support_class)
    signature = api.get("signature")
    return {
        "name": name,
        "kind": kind,
        "class": class_name,
        "extension_module": extension,
        "php_version": api.get("php_version", PHP_VERSION),
        "source": api.get("source"),
        "arginfo_availability": "available" if signature else "unavailable",
        "signature": signature,
        "registry_availability": registry_available,
        "runtime_builtin": runtime_builtin,
        "native_dispatch_route": dispatch_route,
        "runtime_state_capabilities_required": capabilities.get(extension, []),
        "environmental_class": EXTERNAL_EXTENSIONS.get(extension),
        "probe_status": probe_status,
        "probe_evidence": evidence,
        "phpt_evidence": {"count": len(phpt_paths), "paths": phpt_paths[:50]},
        "reference_result": reference_result,
        "target_result": target_result,
        "first_difference": first_difference,
        "support_class": support_class,
        "gap_rank": priority,
        "ranking_inputs": {
            "wordpress_composer_occurrence": occurrence,
            "phpt_count": len(phpt_paths),
            "api_popularity_class": EXTENSION_PRIORITY.get(extension, 2),
            "extension_importance": EXTENSION_PRIORITY.get(extension, 2),
            "dependent_failures": sum(item.get("dependent_failures", 0) for item in evidence),
        },
    }


def language_operations(
    probes: dict[tuple[str, str, str], list[dict[str, Any]]],
    phpt: dict[str, list[str]],
) -> list[dict[str, Any]]:
    coverage_path = ROOT / "crates/php_jit/src/region_ir/coverage.rs"
    region_path = ROOT / "crates/php_jit/src/region_ir/executable.rs"
    clif_paths = sorted((ROOT / "crates/php_jit/src/cranelift_lowering").rglob("*.rs"))
    clif_paths.append(ROOT / "crates/php_jit/src/cranelift_lowering.rs")
    coverage = coverage_path.read_text(encoding="utf-8")
    region = region_path.read_text(encoding="utf-8")
    clif = "\n".join(path.read_text(encoding="utf-8") for path in clif_paths)
    region_variants = enum_variants(region, "RegionInstructionKind")
    terminator_variants = enum_variants(region, "RegionTerminator")
    entries: list[dict[str, Any]] = []
    pattern = re.compile(
        r"(?:InstructionKind|TerminatorKind)::(?P<variant>[A-Za-z0-9_]+)"
        r"(?:\s*\{[^\n]*\})?\s*=>\s*\(\"(?P<label>[A-Za-z0-9_]+)\",\s*"
        r"BaselineLoweringClass::(?P<class>[A-Za-z0-9_]+)(?:\([^)]*\))?",
    )
    for match in pattern.finditer(coverage):
        variant = match.group("variant")
        label = match.group("label")
        source_kind = "terminator" if match.group(0).startswith("TerminatorKind") else "instruction"
        conditional_gap = source_variant_has_gap(region, variant)
        region_available = variant in region_variants or source_kind == "terminator" and variant in terminator_variants
        clif_available = any(
            f"RegionInstructionKind::{candidate}" in clif
            or f"RegionTerminator::{candidate}" in clif
            for candidate in [variant, semantic_region_variant(match.group("class"))]
        )
        key = ("language_operation", "", label.lower())
        evidence = probes.get(key, [])
        probe_status, reference_result, target_result, first_difference = summarize_probes(evidence)
        if conditional_gap:
            support_class = "native_compile_gap"
        elif probe_status == "pass":
            support_class = "native_typed_runtime"
        else:
            support_class = "registered_unprobed"
        phpt_paths = sorted(set(phpt.get(label.lower(), [])))
        entries.append(
            {
                "name": label,
                "kind": source_kind,
                "extension_module": "language",
                "php_version": PHP_VERSION,
                "arginfo_availability": "not_applicable",
                "registry_availability": True,
                "native_dispatch_route": match.group("class"),
                "runtime_state_capabilities_required": [],
                "baseline_manifest": True,
                "region_availability": region_available,
                "cranelift_lowering_availability": clif_available,
                "conditional_missing_lowering": conditional_gap,
                "probe_status": probe_status,
                "probe_evidence": evidence,
                "phpt_evidence": {"count": len(phpt_paths), "paths": phpt_paths[:50]},
                "reference_result": reference_result,
                "target_result": target_result,
                "first_difference": first_difference,
                "support_class": support_class,
            }
        )
    entries.sort(key=lambda row: (row["kind"], row["name"].lower()))
    return entries


def load_registry(path: Path) -> dict[str, Any]:
    if not path.is_file():
        raise FileNotFoundError(f"missing Rust registry dump: {relative(path)}")
    completed = subprocess.run(
        [str(path)], cwd=ROOT, check=True, text=True, capture_output=True
    )
    payload = json.loads(completed.stdout)
    functions: dict[str, dict[str, Any]] = {}
    classes: dict[str, dict[str, Any]] = {}
    constants: dict[str, dict[str, Any]] = {}
    for extension in payload.get("extensions", []):
        module = str(extension["name"]).lower()
        for function in extension.get("functions", []):
            row = {**function, "extension": module}
            key = str(row["name"]).lower()
            if key in functions:
                raise ValueError(f"duplicate registered function: {row['name']}")
            functions[key] = row
        for class_row in extension.get("classes", []):
            row = {**class_row, "extension": module}
            key = str(row["name"]).lower()
            classes.setdefault(key, row)
        for constant in extension.get("constants", []):
            row = {**constant, "extension": module}
            constants.setdefault(str(row["name"]), row)
    return {"functions": functions, "classes": classes, "constants": constants}


def load_probe_evidence(path: Path) -> dict[tuple[str, str, str], list[dict[str, Any]]]:
    manifests = load_jsonl(path, required=False)
    report_results: dict[str, dict[str, Any]] = {}
    for report_path in sorted((ROOT / "target/oracle/probes").glob("**/*.json")):
        try:
            payload = json.loads(report_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            continue
        for result in payload.get("results", []):
            fixture = str(result.get("file") or result.get("path") or "")
            report_results[fixture] = result
            report_results[Path(fixture).name] = result
    evidence: dict[tuple[str, str, str], list[dict[str, Any]]] = defaultdict(list)
    for probe in manifests:
        kind = str(probe.get("kind") or "").lower()
        symbol = str(probe.get("symbol") or "")
        owner = str(probe.get("class") or "")
        if kind == "method" and "::" in symbol and not owner:
            owner, symbol = symbol.split("::", 1)
        result = report_results.get(str(probe.get("path"))) or report_results.get(
            Path(str(probe.get("path"))).name
        )
        item = {
            "id": probe.get("id"),
            "path": probe.get("path"),
            "selection": probe.get("selection"),
            "probe_case": probe.get("probe_case"),
            "support_evidence": bool(probe.get("support_evidence", True)),
            "environmental_class": probe.get("environmental_class"),
            "required_reference_extension": probe.get("required_reference_extension"),
            "generated": True,
            "executed": result is not None,
        }
        if result:
            item.update(
                {
                    "status": result.get("status"),
                    "reference": result.get("reference"),
                    "target": result.get("rust") or result.get("target"),
                    "first_difference": result.get("message")
                    or result.get("failure_category"),
                    "dependent_failures": 1 if result.get("status") != "pass" else 0,
                }
            )
        evidence[(kind, owner.lower(), symbol.lower())].append(item)
    return evidence


def load_phpt_evidence(manifest: Path, symbols: Path) -> dict[str, list[str]]:
    corpus = {str(row.get("path")): row for row in load_jsonl(manifest, required=False)}
    evidence: dict[str, list[str]] = defaultdict(list)
    for row in load_jsonl(symbols, required=False):
        name = str(row.get("php_name") or "").strip().lower()
        path = str(row.get("path") or "")
        if not name or path not in corpus:
            continue
        evidence[name].append(path)
    return evidence


def native_direct_function_names() -> set[str]:
    path = ROOT / "crates/php_vm/src/vm/jit_abi/native_builtins.rs"
    source = path.read_text(encoding="utf-8")
    names: set[str] = set()
    for function in ["execute_native_internal_builtin", "execute_native_builtin"]:
        body = rust_function_body(source, function)
        for value in re.findall(r'"([A-Za-z_\\][A-Za-z0-9_\\]*)"', body):
            names.add(value.lower())
    return names


def native_method_route(class_name: str, method: str) -> bool:
    sources = [
        ROOT / "crates/php_vm/src/vm/jit_abi/internal_classes.rs",
        ROOT / "crates/php_vm/src/vm/jit_abi/object_support.rs",
        ROOT / "crates/php_vm/src/vm/jit_abi/call_dispatch.rs",
    ]
    text = "\n".join(path.read_text(encoding="utf-8") for path in sources if path.is_file()).lower()
    return class_name.lower() in text and method.lower() in text


def extension_capabilities() -> dict[str, list[str]]:
    result: dict[str, list[str]] = {}
    root = ROOT / "fixtures/stdlib/extensions"
    for path in sorted(root.glob("*.json")):
        try:
            row = json.loads(path.read_text(encoding="utf-8"))
        except json.JSONDecodeError as error:
            raise ValueError(f"invalid extension descriptor {relative(path)}: {error}") from error
        result[path.stem.lower()] = sorted(str(item) for item in row.get("capabilities", []))
    return result


def summarize_probes(
    evidence: list[dict[str, Any]],
) -> tuple[str, Any, Any, str | None]:
    if not evidence:
        return "unprobed", None, None, None
    support_evidence = [item for item in evidence if item.get("support_evidence")]
    if not support_evidence:
        return "classified_non_behavioral", None, None, None
    executed = [item for item in support_evidence if item.get("executed")]
    if not executed:
        return "generated_unexecuted", None, None, None
    if all(item.get("status") == "skip" for item in executed):
        return "reference_unavailable", None, None, None
    failing = next((item for item in executed if item.get("status") != "pass"), None)
    chosen = failing or executed[-1]
    return (
        "pass" if failing is None else str(chosen.get("status") or "mismatch"),
        chosen.get("reference"),
        chosen.get("target"),
        chosen.get("first_difference"),
    )


def classify_support(
    *,
    registry_available: bool,
    runtime_builtin: bool,
    dispatch_route: str,
    probe_status: str,
    reference_result: Any,
    target_result: Any,
    first_difference: str | None,
) -> str:
    if probe_status == "pass":
        if dispatch_route == "native_direct":
            return "native_direct"
        if dispatch_route == "native_generic_builtin":
            return "native_generic_builtin"
        return "native_typed_runtime"
    combined = json.dumps([target_result, first_difference]).lower()
    if "timeout" in combined or "crash" in combined or "signal" in combined:
        return "crash_or_timeout"
    if "e_native_unsupported_lowering" in combined or "missinglowering" in combined:
        return "native_compile_gap"
    if probe_status not in {
        "unprobed",
        "generated_unexecuted",
        "classified_non_behavioral",
        "reference_unavailable",
    }:
        reference_status = (reference_result or {}).get("status") if isinstance(reference_result, dict) else None
        target_status = (target_result or {}).get("status") if isinstance(target_result, dict) else None
        if reference_status == target_status == "skipped":
            return "environmentally_unavailable_reference_identical"
        return "semantic_mismatch"
    if registry_available and runtime_builtin:
        return "registered_unprobed"
    return "undefined_in_target"


def fixture_occurrences() -> Counter[str]:
    occurrences: Counter[str] = Counter()
    for root in [ROOT / "fixtures/runtime_semantics/wordpress_blockers", ROOT / "fixtures/runtime_semantics/real_world"]:
        if not root.is_dir():
            continue
        for path in root.rglob("*.php"):
            text = path.read_text(encoding="utf-8", errors="replace").lower()
            occurrences.update(re.findall(r"[a-z_\\][a-z0-9_\\]*", text))
    return occurrences


def rank_gap(extension: str, phpt_count: int, occurrence: int, support_class: str) -> int:
    if support_class in SUPPORTED_CLASSES:
        return 0
    return occurrence * 100 + phpt_count * 10 + EXTENSION_PRIORITY.get(extension, 2)


def enum_variants(source: str, name: str) -> set[str]:
    marker = re.search(rf"pub enum {re.escape(name)}\s*\{{", source)
    if marker is None:
        return set()
    body, _ = balanced_body(source, source.index("{", marker.start()))
    return set(re.findall(r"^\s{4}([A-Z][A-Za-z0-9_]*)\b", body, re.MULTILINE))


def source_variant_has_gap(region_source: str, variant: str) -> bool:
    starts = [match.start() for match in re.finditer(rf"InstructionKind::{re.escape(variant)}\b", region_source)]
    for start in starts:
        next_arm = re.search(r"\n\s{20}InstructionKind::[A-Za-z0-9_]+", region_source[start + 1 :])
        end = start + 1 + next_arm.start() if next_arm else min(len(region_source), start + 5000)
        if "MissingLowering" in region_source[start:end]:
            return True
    return False


def semantic_region_variant(lowering_class: str) -> str:
    return {
        "NativeControlFlow": "NativeCall",
        "NativeStateMachine": "NativeControl",
        "TypedRuntimeHelper": "Binary",
        "CompileTimeFatal": "CompileTimeFatal",
        "DirectClif": "Move",
    }.get(lowering_class, lowering_class)


def rust_function_body(source: str, name: str) -> str:
    match = re.search(rf"\bfn\s+{re.escape(name)}\s*\(", source)
    if match is None:
        return ""
    brace = source.find("{", match.start())
    if brace < 0:
        return ""
    body, _ = balanced_body(source, brace)
    return body


def balanced_body(source: str, opening: int) -> tuple[str, int]:
    depth = 0
    for index in range(opening, len(source)):
        char = source[index]
        if char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                return source[opening + 1 : index], index
    raise ValueError("unbalanced Rust source while extracting inventory")


def write_inventory(out: Path, rows: dict[str, list[dict[str, Any]]]) -> None:
    out.mkdir(parents=True, exist_ok=True)
    for key in ["functions", "methods", "classes", "constants"]:
        write_jsonl(out / f"{key}.jsonl", rows[key])
    (out / "language-operations.json").write_text(
        json.dumps(rows["language_operations"], indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    (out / "summary.md").write_text(render_summary(rows), encoding="utf-8")


def render_summary(rows: dict[str, list[dict[str, Any]]]) -> str:
    all_rows = [row for values in rows.values() for row in values]
    counts = Counter(row["support_class"] for row in all_rows)
    supported = sum(counts[name] for name in SUPPORTED_CLASSES)
    gaps = sorted(
        (row for row in all_rows if row["support_class"] not in SUPPORTED_CLASSES),
        key=lambda row: (-int(row.get("gap_rank", 0)), row["kind"], row["name"].lower()),
    )
    lines = [
        "# Native PHP 8.5 Surface Inventory",
        "",
        "Generated from php-src arginfo, Rust registries, Region IR/Cranelift source, oracle probes, and PHPT manifests.",
        "",
        f"- PHP target: `{PHP_VERSION}`",
        f"- Total inventoried entries: `{len(all_rows)}`",
        f"- Reference-compatible and probed: `{supported}`",
        f"- Remaining/unproven: `{len(all_rows) - supported}`",
        "",
        "## Surface Counts",
        "",
        "| Surface | Count |",
        "| --- | ---: |",
    ]
    for key, values in rows.items():
        lines.append(f"| `{key.replace('_', '-')}` | {len(values)} |")
    lines.extend(["", "## Support Classes", "", "| Class | Count |", "| --- | ---: |"])
    for support_class in sorted(SUPPORT_CLASSES):
        lines.append(f"| `{support_class}` | {counts[support_class]} |")
    lines.extend(["", "## Highest-impact Unproven/Gapped Entries", "", "| Rank | Kind | Symbol | Extension | Status |", "| ---: | --- | --- | --- | --- |"])
    for row in gaps[:100]:
        symbol = f"{row.get('class')}::{row['name']}" if row.get("class") else row["name"]
        lines.append(
            f"| {row.get('gap_rank', 0)} | `{row['kind']}` | `{symbol}` | `{row['extension_module']}` | `{row['support_class']}` |"
        )
    lines.extend(
        [
            "",
            "Supported counts intentionally exclude registered-but-unprobed entries.",
            "",
        ]
    )
    return "\n".join(lines)


def validate_inventory(
    rows: dict[str, list[dict[str, Any]]], registry: dict[str, Any], *, strict: bool
) -> None:
    for group, values in rows.items():
        keys: set[tuple[str, str, str]] = set()
        for row in values:
            support_class = row.get("support_class")
            if support_class not in SUPPORT_CLASSES:
                raise ValueError(f"{group} has invalid support class {support_class!r}")
            key = (str(row["kind"]), str(row.get("class") or "").lower(), str(row["name"]).lower())
            if key in keys:
                raise ValueError(f"duplicate {group} entry: {key}")
            keys.add(key)
    inventoried = {row["name"].lower() for row in rows["functions"] if row["registry_availability"]}
    missing = sorted(set(registry["functions"]) - inventoried)
    extra = sorted(inventoried - set(registry["functions"]))
    if missing or extra:
        raise ValueError(f"registered function coverage mismatch: missing={missing[:10]}, extra={extra[:10]}")
    if strict and not rows["language_operations"]:
        raise ValueError("language operation inventory is empty")


def load_jsonl(path: Path, *, required: bool) -> list[dict[str, Any]]:
    if not path.is_file():
        if required:
            raise FileNotFoundError(f"missing input: {relative(path)}")
        return []
    rows = []
    for line_number, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not raw.strip():
            continue
        try:
            row = json.loads(raw)
        except json.JSONDecodeError as error:
            raise ValueError(f"{relative(path)}:{line_number}: invalid JSONL: {error}") from error
        if not isinstance(row, dict):
            raise ValueError(f"{relative(path)}:{line_number}: expected JSON object")
        rows.append(row)
    return rows


def write_jsonl(path: Path, rows: Iterable[dict[str, Any]]) -> None:
    with path.open("w", encoding="utf-8") as handle:
        for row in rows:
            handle.write(json.dumps(row, sort_keys=True, separators=(",", ":")) + "\n")


def api_key(row: dict[str, Any]) -> tuple[str, str, str]:
    return (
        str(row["kind"]),
        str(row.get("class") or "").lower(),
        str(row["name"]).lower(),
    )


def api_sort_key(row: dict[str, Any]) -> tuple[str, str, str]:
    return api_key(row)


def relative(path: Path) -> str:
    try:
        return path.relative_to(ROOT).as_posix()
    except ValueError:
        return str(path)


def run_self_tests() -> None:
    body = "fn sample() { if true { call(); } } trailing"
    extracted = rust_function_body(body, "sample")
    if "call()" not in extracted or "trailing" in extracted:
        raise AssertionError("balanced Rust function extraction failed")
    if classify_support(
        registry_available=True,
        runtime_builtin=True,
        dispatch_route="native_direct",
        probe_status="pass",
        reference_result={},
        target_result={},
        first_difference=None,
    ) != "native_direct":
        raise AssertionError("supported direct classification failed")
    if classify_support(
        registry_available=True,
        runtime_builtin=True,
        dispatch_route="native_generic_builtin",
        probe_status="generated_unexecuted",
        reference_result=None,
        target_result=None,
        first_difference=None,
    ) != "registered_unprobed":
        raise AssertionError("unprobed registry classification failed")


if __name__ == "__main__":
    raise SystemExit(main())
