#!/usr/bin/env python3
"""Generate checked Rust extension surfaces from canonical JSON descriptors."""

from __future__ import annotations

import argparse
import json
import re
import shutil
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
SCHEMA_VERSION = 1
BUILTIN_RE = re.compile(r'BuiltinEntry::new\(\s*"([^"]+)"', re.MULTILINE)


class DescriptorError(ValueError):
    """A canonical descriptor violates the generated-surface contract."""


@dataclass(frozen=True)
class SourceArginfo:
    extension: str
    source: str
    return_type: str
    required_parameters: int
    total_parameters: int
    variadic: bool


def load_descriptors(schema_dir: Path) -> tuple[dict[str, Any], list[dict[str, Any]]]:
    index = load_json(schema_dir / "index.json")
    if index.get("schema_version") != SCHEMA_VERSION:
        raise DescriptorError("unsupported extension descriptor schema version")
    names = index.get("extensions")
    if not isinstance(names, list) or names != sorted(set(names)):
        raise DescriptorError("index extensions must be unique and sorted")
    descriptors = [load_json(schema_dir / f"{name}.json") for name in names]
    validate_descriptors(index, descriptors)
    return index, descriptors


def load_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise DescriptorError(f"cannot load {path}: {error}") from error
    if not isinstance(value, dict):
        raise DescriptorError(f"{path} must contain a JSON object")
    return value


def validate_descriptors(index: dict[str, Any], descriptors: list[dict[str, Any]]) -> None:
    expected_names = index["extensions"]
    actual_names = [descriptor.get("name") for descriptor in descriptors]
    if actual_names != expected_names:
        raise DescriptorError("descriptor filenames and names disagree with index")

    function_owners: dict[str, str] = {}
    class_owners: dict[str, str] = {}
    constant_owners: dict[str, str] = {}
    for descriptor in descriptors:
        name = descriptor["name"]
        if descriptor.get("schema_version") != SCHEMA_VERSION:
            raise DescriptorError(f"{name}: unsupported schema version")
        for field in ("functions", "classes", "constants", "dependencies", "capabilities"):
            if not isinstance(descriptor.get(field), list):
                raise DescriptorError(f"{name}: {field} must be an array")
        validate_sorted_symbols(name, "functions", descriptor["functions"])
        validate_sorted_symbols(name, "classes", descriptor["classes"])
        for function in descriptor["functions"]:
            symbol = require_name(name, "function", function)
            lowered = symbol.lower()
            previous = function_owners.setdefault(lowered, name)
            if previous != name:
                raise DescriptorError(
                    f"duplicate function {symbol!r} in {previous} and {name}"
                )
            visibility = function.get("visibility")
            if visibility not in {"php", "internal_test"}:
                raise DescriptorError(f"{name}:{symbol}: invalid visibility {visibility!r}")
            implementations = function.get("implementations")
            if not isinstance(implementations, list):
                raise DescriptorError(f"{name}:{symbol}: implementations must be an array")
            if not implementations and "implementation_gap" not in function:
                raise DescriptorError(f"{name}:{symbol}: missing implementation mapping")
            if "arginfo" not in function and "signature_gap" not in function:
                raise DescriptorError(f"{name}:{symbol}: missing signature metadata or gap")
            if "arginfo" in function and "signature_gap" in function:
                raise DescriptorError(f"{name}:{symbol}: arginfo and signature gap conflict")
            seen_implementations: set[tuple[str, str | None]] = set()
            for implementation in implementations:
                kind = implementation.get("kind")
                module = implementation.get("module")
                if kind not in {"runtime", "extension", "vm"}:
                    raise DescriptorError(
                        f"{name}:{symbol}: invalid implementation kind {kind!r}"
                    )
                if kind == "vm" and module is not None:
                    raise DescriptorError(f"{name}:{symbol}: VM mapping cannot name a module")
                if kind != "vm" and not isinstance(module, str):
                    raise DescriptorError(f"{name}:{symbol}: mapping requires a module")
                key = (kind, module)
                if key in seen_implementations:
                    raise DescriptorError(f"{name}:{symbol}: duplicate implementation mapping")
                seen_implementations.add(key)
        for class_like in descriptor["classes"]:
            symbol = require_name(name, "class", class_like)
            kind = class_like.get("kind")
            if kind not in {"class", "interface", "trait", "enum"}:
                raise DescriptorError(f"{name}:{symbol}: invalid class kind {kind!r}")
            lowered = symbol.lower()
            previous = class_owners.setdefault(lowered, name)
            if previous != name:
                raise DescriptorError(f"duplicate class {symbol!r} in {previous} and {name}")
        for constant in descriptor["constants"]:
            symbol = require_name(name, "constant", constant)
            previous = constant_owners.setdefault(symbol, name)
            if previous != name:
                raise DescriptorError(
                    f"duplicate constant {symbol!r} in {previous} and {name}"
                )
            validate_constant_value(name, symbol, constant.get("value"))


