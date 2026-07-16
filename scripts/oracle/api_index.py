#!/usr/bin/env python3
"""Build a deterministic php-src/reference-PHP API oracle index."""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_OUT = REPO_ROOT / "target/oracle/api/php-source-api-symbols.jsonl"
DEFAULT_SUMMARY = REPO_ROOT / "target/oracle/api/php-source-api-summary.md"
DEFAULT_REGISTRY = REPO_ROOT / "target/debug/dump_stdlib_registry"
PHP_VERSION = "8.5.7"
KINDS = {
    "function",
    "class",
    "interface",
    "trait",
    "enum",
    "method",
    "class_constant",
    "constant",
    "property",
    "ini",
    "extension",
    "alias",
}
STATUSES = {
    "matched",
    "missing_in_rust",
    "rust_stub",
    "metadata_mismatch",
    "reference_only_known_gap",
    "reference_unavailable",
    "extractor_gap",
}


@dataclass(frozen=True)
class ReferencePayload:
    status: str
    reason: str | None
    php: str | None
    php_version: str
    rows: list[dict[str, Any]]
    gaps: list[str]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--php-src", type=Path)
    parser.add_argument("--reference-php", type=Path)
    parser.add_argument("--registry", type=Path, default=DEFAULT_REGISTRY)
    parser.add_argument("--out", type=Path, default=DEFAULT_OUT)
    parser.add_argument("--summary", type=Path, default=DEFAULT_SUMMARY)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--self-test-only", action="store_true")
    parser.add_argument("--summary-only", action="store_true")
    args = parser.parse_args()

    if args.summary_only:
        if not args.summary.is_file():
            raise SystemExit(f"missing summary; run oracle-api-index first: {relative(args.summary)}")
        print(args.summary.read_text(encoding="utf-8"), end="")
        return 0

    try:
        if args.self_test or args.self_test_only:
            run_self_tests()
        if args.self_test_only:
            return 0

        php_src = resolve_php_src(args.php_src)
        reference_php = resolve_reference_php(args.reference_php, php_src)
        rust = load_rust_registry(args.registry)
        static_rows, extractor_gaps = load_static_rows(php_src)
        reference = load_reference_rows(reference_php, args.out.parent)
        rows = merge_rows(static_rows, reference.rows)
        rows.extend(rust_only_rows(rows, rust, reference.php_version))
        rows.extend(extractor_gap_rows(extractor_gaps, reference.php_version))
        rows = classify_rows(rows, rust, reference)
        validate_rows(rows)
        write_jsonl(args.out, rows)
        args.summary.write_text(
            render_summary(
                rows=rows,
                php_src=php_src,
                reference=reference,
                rust_status=rust["status"],
                extractor_gaps=extractor_gaps,
                out=args.out,
            ),
            encoding="utf-8",
        )
    except Exception as error:  # noqa: BLE001 - script boundary.
        print(f"oracle API index error: {error}", file=sys.stderr)
        return 1

    print(f"[ok] wrote {relative(args.out)}")
    print(f"[ok] wrote {relative(args.summary)}")
    return 0