def validate_sorted_symbols(owner: str, field: str, values: list[dict[str, Any]]) -> None:
    names = [require_name(owner, field.rstrip("s"), value) for value in values]
    if names != sorted(names):
        raise DescriptorError(f"{owner}: {field} must use stable byte ordering")


def require_name(owner: str, kind: str, value: Any) -> str:
    if not isinstance(value, dict) or not isinstance(value.get("name"), str):
        raise DescriptorError(f"{owner}: {kind} entry requires a string name")
    return value["name"]


def validate_constant_value(owner: str, name: str, value: Any) -> None:
    if value is None:
        return
    if not isinstance(value, dict) or value.get("kind") not in {
        "null",
        "bool",
        "int",
        "float",
        "string",
        "array",
    }:
        raise DescriptorError(f"{owner}:{name}: invalid constant value")
    kind = value["kind"]
    if kind == "array":
        values = value.get("values")
        if not isinstance(values, list):
            raise DescriptorError(f"{owner}:{name}: array value requires values")
        for item in values:
            validate_constant_value(owner, name, item)


def load_arginfo(path: Path) -> dict[str, SourceArginfo]:
    """Read the committed output of the separately verified php-src extractor."""
    source = path.read_text(encoding="utf-8")
    start_marker = "pub const GENERATED_FUNCTIONS: &[GeneratedFunctionMetadata] = &["
    end_marker = "pub const GENERATED_CLASSES: &[GeneratedClassMetadata] = &["
    try:
        section = source.split(start_marker, maxsplit=1)[1].split(end_marker, maxsplit=1)[0]
    except IndexError as error:
        raise DescriptorError(f"{path}: generated arginfo function section is missing") from error
    blocks = split_rust_structs(section, "GeneratedFunctionMetadata")
    result: dict[str, SourceArginfo] = {}
    field = lambda name, block: re.search(  # noqa: E731 - compact parser helper.
        rf'{name}:\s*"((?:\\.|[^"])*)"', block
    )
    for block in blocks:
        matches = {name: field(name, block) for name in ("name", "extension", "source", "return_type")}
        if any(match is None for match in matches.values()):
            raise DescriptorError(f"{path}: malformed generated function metadata block")
        values = {name: rust_unescape(match.group(1)) for name, match in matches.items()}
        total = block.count("GeneratedParamMetadata {")
        required = len(re.findall(r"optional:\s*false", block))
        metadata = SourceArginfo(
            extension=values["extension"],
            source=values["source"],
            return_type=values["return_type"],
            required_parameters=required,
            total_parameters=total,
            variadic=bool(re.search(r"variadic:\s*true", block)),
        )
        lowered = values["name"].lower()
        if lowered in result:
            raise DescriptorError(f"{path}: duplicate generated arginfo function {lowered}")
        result[lowered] = metadata
    if not result:
        raise DescriptorError(f"{path}: no generated arginfo functions found")
    return result


def split_rust_structs(section: str, type_name: str) -> list[str]:
    blocks = []
    cursor = 0
    marker = f"{type_name} {{"
    while True:
        start = section.find(marker, cursor)
        if start < 0:
            return blocks
        brace = section.find("{", start)
        depth = 0
        for index in range(brace, len(section)):
            if section[index] == "{":
                depth += 1
            elif section[index] == "}":
                depth -= 1
                if depth == 0:
                    blocks.append(section[start : index + 1])
                    cursor = index + 1
                    break
        else:
            raise DescriptorError(f"unterminated {type_name} block")


def rust_unescape(value: str) -> str:
    return bytes(value, "utf-8").decode("unicode_escape")


def validate_arginfo(
    descriptors: list[dict[str, Any]], arginfo: dict[str, SourceArginfo]
) -> None:
    for descriptor in descriptors:
        for function in descriptor["functions"]:
            name = function["name"]
            source = arginfo.get(name.lower())
            declared = function.get("arginfo")
            if declared is None:
                if source is not None:
                    raise DescriptorError(
                        f"{descriptor['name']}:{name}: signature gap hides available arginfo"
                    )
                continue
            if source is None:
                raise DescriptorError(f"{descriptor['name']}:{name}: pinned arginfo disappeared")
            if declared != {"extension": source.extension, "source": source.source}:
                raise DescriptorError(f"{descriptor['name']}:{name}: arginfo source drift")
            if source.extension != descriptor["name"] and not function.get(
                "arginfo_owner_override"
            ):
                raise DescriptorError(
                    f"{descriptor['name']}:{name}: arginfo owner disagreement requires override"
                )
            if source.extension == descriptor["name"] and function.get(
                "arginfo_owner_override"
            ):
                raise DescriptorError(
                    f"{descriptor['name']}:{name}: stale arginfo owner override"
                )


def source_runtime_mappings() -> dict[str, set[tuple[str, str]]]:
    mappings: dict[str, set[tuple[str, str]]] = {}
    roots = (
        (ROOT / "crates/php_runtime/src/builtins/modules", "runtime"),
        (ROOT / "crates/php_extensions/src", "extension"),
    )
    for source_root, kind in roots:
        for path in sorted(source_root.rglob("*.rs")):
            if path.name == "lib.rs":
                continue
            for name in BUILTIN_RE.findall(path.read_text(encoding="utf-8")):
                mappings.setdefault(name.lower(), set()).add((kind, path.stem))
    return mappings


def validate_runtime_mappings(descriptors: list[dict[str, Any]]) -> None:
    expected: dict[str, set[tuple[str, str]]] = {}
    for descriptor in descriptors:
        for function in descriptor["functions"]:
            for implementation in function["implementations"]:
                if implementation["kind"] in {"runtime", "extension"}:
                    expected.setdefault(function["name"].lower(), set()).add(
                        (implementation["kind"], implementation["module"])
                    )
    actual = source_runtime_mappings()
    if expected != actual:
        missing = sorted((name, mapping) for name, values in expected.items() for mapping in values - actual.get(name, set()))
        extra = sorted((name, mapping) for name, values in actual.items() for mapping in values - expected.get(name, set()))
        details = []
        if missing:
            details.append(f"missing source mappings: {missing[:8]}")
        if extra:
            details.append(f"unowned source mappings: {extra[:8]}")
        raise DescriptorError("runtime implementation drift; " + "; ".join(details))


def generate(
    index: dict[str, Any],
    descriptors: list[dict[str, Any]],
    arginfo: dict[str, SourceArginfo],
    out_root: Path,
) -> None:
    write_php_std(descriptors, out_root / "crates/php_std/src/generated/extensions")
    write_php_runtime(
        index, descriptors, arginfo, out_root / "crates/php_runtime/src/builtins/generated"
    )
    write_php_extensions(descriptors, out_root / "crates/php_extensions/src/generated.rs")


def reset_dir(path: Path) -> None:
    if path.exists():
        shutil.rmtree(path)
    path.mkdir(parents=True)


def write_php_std(descriptors: list[dict[str, Any]], out_dir: Path) -> None:
    reset_dir(out_dir)
    modules = []
    for descriptor in descriptors:
        module = rust_identifier(descriptor["name"])
        modules.append(module)
        (out_dir / f"{module}.rs").write_text(
            render_php_std_extension(descriptor), encoding="utf-8"
        )
    lines = generated_header("canonical extension descriptors")
    lines.extend(f"mod {module};" for module in modules)
    lines.extend(["", "pub(crate) fn descriptors() -> Vec<crate::ExtensionDescriptor> {", "    vec!["])
    lines.extend(f"        {module}::descriptor()," for module in modules)
    lines.extend(["    ]", "}", ""])
    (out_dir / "mod.rs").write_text("\n".join(lines), encoding="utf-8")