def import_arginfo() -> Any:
    path = REPO_ROOT / "scripts/stdlib/generate_arginfo.py"
    spec = importlib.util.spec_from_file_location("phrust_generate_arginfo", path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot import {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def resolve_php_src(argument: Path | None) -> Path:
    candidates = []
    if argument is not None:
        candidates.append(argument)
    if os.environ.get("PHP_SRC_DIR"):
        candidates.append(Path(os.environ["PHP_SRC_DIR"]))
    candidates.extend(
        [
            REPO_ROOT / "third_party/php-src",
            REPO_ROOT / "third_party/php-src-8.5.7",
            Path("/Volumes/CrucialMusic/src/phrust/third_party/php-src"),
        ]
    )
    for candidate in candidates:
        if candidate.is_dir():
            return candidate.resolve()
    tried = ", ".join(str(candidate) for candidate in candidates)
    raise FileNotFoundError(f"php-src checkout not found; tried: {tried}")


def resolve_reference_php(argument: Path | None, php_src: Path) -> Path | None:
    explicit = argument is not None or bool(os.environ.get("REFERENCE_PHP"))
    candidate = argument
    if candidate is None and os.environ.get("REFERENCE_PHP"):
        candidate = Path(os.environ["REFERENCE_PHP"])
    if candidate is None:
        default = php_src / "sapi/cli/php"
        candidate = default if default.is_file() else None
    if candidate is None:
        return None
    if candidate.is_file():
        return candidate.resolve()
    if explicit:
        raise FileNotFoundError(f"REFERENCE_PHP does not point to a file: {candidate}")
    return None


def load_rust_registry(registry: Path) -> dict[str, Any]:
    if not registry.is_file():
        return {
            "status": {"status": "skipped", "reason": f"missing {relative(registry)}"},
            "functions": {},
            "classes": {},
            "constants": {},
        }
    completed = subprocess.run(
        [str(registry)],
        cwd=REPO_ROOT,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    raw = json.loads(completed.stdout)
    functions: dict[str, dict[str, Any]] = {}
    classes: dict[str, dict[str, Any]] = {}
    constants: dict[str, dict[str, Any]] = {}
    for extension in raw.get("extensions", []):
        require_fields(extension, {"name", "enabled_by_default", "functions", "classes", "constants"}, "extension")
        extension_name = normalize_extension(extension["name"])
        for function in extension["functions"]:
            require_fields(
                function,
                {
                    "name",
                    "runtime_builtin",
                    "arginfo_source",
                    "required_parameters",
                    "total_parameters",
                    "variadic",
                },
                "registry function",
            )
            functions[function["name"].lower()] = {
                **function,
                "extension": extension_name,
                "present": True,
            }
        for class_row in extension["classes"]:
            require_fields(class_row, {"name", "kind"}, "registry class")
            classes[class_row["name"].lower()] = {
                **class_row,
                "extension": extension_name,
                "present": True,
            }
        for constant in extension["constants"]:
            require_fields(constant, {"name"}, "registry constant")
            if "has_value" not in constant:
                # Registry dumps produced before the explicit presence bit
                # carried the serialized value instead. Accept that schema so
                # source-index generation remains usable during a toolchain
                # upgrade, while normalizing every row for downstream users.
                constant = {**constant, "has_value": constant.get("value") is not None}
            constants[constant["name"]] = {
                **constant,
                "extension": extension_name,
                "present": True,
            }
    return {
        "status": {"status": "available", "path": str(registry)},
        "functions": functions,
        "classes": classes,
        "constants": constants,
    }


def load_static_rows(php_src: Path) -> tuple[list[dict[str, Any]], list[str]]:
    arginfo = import_arginfo()
    metadata = arginfo.collect_metadata(php_src)
    rows: list[dict[str, Any]] = []
    extensions: set[str] = set()

    for function in metadata.functions:
        extensions.add(function.extension)
        rows.append(
            base_row(
                kind="function",
                name=function.name,
                extension=function.extension,
                source=function.source,
                provenance=["php-src-stub"],
                signature=function_signature(function),
            )
        )
    for class_meta in metadata.classes:
        extensions.add(class_meta.extension)
        rows.append(
            base_row(
                kind=class_meta.kind,
                name=class_meta.name,
                extension=class_meta.extension,
                source=class_meta.source,
                provenance=["php-src-stub"],
            )
        )
    for method in metadata.methods:
        extensions.add(method.extension)
        rows.append(
            base_row(
                kind="method",
                name=method.name,
                class_name=method.class_name,
                extension=method.extension,
                source=method.source,
                provenance=["php-src-stub"],
                signature=function_signature(method),
                static=method.is_static,
            )
        )
    for constant in metadata.constants:
        extensions.add(constant.extension)
        rows.append(
            base_row(
                kind="class_constant" if constant.owner else "constant",
                name=constant.name,
                class_name=constant.owner,
                extension=constant.extension,
                source=constant.source,
                provenance=["php-src-stub"],
                runtime_value={"type": "stub-expression", "value": constant.value},
            )
        )
    extra_rows, extra_gaps = load_extra_stub_rows(php_src, arginfo)
    rows.extend(extra_rows)
    for row in extra_rows:
        if row.get("extension"):
            extensions.add(row["extension"])
    for extension in sorted(extensions):
        rows.append(
            base_row(
                kind="extension",
                name=extension,
                extension=extension,
                source="php-src stubs",
                provenance=["php-src-stub"],
            )
        )
    return rows, [*metadata.gaps, *extra_gaps]


def load_extra_stub_rows(php_src: Path, arginfo: Any) -> tuple[list[dict[str, Any]], list[str]]:
    rows: list[dict[str, Any]] = []
    gaps: set[str] = set()
    for stub in sorted(php_src.rglob("*.stub.php")):
        relative = stub.relative_to(php_src).as_posix()
        extension = arginfo.module_owner(relative)
        text = arginfo.strip_comments(stub.read_text(encoding="utf-8"))
        class_ranges = arginfo.find_class_ranges(text, relative, gaps)
        for class_range in class_ranges:
            body = text[class_range.body_start : class_range.body_end]
            if class_range.kind == "enum":
                for case in parse_enum_cases(body):
                    rows.append(
                        base_row(
                            kind="class_constant",
                            name=case,
                            class_name=class_range.name,
                            extension=extension,
                            source=relative,
                            provenance=["php-src-stub"],
                            enum_case=True,
                        )
                    )
            for property_row in parse_properties(body, class_range.name, extension, relative):
                rows.append(property_row)
        for alias in parse_aliases(text):
            rows.append(
                base_row(
                    kind="alias",
                    name=alias["name"],
                    class_name=alias["target"],
                    extension=extension,
                    source=relative,
                    provenance=["php-src-stub"],
                    autoload_sensitive=True,
                )
            )
    return rows, sorted(gaps)


def parse_enum_cases(body: str) -> list[str]:
    cases = []
    for statement in body.split(";"):
        stripped = statement.strip()
        if not stripped.startswith("case "):
            continue
        name = stripped.split()[1].split("=", 1)[0].strip()
        if name.isidentifier():
            cases.append(name)
    return cases


def parse_properties(
    body: str, class_name: str, extension: str, source: str
) -> list[dict[str, Any]]:
    rows = []
    for statement in body.split(";"):
        stripped = " ".join(statement.strip().split())
        if "$" not in stripped or "function " in stripped:
            continue
        visibility = "public"
        for candidate in ["public", "protected", "private"]:
            if f"{candidate} " in f" {stripped} ":
                visibility = candidate
                break
        name_part = stripped.rsplit("$", 1)[-1].split("=", 1)[0].strip()
        name = ""
        for char in name_part:
            if char.isalnum() or char == "_":
                name += char
            else:
                break
        if not name:
            continue
        rows.append(
            base_row(
                kind="property",
                name=name,
                class_name=class_name,
                extension=extension,
                source=source,
                provenance=["php-src-stub"],
                visibility=visibility,
                static=" static " in f" {stripped} ",
                readonly=" readonly " in f" {stripped} ",
            )
        )
    return rows


def parse_aliases(text: str) -> list[dict[str, str]]:
    aliases = []
    marker = "class_alias("
    start = 0
    while True:
        index = text.find(marker, start)
        if index < 0:
            break
        end = text.find(")", index)
        if end < 0:
            break
        args = [part.strip() for part in text[index + len(marker) : end].split(",")]
        if len(args) >= 2 and is_php_string_literal(args[0]) and is_php_string_literal(args[1]):
            args = [arg.strip("\"'") for arg in args]
            aliases.append({"target": args[0], "name": args[1]})
        start = end + 1
    return aliases


def is_php_string_literal(value: str) -> bool:
    return (
        len(value) >= 2
        and value[0] == value[-1]
        and value[0] in {"'", '"'}
        and "$" not in value
    )


def load_reference_rows(reference_php: Path | None, out_dir: Path) -> ReferencePayload:
    if reference_php is None:
        return ReferencePayload(
            status="skipped",
            reason="REFERENCE_PHP unavailable",
            php=None,
            php_version=PHP_VERSION,
            rows=[],
            gaps=[],
        )
    out_dir.mkdir(parents=True, exist_ok=True)
    script = out_dir / "reference_api_introspection.php"
    script.write_text(reference_script(), encoding="utf-8")
    completed = subprocess.run(
        [str(reference_php), str(script)],
        cwd=REPO_ROOT,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    payload = json.loads(completed.stdout)
    rows = []
    php_version = payload.get("php_version", PHP_VERSION)
    for row in payload.get("rows", []):
        row.setdefault("php_version", php_version)
        row["provenance"] = ["reference-php"]
        rows.append(row)
    return ReferencePayload(
        status="available",
        reason=None,
        php=str(reference_php),
        php_version=php_version,
        rows=rows,
        gaps=payload.get("gaps", []),
    )


def reference_script() -> str:
    return r'''<?php
error_reporting(0);
ini_set("display_errors", "0");
function phrust_value($value) {
    if (is_null($value) || is_bool($value) || is_int($value) || is_float($value) || is_string($value)) {
        return ["type" => gettype($value), "value" => $value];
    }
    if (is_array($value)) {
        return ["type" => "array", "count" => count($value)];
    }
    if (is_object($value)) {
        return ["type" => "object", "class" => get_class($value)];
    }
    return ["type" => gettype($value)];
}
function phrust_param(ReflectionParameter $param) {
    $default = null;
    if ($param->isDefaultValueAvailable()) {
        try {
            $default = phrust_value($param->getDefaultValue());
        } catch (Throwable $error) {
            $default = ["type" => "unavailable"];
        }
    }
    return [
        "name" => $param->getName(),
        "type" => $param->hasType() ? (string)$param->getType() : "mixed",
        "default" => $default,
        "optional" => $param->isOptional(),
        "by_ref" => $param->isPassedByReference(),
        "variadic" => $param->isVariadic(),
    ];
}
function phrust_function_signature($reflector) {
    return [
        "parameters" => array_map("phrust_param", $reflector->getParameters()),
        "return_type" => $reflector->hasReturnType() ? (string)$reflector->getReturnType() : "mixed",
        "return_by_ref" => $reflector->returnsReference(),
    ];
}
function phrust_class_kind(ReflectionClass $class) {
    if ($class->isEnum()) return "enum";
    if ($class->isInterface()) return "interface";
    if ($class->isTrait()) return "trait";
    return "class";
}
function phrust_visibility($reflector) {
    if ($reflector->isPrivate()) return "private";
    if ($reflector->isProtected()) return "protected";
    return "public";
}
$rows = [];
$gaps = [];
foreach (get_loaded_extensions() as $extension) {
    $rows[] = [
        "kind" => "extension",
        "name" => strtolower($extension),
        "class" => null,
        "extension" => strtolower($extension),
        "source" => "reference-php",
        "php_version" => PHP_VERSION,
    ];
}
foreach (get_defined_functions()["internal"] as $function) {
    try {
        $ref = new ReflectionFunction($function);
        $rows[] = [
            "kind" => "function",
            "name" => $ref->getName(),
            "class" => null,
            "extension" => strtolower($ref->getExtensionName() ?: "core"),
            "source" => "reference-php",
            "php_version" => PHP_VERSION,
            "signature" => phrust_function_signature($ref),
            "autoload_sensitive" => false,
        ];
    } catch (Throwable $error) {
        $gaps[] = "function " . $function . ": " . $error->getMessage();
    }
}
foreach (array_merge(get_declared_classes(), get_declared_interfaces(), get_declared_traits()) as $class_name) {
    try {
        $class = new ReflectionClass($class_name);
        if (!$class->isInternal()) continue;
        $extension = strtolower($class->getExtensionName() ?: "core");
        $rows[] = [
            "kind" => phrust_class_kind($class),
            "name" => $class->getName(),
            "class" => null,
            "extension" => $extension,
            "source" => "reference-php",
            "php_version" => PHP_VERSION,
            "abstract" => $class->isAbstract(),
            "final" => $class->isFinal(),
            "readonly" => method_exists($class, "isReadOnly") ? $class->isReadOnly() : false,
            "autoload_sensitive" => false,
        ];
        foreach ($class->getMethods() as $method) {
            if (!$method->isInternal()) continue;
            $rows[] = [
                "kind" => "method",
                "name" => $method->getName(),
                "class" => $class->getName(),
                "extension" => $extension,
                "source" => "reference-php",
                "php_version" => PHP_VERSION,
                "signature" => phrust_function_signature($method),
                "visibility" => phrust_visibility($method),
                "static" => $method->isStatic(),
                "abstract" => $method->isAbstract(),
                "final" => $method->isFinal(),
                "autoload_sensitive" => false,
            ];
        }
        foreach ($class->getReflectionConstants() as $constant) {
            $rows[] = [
                "kind" => "class_constant",
                "name" => $constant->getName(),
                "class" => $class->getName(),
                "extension" => $extension,
                "source" => "reference-php",
                "php_version" => PHP_VERSION,
                "visibility" => phrust_visibility($constant),
                "enum_case" => $constant->isEnumCase(),
                "runtime_value" => phrust_value($constant->getValue()),
            ];
        }
        foreach ($class->getProperties() as $property) {
            $rows[] = [
                "kind" => "property",
                "name" => $property->getName(),
                "class" => $class->getName(),
                "extension" => $extension,
                "source" => "reference-php",
                "php_version" => PHP_VERSION,
                "visibility" => phrust_visibility($property),
                "static" => $property->isStatic(),
                "readonly" => method_exists($property, "isReadOnly") ? $property->isReadOnly() : false,
            ];
        }
    } catch (Throwable $error) {
        $gaps[] = "class " . $class_name . ": " . $error->getMessage();
    }
}
foreach (get_defined_constants(true) as $extension => $constants) {
    foreach ($constants as $name => $value) {
        $rows[] = [
            "kind" => "constant",
            "name" => $name,
            "class" => null,
            "extension" => strtolower($extension),
            "source" => "reference-php",
            "php_version" => PHP_VERSION,
            "runtime_value" => phrust_value($value),
        ];
    }
}
$flags = JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE | JSON_INVALID_UTF8_SUBSTITUTE | JSON_PARTIAL_OUTPUT_ON_ERROR;
echo json_encode(["php_version" => PHP_VERSION, "rows" => $rows, "gaps" => $gaps], $flags) . "\n";
'''


def merge_rows(static_rows: list[dict[str, Any]], reference_rows: list[dict[str, Any]]) -> list[dict[str, Any]]:
    merged: dict[tuple[str, str | None, str], dict[str, Any]] = {}
    for row in static_rows:
        merged[row_key(row)] = row
    for row in reference_rows:
        key = row_key(row)
        if key not in merged:
            merged[key] = normalize_row(row)
            continue
        existing = merged[key]
        existing["provenance"] = sorted(set(existing["provenance"]) | set(row["provenance"]))
        existing["php_version"] = row.get("php_version", existing["php_version"])
        existing["runtime_value"] = row.get("runtime_value", existing.get("runtime_value"))
        if "signature" in row:
            existing.setdefault("reference_signature", row["signature"])
        for field in [
            "visibility",
            "static",
            "abstract",
            "final",
            "readonly",
            "enum_case",
            "autoload_sensitive",
        ]:
            if field in row and field not in existing:
                existing[field] = row[field]
    return list(merged.values())


def classify_rows(
    rows: list[dict[str, Any]], rust: dict[str, Any], reference: ReferencePayload
) -> list[dict[str, Any]]:
    classified = []
    for row in rows:
        row = normalize_row(row)
        row["rust_registry"] = rust_registry_for(row, rust)
        if row["status"] == "extractor_gap":
            classified.append(row)
            continue
        if reference.status != "available":
            row["status"] = "reference_unavailable"
        else:
            row["status"] = status_for(row)
        classified.append(row)
    return sorted(classified, key=row_sort_key)


def rust_only_rows(
    rows: list[dict[str, Any]], rust: dict[str, Any], php_version: str
) -> list[dict[str, Any]]:
    """Expose registry symbols absent from php-src/reference extraction."""
    existing = {row_key(row) for row in rows}
    additions: list[dict[str, Any]] = []
    for entry in rust.get("functions", {}).values():
        key = ("function", "", entry["name"].lower())
        if key in existing:
            continue
        row = base_row(
            kind="function",
            name=entry["name"],
            extension=entry.get("extension") or "core",
            source="Rust extension registry",
            provenance=["rust-registry"],
        )
        row["php_version"] = php_version
        additions.append(row)
    for entry in rust.get("classes", {}).values():
        kind = str(entry.get("kind") or "class").lower()
        if kind not in {"class", "interface", "trait", "enum"}:
            kind = "class"
        key = (kind, "", entry["name"].lower())
        if key in existing:
            continue
        row = base_row(
            kind=kind,
            name=entry["name"],
            extension=entry.get("extension") or "core",
            source="Rust extension registry",
            provenance=["rust-registry"],
        )
        row["php_version"] = php_version
        additions.append(row)
    for entry in rust.get("constants", {}).values():
        key = ("constant", "", entry["name"].lower())
        if key in existing:
            continue
        row = base_row(
            kind="constant",
            name=entry["name"],
            extension=entry.get("extension") or "core",
            source="Rust extension registry",
            provenance=["rust-registry"],
        )
        row["php_version"] = php_version
        additions.append(row)
    return additions


def rust_registry_for(row: dict[str, Any], rust: dict[str, Any]) -> dict[str, Any]:
    if rust["status"]["status"] != "available":
        return {
            "present": False,
            "runtime_builtin": False,
            "class_registered": False,
            "arginfo_present": False,
            "registry_available": False,
        }
    kind = row["kind"]
    if kind == "function":
        entry = rust["functions"].get(row["name"].lower())
        return {
            "present": bool(entry),
            "runtime_builtin": bool(entry and entry["runtime_builtin"]),
            "class_registered": False,
            "arginfo_present": bool(entry and entry["arginfo_source"]),
            "required_parameters": entry.get("required_parameters") if entry else None,
            "total_parameters": entry.get("total_parameters") if entry else None,
            "registry_available": True,
        }
    if kind in {"class", "interface", "trait", "enum", "method", "class_constant", "property"}:
        class_name = row["name"] if kind in {"class", "interface", "trait", "enum"} else row.get("class")
        entry = rust["classes"].get(str(class_name).lower()) if class_name else None
        return {
            "present": bool(entry),
            "runtime_builtin": False,
            "class_registered": bool(entry),
            "arginfo_present": kind == "method" and bool(row.get("signature")),
            "registry_available": True,
        }
    if kind == "constant":
        entry = rust["constants"].get(row["name"])
        return {
            "present": bool(entry),
            "runtime_builtin": False,
            "class_registered": False,
            "arginfo_present": False,
            "registry_available": True,
        }
    if kind == "extension":
        extension = row.get("extension") or row["name"]
        present = any(
            entry.get("extension") == extension
            for group in [rust["functions"], rust["classes"], rust["constants"]]
            for entry in group.values()
        )
        return {
            "present": present,
            "runtime_builtin": False,
            "class_registered": False,
            "arginfo_present": False,
            "registry_available": True,
        }
    return {
        "present": False,
        "runtime_builtin": False,
        "class_registered": False,
        "arginfo_present": False,
        "registry_available": True,
    }


def status_for(row: dict[str, Any]) -> str:
    provenance = set(row["provenance"])
    rust = row["rust_registry"]
    if provenance == {"reference-php"}:
        return "reference_only_known_gap"
    if row["kind"] == "function":
        if not rust["present"]:
            return "missing_in_rust"
        if not rust["runtime_builtin"]:
            return "rust_stub"
        if row.get("signature") and rust.get("total_parameters") is not None:
            if len(row["signature"].get("parameters", [])) != rust["total_parameters"]:
                return "metadata_mismatch"
        return "matched"
    if row["kind"] in {"class", "interface", "trait", "enum", "method", "class_constant", "property"}:
        return "matched" if rust["class_registered"] else "missing_in_rust"
    if row["kind"] in {"constant", "extension"}:
        return "matched" if rust["present"] else "missing_in_rust"
    return "reference_only_known_gap" if "reference-php" in provenance else "missing_in_rust"


def extractor_gap_rows(gaps: list[str], php_version: str) -> list[dict[str, Any]]:
    return [
        {
            "kind": "extension",
            "name": f"extractor-gap-{index + 1}",
            "class": None,
            "extension": "oracle",
            "source": gap.split(":", 1)[0],
            "php_version": php_version,
            "provenance": ["php-src-stub"],
            "signature": None,
            "runtime_value": {"type": "extractor-gap", "value": gap},
            "rust_registry": {},
            "status": "extractor_gap",
        }
        for index, gap in enumerate(gaps)
    ]


def base_row(
    *,
    kind: str,
    name: str,
    class_name: str | None = None,
    extension: str | None = None,
    source: str,
    provenance: list[str],
    signature: dict[str, Any] | None = None,
    runtime_value: dict[str, Any] | None = None,
    visibility: str | None = None,
    static: bool | None = None,
    abstract: bool | None = None,
    final: bool | None = None,
    readonly: bool | None = None,
    enum_case: bool | None = None,
    autoload_sensitive: bool | None = None,
) -> dict[str, Any]:
    row: dict[str, Any] = {
        "kind": kind,
        "name": name,
        "class": class_name,
        "extension": normalize_extension(extension or "core"),
        "source": source,
        "php_version": PHP_VERSION,
        "provenance": provenance,
        "signature": signature,
        "runtime_value": runtime_value,
        "rust_registry": {},
        "status": "reference_unavailable",
    }
    for key, value in [
        ("visibility", visibility),
        ("static", static),
        ("abstract", abstract),
        ("final", final),
        ("readonly", readonly),
        ("enum_case", enum_case),
        ("autoload_sensitive", autoload_sensitive),
    ]:
        if value is not None:
            row[key] = value
    return normalize_row(row)


def function_signature(function: Any) -> dict[str, Any]:
    return {
        "parameters": [
            {
                "name": param.name,
                "type": param.type_decl,
                "default": param.default_value,
                "optional": param.optional,
                "by_ref": param.by_ref,
                "variadic": param.variadic,
            }
            for param in function.params
        ],
        "return_type": function.return_type,
        "return_by_ref": False,
    }


def normalize_row(row: dict[str, Any]) -> dict[str, Any]:
    row = dict(row)
    row.setdefault("class", None)
    row.setdefault("extension", "core")
    row["extension"] = normalize_extension(row["extension"])
    row.setdefault("source", "")
    row.setdefault("php_version", PHP_VERSION)
    row.setdefault("provenance", [])
    row["provenance"] = sorted(set(row["provenance"]))
    row.setdefault("signature", None)
    row.setdefault("runtime_value", None)
    row.setdefault("rust_registry", {})
    row.setdefault("status", "reference_unavailable")
    return row


def validate_rows(rows: list[dict[str, Any]]) -> None:
    previous: tuple[Any, ...] | None = None
    for row in rows:
        for field in [
            "kind",
            "name",
            "class",
            "extension",
            "source",
            "php_version",
            "provenance",
            "signature",
            "runtime_value",
            "rust_registry",
            "status",
        ]:
            if field not in row:
                raise ValueError(f"row missing {field}: {row}")
        if row["kind"] not in KINDS:
            raise ValueError(f"unsupported row kind {row['kind']!r}")
        if row["status"] not in STATUSES:
            raise ValueError(f"unsupported row status {row['status']!r}")
        key = row_sort_key(row)
        if previous is not None and previous > key:
            raise ValueError("rows are not sorted")
        previous = key


def write_jsonl(path: Path, rows: list[dict[str, Any]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as handle:
        for row in rows:
            handle.write(json.dumps(row, sort_keys=True, separators=(",", ":")) + "\n")


def render_summary(
    *,
    rows: list[dict[str, Any]],
    php_src: Path,
    reference: ReferencePayload,
    rust_status: dict[str, Any],
    extractor_gaps: list[str],
    out: Path,
) -> str:
    by_status = count_by(rows, "status")
    by_kind = count_by(rows, "kind")
    lines = [
        "# PHP Source API Oracle Summary",
        "",
        "Generated by `scripts/oracle/api_index.py` via `just oracle-api-index`.",
        "",
        f"- php-src: `{php_src}`",
        f"- Reference PHP: `{reference.status}`"
        + (f" (`{reference.php}`)" if reference.php else f" ({reference.reason})"),
        f"- Rust registry: `{rust_status['status']}`",
        f"- JSONL: `{relative(out)}`",
        "",
        "## Status Counts",
        "",
        "| Status | Count |",
        "| --- | ---: |",
    ]
    for status in sorted(STATUSES):
        lines.append(f"| `{status}` | {by_status.get(status, 0)} |")
    lines.extend(["", "## Kind Counts", "", "| Kind | Count |", "| --- | ---: |"])
    for kind in sorted(KINDS):
        lines.append(f"| `{kind}` | {by_kind.get(kind, 0)} |")
    lines.extend(["", "## Extractor Gaps", ""])
    if extractor_gaps or reference.gaps:
        for gap in extractor_gaps[:50]:
            lines.append(f"- `{gap}`")
        for gap in reference.gaps[:50]:
            lines.append(f"- `reference: {gap}`")
        if len(extractor_gaps) + len(reference.gaps) > 100:
            lines.append("- Additional gaps omitted from the concise summary; see JSONL rows.")
    else:
        lines.append("No extractor gaps reported.")
    lines.extend(["", "## Next Consumers", ""])
    lines.append("- `oracle-probe-generate` consumes missing/stub/mismatch rows for behavior probes.")
    lines.append("- `oracle-gap-report` consumes the same statuses for ratchets and docs summaries.")
    lines.append("")
    return "\n".join(lines)


def count_by(rows: list[dict[str, Any]], field: str) -> dict[str, int]:
    counts: dict[str, int] = {}
    for row in rows:
        key = row[field]
        counts[key] = counts.get(key, 0) + 1
    return counts


def require_fields(row: dict[str, Any], fields: set[str], label: str) -> None:
    missing = sorted(fields - set(row))
    if missing:
        raise ValueError(f"{label} missing fields: {', '.join(missing)}")


def row_key(row: dict[str, Any]) -> tuple[str, str | None, str]:
    return (row["kind"], normalize_nullable(row.get("class")), row["name"].lower())


def row_sort_key(row: dict[str, Any]) -> tuple[str, str, str, str, str]:
    return (
        row["kind"],
        normalize_nullable(row.get("class")).lower(),
        row["name"].lower(),
        row.get("extension") or "",
        ",".join(row.get("provenance", [])),
    )


def normalize_nullable(value: Any) -> str:
    return "" if value is None else str(value)


def normalize_extension(value: str) -> str:
    return value.lower().replace("zend", "core")


def relative(path: Path) -> str:
    try:
        return path.relative_to(REPO_ROOT).as_posix()
    except ValueError:
        return str(path)


def run_self_tests() -> None:
    sample_rows = [
        base_row(
            kind="function",
            name="b",
            extension="standard",
            source="fixture",
            provenance=["php-src-stub"],
        ),
        base_row(
            kind="function",
            name="a",
            extension="standard",
            source="fixture",
            provenance=["php-src-stub"],
        ),
    ]
    sorted_rows = sorted(sample_rows, key=row_sort_key)
    validate_rows(sorted_rows)
    if [row["name"] for row in sorted_rows] != ["a", "b"]:
        raise AssertionError("row sorting self-test failed")
    registry_only = rust_only_rows(
        sample_rows,
        {
            "functions": {
                "runtime_only": {
                    "name": "runtime_only",
                    "extension": "test",
                    "runtime_builtin": True,
                }
            },
            "classes": {},
            "constants": {},
        },
        PHP_VERSION,
    )
    if len(registry_only) != 1 or registry_only[0]["name"] != "runtime_only":
        raise AssertionError("Rust-only registry symbol was not indexed exactly once")

    with tempfile.TemporaryDirectory() as temp_dir:
        root = Path(temp_dir)
        stub = root / "ext/standard/basic.stub.php"
        stub.parent.mkdir(parents=True)
        stub.write_text(
            "<?php\nfunction oracle_by_ref(string &$value, ...$rest): void {}\n",
            encoding="utf-8",
        )
        rows, _ = load_static_rows(root)
        row = next(row for row in rows if row["kind"] == "function" and row["name"] == "oracle_by_ref")
        first_param = row["signature"]["parameters"][0]
        if not first_param["by_ref"]:
            raise AssertionError("by_ref=true was not preserved in oracle rows")
        if not row["signature"]["parameters"][1]["variadic"]:
            raise AssertionError("variadic=true was not preserved in oracle rows")


if __name__ == "__main__":
    raise SystemExit(main())