def render_php_std_extension(descriptor: dict[str, Any]) -> str:
    name = descriptor["name"]
    lines = generated_header(f"canonical {name} extension descriptor")
    imports = [
        "ClassDescriptor",
        "ConstantDescriptor",
        "ExtensionDescriptor",
        "FunctionDescriptor",
    ]
    if descriptor["classes"]:
        imports.append("ClassKind")
    if descriptor["functions"]:
        imports.append("SymbolVisibility")
    if any(constant.get("value") is not None for constant in descriptor["constants"]):
        imports.append("ConstantValue")
    imports.sort()
    lines.extend(
        [
            f"use crate::{{{', '.join(imports)}}};",
            "",
            "pub(crate) fn descriptor() -> ExtensionDescriptor {",
            "    ExtensionDescriptor::from_generated(",
            f"        {rust_string(name)},",
            f"        {rust_string(descriptor['version'])},",
            f"        {rust_bool(descriptor['enabled_by_default'])},",
            "        FUNCTIONS,",
            "        CONSTANTS,",
            "        CLASSES,",
            "        DEPENDENCIES,",
            "        CAPABILITIES,",
            f"        {rust_option_string(state_slot_name(descriptor))},",
            "    )",
            "}",
            "",
            "const DEPENDENCIES: &[&str] = &[",
        ]
    )
    if any(
        constant_value_contains_kind(constant.get("value"), "float")
        for constant in descriptor["constants"]
    ):
        lines[4:4] = ["use php_runtime::api::FloatValue;", ""]
    lines.extend(f"    {rust_string(value)}," for value in descriptor["dependencies"])
    lines.extend(["] ;".replace(" ", ""), "", "const CAPABILITIES: &[&str] = &["])
    lines.extend(f"    {rust_string(value)}," for value in descriptor["capabilities"])
    lines.extend(["];", "", "const FUNCTIONS: &[FunctionDescriptor] = &["])
    for function in descriptor["functions"]:
        visibility = (
            "SymbolVisibility::PhpVisible"
            if function["visibility"] == "php"
            else "SymbolVisibility::InternalTestFixture"
        )
        runtime_module = implementation_module(function, "runtime")
        extension_module = implementation_module(function, "extension")
        vm_mediated = any(item["kind"] == "vm" for item in function["implementations"])
        lines.extend(
            [
                "    FunctionDescriptor::generated(",
                f"        {rust_string(function['name'])},",
                f"        {rust_string(name)},",
                f"        {visibility},",
                f"        {rust_option_string(runtime_module)},",
                f"        {rust_option_string(extension_module)},",
                f"        {rust_bool(vm_mediated)},",
                "    ),",
            ]
        )
    lines.extend(["];", "", "const CLASSES: &[ClassDescriptor] = &["])
    for class_like in descriptor["classes"]:
        kind = class_like["kind"].title()
        lines.append(
            f"    ClassDescriptor::new({rust_string(class_like['name'])}, {rust_string(name)}, ClassKind::{kind}),"
        )
    lines.extend(["];", "", "const CONSTANTS: &[ConstantDescriptor] = &["])
    for constant in descriptor["constants"]:
        expression = render_constant(name, constant)
        lines.append(f"    {expression},")
    lines.extend(["];", ""])
    return "\n".join(lines)


def render_constant(extension: str, constant: dict[str, Any]) -> str:
    name = rust_string(constant["name"])
    owner = rust_string(extension)
    value = constant.get("value")
    if value is None:
        expression = f"ConstantDescriptor::new({name}, {owner})"
    elif constant["name"] in PLATFORM_STRING_CONSTANTS:
        expression = (
            f"ConstantDescriptor::with_value({name}, {owner}, "
            f"ConstantValue::String({PLATFORM_STRING_CONSTANTS[constant['name']]}))"
        )
    else:
        expression = (
            f"ConstantDescriptor::with_value({name}, {owner}, {render_constant_value(value)})"
        )
    deprecation = constant.get("deprecation")
    if deprecation is not None:
        expression += f".deprecated({rust_string(deprecation)})"
    return expression


# The canonical descriptors are extracted on one reference host. These values
# must follow the Rust target instead of freezing the extraction host into the
# generated standard-library surface.
PLATFORM_STRING_CONSTANTS = {
    "DIRECTORY_SEPARATOR": "crate::constants::DIRECTORY_SEPARATOR",
    "PATH_SEPARATOR": "crate::constants::PATH_SEPARATOR",
    "PHP_EOL": "crate::constants::PHP_EOL",
    "PHP_OS": "crate::constants::PHP_OS",
    "PHP_OS_FAMILY": "crate::constants::PHP_OS_FAMILY",
}


def render_constant_value(value: dict[str, Any]) -> str:
    kind = value["kind"]
    if kind == "null":
        return "ConstantValue::Null"
    if kind == "bool":
        return f"ConstantValue::Bool({rust_bool(value['value'])})"
    if kind == "int":
        return f"ConstantValue::Int({value['value']})"
    if kind == "float":
        return f"ConstantValue::Float(FloatValue::from_f64(f64::from_bits({value['bits']})))"
    if kind == "string":
        return f"ConstantValue::String({rust_string(value['value'])})"
    values = ", ".join(render_constant_value(item) for item in value["values"])
    return f"ConstantValue::Array(&[{values}])"


def constant_value_contains_kind(value: Any, kind: str) -> bool:
    if not isinstance(value, dict):
        return False
    if value.get("kind") == kind:
        return True
    return any(constant_value_contains_kind(item, kind) for item in value.get("values", []))


def write_php_runtime(
    index: dict[str, Any],
    descriptors: list[dict[str, Any]],
    arginfo: dict[str, SourceArginfo],
    out_dir: Path,
) -> None:
    reset_dir(out_dir)
    functions_by_module: dict[str, list[tuple[str, str]]] = {
        module: [] for module in index["runtime_module_order"]
    }
    for descriptor in descriptors:
        for function in descriptor["functions"]:
            module = implementation_module(function, "runtime")
            if module is not None:
                functions_by_module.setdefault(module, []).append(
                    (function["name"], descriptor["name"])
                )
    modules = index["runtime_module_order"]
    if set(modules) != set(functions_by_module):
        raise DescriptorError("runtime module order does not cover generated modules")
    for module in modules:
        rows = sorted(functions_by_module[module], key=lambda item: item[0].lower())
        lines = generated_header(f"canonical builtin signatures for {module}")
        lines.extend(["use super::GeneratedBuiltinDescriptor;", "", "pub const FUNCTIONS: &[GeneratedBuiltinDescriptor] = &["])
        for name, extension in rows:
            signature = arginfo.get(name.lower())
            if signature is None:
                signature_fields = ("None", "0", "0", "false")
            else:
                signature_fields = (
                    f"Some({rust_string(signature.return_type)})",
                    str(signature.required_parameters),
                    str(signature.total_parameters),
                    rust_bool(signature.variadic),
                )
            lines.extend(
                [
                    "    GeneratedBuiltinDescriptor {",
                    f"        name: {rust_string(name)},",
                    f"        extension: {rust_string(extension)},",
                    f"        return_type: {signature_fields[0]},",
                    f"        required_parameters: {signature_fields[1]},",
                    f"        total_parameters: {signature_fields[2]},",
                    f"        variadic: {signature_fields[3]},",
                    "    },",
                ]
            )
        lines.extend(["];", ""])
        (out_dir / f"{module}.rs").write_text("\n".join(lines), encoding="utf-8")
    lines = generated_header("canonical builtin module registry")
    lines.extend(
        [
            "#[derive(Clone, Copy, Debug, Eq, PartialEq)]",
            "pub struct GeneratedBuiltinDescriptor {",
            "    pub name: &'static str,",
            "    pub extension: &'static str,",
            "    pub return_type: Option<&'static str>,",
            "    pub required_parameters: usize,",
            "    pub total_parameters: usize,",
            "    pub variadic: bool,",
            "}",
            "",
            "#[derive(Clone, Copy, Debug)]",
            "pub struct GeneratedBuiltinModule {",
            "    pub name: &'static str,",
            "    pub functions: &'static [GeneratedBuiltinDescriptor],",
            "}",
            "",
        ]
    )
    lines.extend(f"mod {module};" for module in modules)
    lines.extend(["", "pub const MODULES: &[GeneratedBuiltinModule] = &["])
    lines.extend(
        f"    GeneratedBuiltinModule {{ name: {rust_string(module)}, functions: {module}::FUNCTIONS }},"
        for module in modules
    )
    lines.extend(["];", ""])
    (out_dir / "mod.rs").write_text("\n".join(lines), encoding="utf-8")


def write_php_extensions(descriptors: list[dict[str, Any]], out: Path) -> None:
    selected = [item for item in descriptors if any(
        implementation["kind"] == "extension"
        for function in item["functions"]
        for implementation in function["implementations"]
    )]
    lines = generated_header("canonical external extension lifecycle metadata")
    lines.extend(
        [
            "use php_runtime::api::ExtensionCapability;",
            "",
            "#[derive(Clone, Copy, Debug)]",
            "pub struct GeneratedExtensionMetadata {",
            "    pub name: &'static str,",
            "    pub version: &'static str,",
            "    pub dependencies: &'static [&'static str],",
            "    pub capabilities: &'static [ExtensionCapability],",
            "    pub state_slot: Option<&'static str>,",
            "}",
            "",
        ]
    )
    for descriptor in selected:
        constant = rust_identifier(descriptor["name"]).upper()
        capabilities = ", ".join(
            f"ExtensionCapability::{capability_variant(item)}"
            for item in descriptor["capabilities"]
        )
        dependencies = ", ".join(rust_string(item) for item in descriptor["dependencies"])
        lines.extend(
            [
                f"pub const {constant}: GeneratedExtensionMetadata = GeneratedExtensionMetadata {{",
                f"    name: {rust_string(descriptor['name'])},",
                f"    version: {rust_string(descriptor['version'])},",
                f"    dependencies: &[{dependencies}],",
                f"    capabilities: &[{capabilities}],",
                f"    state_slot: {rust_option_string(state_slot_name(descriptor))},",
                "};",
                "",
            ]
        )
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text("\n".join(lines), encoding="utf-8")


def implementation_module(function: dict[str, Any], kind: str) -> str | None:
    modules = [item["module"] for item in function["implementations"] if item["kind"] == kind]
    if len(modules) > 1:
        raise DescriptorError(f"{function['name']}: multiple {kind} implementation modules")
    return modules[0] if modules else None


def state_slot_name(descriptor: dict[str, Any]) -> str | None:
    state_slot = descriptor.get("state_slot")
    return state_slot.get("type") if isinstance(state_slot, dict) else None


def capability_variant(value: str) -> str:
    variants = {"clock": "Clock", "filesystem": "Filesystem", "network": "Network", "process_shared_state": "ProcessSharedState"}
    try:
        return variants[value]
    except KeyError as error:
        raise DescriptorError(f"unknown extension capability {value!r}") from error


def generated_header(subject: str) -> list[str]:
    return [
        "// @generated by scripts/stdlib/generate_extension_surfaces.py",
        f"// {subject}; edit fixtures/stdlib/extensions instead.",
        "",
    ]


def rust_identifier(value: str) -> str:
    identifier = re.sub(r"[^a-zA-Z0-9_]", "_", value).lower()
    if not identifier or identifier[0].isdigit():
        identifier = f"extension_{identifier}"
    return identifier


def rust_string(value: str) -> str:
    escaped = []
    for character in value:
        if character == '"':
            escaped.append('\\"')
        elif character == "\\":
            escaped.append("\\\\")
        elif character == "\n":
            escaped.append("\\n")
        elif character == "\r":
            escaped.append("\\r")
        elif character == "\t":
            escaped.append("\\t")
        elif character.isprintable():
            escaped.append(character)
        else:
            escaped.append(f"\\u{{{ord(character):x}}}")
    return '"' + "".join(escaped) + '"'


def rust_option_string(value: str | None) -> str:
    return "None" if value is None else f"Some({rust_string(value)})"


def rust_bool(value: bool) -> str:
    return "true" if value else "false"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--schema-dir", type=Path, required=True)
    parser.add_argument("--arginfo", type=Path, required=True)
    parser.add_argument("--out-root", type=Path, default=ROOT)
    parser.add_argument("--skip-source-mapping-check", action="store_true")
    args = parser.parse_args()
    try:
        index, descriptors = load_descriptors(args.schema_dir)
        arginfo = load_arginfo(args.arginfo)
        validate_arginfo(descriptors, arginfo)
        if not args.skip_source_mapping_check:
            validate_runtime_mappings(descriptors)
        generate(index, descriptors, arginfo, args.out_root)
    except (DescriptorError, FileNotFoundError, OSError) as error:
        print(f"extension surface generation error: {error}", file=sys.stderr)
        return 1
    print(f"[ok] generated canonical extension surfaces under {args.out_root}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
