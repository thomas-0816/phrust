#!/usr/bin/env python3
"""Generate deterministic Rust arginfo metadata from php-src stub files."""

from __future__ import annotations

import argparse
import re
import sys
from dataclasses import dataclass, replace
from pathlib import Path


CLASS_RE = re.compile(
    r"\b(?P<kind>class|interface|trait|enum)\s+"
    r"(?P<name>[A-Za-z_\\][A-Za-z0-9_\\]*)[^{;]*\{",
    re.DOTALL,
)
NAMESPACE_RE = re.compile(
    r"\bnamespace\s+(?P<name>[A-Za-z_\\][A-Za-z0-9_\\]*)\s*[;{]",
    re.DOTALL,
)
FUNCTION_RE = re.compile(
    r"(?P<static>\bstatic\s+)?function\s+"
    r"(?P<name>[A-Za-z_\\][A-Za-z0-9_\\]*)\s*"
    r"\((?P<params>.*?)\)\s*(?::\s*(?P<return>[^;{]+))?[;{]",
    re.DOTALL,
)
PARAM_RE = re.compile(
    r"(?P<prefix>.*?)"
    r"(?P<byref>&\s*)?"
    r"(?P<variadic>\.\.\.\s*)?"
    r"\$(?P<name>[A-Za-z_][A-Za-z0-9_]*)\s*$",
    re.DOTALL,
)


@dataclass(frozen=True)
class ParamMetadata:
    name: str
    type_decl: str
    default_value: str | None
    optional: bool
    by_ref: bool
    variadic: bool


@dataclass(frozen=True)
class FunctionMetadata:
    name: str
    extension: str
    source: str
    return_type: str
    params: tuple[ParamMetadata, ...]


@dataclass(frozen=True)
class MethodMetadata:
    class_name: str
    name: str
    extension: str
    source: str
    return_type: str
    params: tuple[ParamMetadata, ...]
    is_static: bool


@dataclass(frozen=True)
class ConstantMetadata:
    owner: str | None
    name: str
    extension: str
    source: str
    type_decl: str
    value: str


@dataclass(frozen=True)
class ClassMetadata:
    name: str
    kind: str
    extension: str
    source: str


@dataclass(frozen=True)
class GeneratedMetadata:
    functions: tuple[FunctionMetadata, ...]
    classes: tuple[ClassMetadata, ...]
    methods: tuple[MethodMetadata, ...]
    constants: tuple[ConstantMetadata, ...]
    override_count: int
    gaps: tuple[str, ...]


@dataclass(frozen=True)
class ClassRange:
    name: str
    kind: str
    body_start: int
    body_end: int
    full_start: int
    full_end: int


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--php-src", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--overrides", type=Path)
    args = parser.parse_args()

    try:
        metadata = collect_metadata(args.php_src)
        if args.overrides:
            metadata = apply_overrides(metadata, args.overrides)
        write_rust(args.out, metadata)
    except Exception as error:  # noqa: BLE001 - script boundary.
        print(f"standard-library arginfo generation error: {error}", file=sys.stderr)
        return 1
    print(f"[ok] wrote {args.out}")
    print(f"functions={len(metadata.functions)}")
    print(f"classes={len(metadata.classes)}")
    print(f"methods={len(metadata.methods)}")
    print(f"constants={len(metadata.constants)}")
    print(f"overrides={metadata.override_count}")
    print(f"extractor_gaps={len(metadata.gaps)}")
    return 0


def collect_metadata(php_src: Path) -> GeneratedMetadata:
    if not php_src.exists():
        raise FileNotFoundError(f"php-src path does not exist: {php_src}")

    functions: dict[str, FunctionMetadata] = {}
    classes: dict[str, ClassMetadata] = {}
    methods: dict[tuple[str, str], MethodMetadata] = {}
    constants: dict[tuple[str | None, str], ConstantMetadata] = {}
    gaps: set[str] = set()

    for stub in sorted(php_src.rglob("*.stub.php")):
        relative = stub.relative_to(php_src).as_posix()
        extension = module_owner(relative)
        raw = stub.read_text(encoding="utf-8")
        text = strip_comments(raw)
        class_ranges = find_class_ranges(text, relative, gaps)

        for class_range in class_ranges:
            class_key = class_range.name.lower()
            classes[class_key] = ClassMetadata(
                name=class_range.name,
                kind=class_range.kind,
                extension=extension,
                source=relative,
            )
            body = text[class_range.body_start : class_range.body_end]
            for match in FUNCTION_RE.finditer(body):
                params = tuple(parse_params(match.group("params"), relative, gaps))
                method = MethodMetadata(
                    class_name=class_range.name,
                    name=match.group("name"),
                    extension=extension,
                    source=relative,
                    return_type=normalize_type(match.group("return") or "mixed"),
                    params=params,
                    is_static=bool(match.group("static")),
                )
                methods[(class_key, method.name.lower())] = method
            for constant in parse_constants(body, relative, extension, class_range.name, gaps):
                constants[(class_key, constant.name)] = constant

        for match in FUNCTION_RE.finditer(text):
            if is_inside_ranges(match.start(), class_ranges):
                continue
            name = match.group("name").split("\\")[-1]
            params = tuple(parse_params(match.group("params"), relative, gaps))
            functions[name.lower()] = FunctionMetadata(
                name=name,
                extension=extension,
                source=relative,
                return_type=normalize_type(match.group("return") or "mixed"),
                params=params,
            )

        for constant in parse_top_level_constants(text, relative, extension, class_ranges, gaps):
            constants[(None, constant.name)] = constant

    return GeneratedMetadata(
        functions=tuple(functions[key] for key in sorted(functions)),
        classes=tuple(classes[key] for key in sorted(classes)),
        methods=tuple(methods[key] for key in sorted(methods)),
        constants=tuple(constants[key] for key in sorted(constants, key=constant_sort_key)),
        override_count=0,
        gaps=tuple(sorted(gaps)),
    )


def module_owner(relative: str) -> str:
    parts = relative.split("/")
    if parts[0] == "ext" and len(parts) > 1:
        return parts[1]
    if parts[0] == "Zend":
        return "core"
    if parts[0] == "main":
        return "core"
    return parts[0].lower()


def strip_comments(text: str) -> str:
    text = re.sub(r"/\*.*?\*/", "", text, flags=re.DOTALL)
    return re.sub(r"//.*", "", text)


def find_class_ranges(text: str, relative: str, gaps: set[str]) -> list[ClassRange]:
    ranges = []
    for match in CLASS_RE.finditer(text):
        open_brace = text.find("{", match.start(), match.end())
        close_brace = find_matching_brace(text, open_brace)
        if close_brace is None:
            gaps.add(f"{relative}: unmatched class body for {match.group('name')}")
            continue
        ranges.append(
            ClassRange(
                name=qualify_name(namespace_at(text, match.start()), match.group("name")),
                kind=match.group("kind"),
                body_start=open_brace + 1,
                body_end=close_brace,
                full_start=match.start(),
                full_end=close_brace + 1,
            )
        )
    return ranges


def namespace_at(text: str, position: int) -> str | None:
    namespace = None
    for match in NAMESPACE_RE.finditer(text):
        if match.start() >= position:
            break
        namespace = match.group("name").strip("\\")
    return namespace or None


def qualify_name(namespace: str | None, raw_name: str) -> str:
    name = raw_name.strip("\\")
    if "\\" in name or namespace is None:
        return name
    return f"{namespace}\\{name}"


def find_matching_brace(text: str, open_brace: int) -> int | None:
    depth = 0
    quote: str | None = None
    escaped = False
    for index in range(open_brace, len(text)):
        char = text[index]
        if quote:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == quote:
                quote = None
            continue
        if char in {'"', "'"}:
            quote = char
        elif char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                return index
    return None


def is_inside_ranges(position: int, ranges: list[ClassRange]) -> bool:
    return any(class_range.full_start <= position <= class_range.full_end for class_range in ranges)


def parse_params(raw: str, relative: str, gaps: set[str]) -> list[ParamMetadata]:
    if not raw.strip():
        return []
    params = []
    for chunk in split_top_level(raw, ","):
        if not chunk:
            continue
        chunk = strip_attributes(chunk.strip())
        declaration, default = split_default(chunk)
        match = PARAM_RE.match(declaration.strip())
        if not match:
            gaps.add(f"{relative}: unsupported parameter declaration `{chunk}`")
            continue
        type_decl = normalize_type(match.group("prefix") or "mixed")
        if not type_decl:
            type_decl = "mixed"
        params.append(
            ParamMetadata(
                name=match.group("name"),
                type_decl=type_decl,
                default_value=normalize_default(default),
                optional=default is not None,
                by_ref=bool(match.group("byref")),
                variadic=bool(match.group("variadic")),
            )
        )
    return params


def strip_attributes(value: str) -> str:
    while value.lstrip().startswith("#["):
        stripped = value.lstrip()
        end = stripped.find("]")
        if end < 0:
            return value
        value = stripped[end + 1 :].strip()
    return value


def split_default(value: str) -> tuple[str, str | None]:
    parts = split_top_level(value, "=", maxsplit=1)
    if len(parts) == 1:
        return parts[0], None
    return parts[0], parts[1].strip()


def split_top_level(raw: str, separator: str, maxsplit: int | None = None) -> list[str]:
    parts: list[str] = []
    depth = 0
    start = 0
    quote: str | None = None
    escaped = False
    splits = 0
    for index, char in enumerate(raw):
        if quote:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == quote:
                quote = None
            continue
        if char in {'"', "'"}:
            quote = char
        elif char in "([{":
            depth += 1
        elif char in ")]}":
            depth = max(0, depth - 1)
        elif char == separator and depth == 0:
            parts.append(raw[start:index].strip())
            start = index + 1
            splits += 1
            if maxsplit is not None and splits >= maxsplit:
                break
    parts.append(raw[start:].strip())
    return parts


def normalize_type(raw: str) -> str:
    cleaned = strip_attributes(raw).strip()
    if not cleaned:
        return ""
    cleaned = re.sub(r"\s+", " ", cleaned)
    cleaned = cleaned.replace(" | ", "|").replace(" & ", "&")
    cleaned = cleaned.replace(" ? ", "?")
    return cleaned


def normalize_default(raw: str | None) -> str | None:
    if raw is None:
        return None
    return " ".join(raw.strip().split())


def parse_top_level_constants(
    text: str,
    relative: str,
    extension: str,
    class_ranges: list[ClassRange],
    gaps: set[str],
) -> list[ConstantMetadata]:
    constants = []
    for match in re.finditer(r"\bconst\s+([^;]+);", text, re.DOTALL):
        if is_inside_ranges(match.start(), class_ranges):
            continue
        constant = parse_const_body(
            match.group(1), relative, extension, owner=None, gaps=gaps
        )
        if constant:
            constants.append(constant)
    return constants


def parse_constants(
    body: str,
    relative: str,
    extension: str,
    owner: str,
    gaps: set[str],
) -> list[ConstantMetadata]:
    constants = []
    for match in re.finditer(
        r"\b(?:(?:public|protected|private)\s+)?const\s+([^;]+);", body, re.DOTALL
    ):
        constant = parse_const_body(match.group(1), relative, extension, owner, gaps)
        if constant:
            constants.append(constant)
    return constants


def parse_const_body(
    body: str,
    relative: str,
    extension: str,
    owner: str | None,
    gaps: set[str],
) -> ConstantMetadata | None:
    parts = split_top_level(body, "=", maxsplit=1)
    if len(parts) != 2:
        gaps.add(f"{relative}: unsupported constant declaration `{body.strip()}`")
        return None
    left, value = parts
    tokens = left.split()
    if not tokens:
        gaps.add(f"{relative}: empty constant declaration")
        return None
    name = tokens[-1].split("\\")[-1]
    type_decl = normalize_type(" ".join(tokens[:-1])) or "mixed"
    return ConstantMetadata(
        owner=owner,
        name=name,
        extension=extension,
        source=relative,
        type_decl=type_decl,
        value=normalize_default(value) or "",
    )


def constant_sort_key(key: tuple[str | None, str]) -> tuple[str, str]:
    owner, name = key
    return (owner or "", name.lower())


def apply_overrides(
    metadata: GeneratedMetadata, overrides_path: Path
) -> GeneratedMetadata:
    functions = {function.name.lower(): function for function in metadata.functions}
    override_count = 0
    for line_number, raw in enumerate(
        overrides_path.read_text(encoding="utf-8").splitlines(), 1
    ):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        parts = dict(part.split("=", 1) for part in line.split() if "=" in part)
        name = parts.get("name", "").lower()
        reason = parts.get("reason", "").strip()
        if not reason:
            raise ValueError(f"{overrides_path}:{line_number}: override lacks reason=...")
        if name not in functions:
            raise ValueError(f"{overrides_path}:{line_number}: unknown function {name!r}")
        function = functions[name]
        if "return" in parts:
            function = replace(function, return_type=parts["return"])
        if "required" in parts:
            required = int(parts["required"])
            params = tuple(
                replace(param, optional=index >= required)
                for index, param in enumerate(function.params)
            )
            function = replace(function, params=params)
        functions[name] = function
        override_count += 1
    return replace(
        metadata,
        functions=tuple(functions[key] for key in sorted(functions)),
        override_count=override_count,
    )


def write_rust(path: Path, metadata: GeneratedMetadata) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    lines = [
        "// @generated by scripts/stdlib/generate_arginfo.py",
        "// Source: php-src *.stub.php metadata for PHP 8.5.7.",
        "// Attribution: derived from php-src stub declarations; no C code is copied.",
        "",
        "#![allow(clippy::too_many_lines)]",
        "",
        "#[derive(Clone, Copy, Debug, Eq, PartialEq)]",
        "pub struct GeneratedParamMetadata {",
        "    pub name: &'static str,",
        "    pub type_decl: &'static str,",
        "    pub default_value: Option<&'static str>,",
        "    pub optional: bool,",
        "    pub by_ref: bool,",
        "    pub variadic: bool,",
        "}",
        "",
        "#[derive(Clone, Copy, Debug, Eq, PartialEq)]",
        "pub struct GeneratedFunctionMetadata {",
        "    pub name: &'static str,",
        "    pub extension: &'static str,",
        "    pub source: &'static str,",
        "    pub return_type: &'static str,",
        "    pub params: &'static [GeneratedParamMetadata],",
        "}",
        "",
        "#[derive(Clone, Copy, Debug, Eq, PartialEq)]",
        "pub struct GeneratedClassMetadata {",
        "    pub name: &'static str,",
        "    pub kind: &'static str,",
        "    pub extension: &'static str,",
        "    pub source: &'static str,",
        "}",
        "",
        "#[derive(Clone, Copy, Debug, Eq, PartialEq)]",
        "pub struct GeneratedMethodMetadata {",
        "    pub class_name: &'static str,",
        "    pub name: &'static str,",
        "    pub extension: &'static str,",
        "    pub source: &'static str,",
        "    pub return_type: &'static str,",
        "    pub params: &'static [GeneratedParamMetadata],",
        "    pub is_static: bool,",
        "}",
        "",
        "#[derive(Clone, Copy, Debug, Eq, PartialEq)]",
        "pub struct GeneratedConstantMetadata {",
        "    pub owner: Option<&'static str>,",
        "    pub name: &'static str,",
        "    pub extension: &'static str,",
        "    pub source: &'static str,",
        "    pub type_decl: &'static str,",
        "    pub value: &'static str,",
        "}",
        "",
        f"pub const GENERATED_ARGINFO_FUNCTION_COUNT: usize = {len(metadata.functions)};",
        f"pub const GENERATED_ARGINFO_CLASS_COUNT: usize = {len(metadata.classes)};",
        f"pub const GENERATED_ARGINFO_METHOD_COUNT: usize = {len(metadata.methods)};",
        f"pub const GENERATED_ARGINFO_CONSTANT_COUNT: usize = {len(metadata.constants)};",
        f"pub const GENERATED_ARGINFO_OVERRIDE_COUNT: usize = {metadata.override_count};",
        "",
        "pub const GENERATED_ARGINFO_EXTRACTOR_GAPS: &[&str] = &[",
    ]
    for gap in metadata.gaps:
        lines.append(f'    "{rust_escape(gap)}",')
    lines.extend(
        [
            "];",
            "",
            "pub const GENERATED_FUNCTIONS: &[GeneratedFunctionMetadata] = &[",
        ]
    )
    for function in metadata.functions:
        lines.extend(write_function(function))
    lines.extend(["];", "", "pub const GENERATED_CLASSES: &[GeneratedClassMetadata] = &["])
    for class_meta in metadata.classes:
        lines.extend(
            [
                "    GeneratedClassMetadata {",
                f'        name: "{rust_escape(class_meta.name)}",',
                f'        kind: "{rust_escape(class_meta.kind)}",',
                f'        extension: "{rust_escape(class_meta.extension)}",',
                f'        source: "{rust_escape(class_meta.source)}",',
                "    },",
            ]
        )
    lines.extend(["];", "", "pub const GENERATED_METHODS: &[GeneratedMethodMetadata] = &["])
    for method in metadata.methods:
        lines.extend(write_method(method))
    lines.extend(
        ["];",
         "",
         "pub const GENERATED_CONSTANTS: &[GeneratedConstantMetadata] = &["]
    )
    for constant in metadata.constants:
        owner = (
            f'Some("{rust_escape(constant.owner)}")'
            if constant.owner is not None
            else "None"
        )
        lines.extend(
            [
                "    GeneratedConstantMetadata {",
                f"        owner: {owner},",
                f'        name: "{rust_escape(constant.name)}",',
                f'        extension: "{rust_escape(constant.extension)}",',
                f'        source: "{rust_escape(constant.source)}",',
                f'        type_decl: "{rust_escape(constant.type_decl)}",',
                f'        value: "{rust_escape(constant.value)}",',
                "    },",
            ]
        )
    lines.extend(
        [
            "];",
            "",
            "pub fn function_metadata(name: &str) -> Option<&'static GeneratedFunctionMetadata> {",
            "    GENERATED_FUNCTIONS",
            "        .iter()",
            "        .find(|function| function.name.eq_ignore_ascii_case(name))",
            "}",
            "",
            "pub fn class_metadata(name: &str) -> Option<&'static GeneratedClassMetadata> {",
            "    GENERATED_CLASSES",
            "        .iter()",
            "        .find(|class| class.name.eq_ignore_ascii_case(name))",
            "}",
            "",
            "pub fn method_metadata(",
            "    class_name: &str,",
            "    method_name: &str,",
            ") -> Option<&'static GeneratedMethodMetadata> {",
            "    GENERATED_METHODS.iter().find(|method| {",
            "        method.class_name.eq_ignore_ascii_case(class_name)",
            "            && method.name.eq_ignore_ascii_case(method_name)",
            "    })",
            "}",
            "",
            "pub fn class_methods(class_name: &str) -> Vec<&'static GeneratedMethodMetadata> {",
            "    GENERATED_METHODS",
            "        .iter()",
            "        .filter(|method| method.class_name.eq_ignore_ascii_case(class_name))",
            "        .collect()",
            "}",
            "",
            "pub fn constant_metadata(",
            "    owner: Option<&str>,",
            "    name: &str,",
            ") -> Option<&'static GeneratedConstantMetadata> {",
            "    GENERATED_CONSTANTS.iter().find(|constant| {",
            "        owner.map_or(constant.owner.is_none(), |owner| {",
            "            constant",
            "                .owner",
            "                .is_some_and(|constant_owner| constant_owner.eq_ignore_ascii_case(owner))",
            "        }) && constant.name == name",
            "    })",
            "}",
        ]
    )
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def write_function(function: FunctionMetadata) -> list[str]:
    return [
        "    GeneratedFunctionMetadata {",
        f'        name: "{rust_escape(function.name)}",',
        f'        extension: "{rust_escape(function.extension)}",',
        f'        source: "{rust_escape(function.source)}",',
        f'        return_type: "{rust_escape(function.return_type)}",',
        "        params: &[",
        *write_params(function.params),
        "        ],",
        "    },",
    ]


def write_method(method: MethodMetadata) -> list[str]:
    return [
        "    GeneratedMethodMetadata {",
        f'        class_name: "{rust_escape(method.class_name)}",',
        f'        name: "{rust_escape(method.name)}",',
        f'        extension: "{rust_escape(method.extension)}",',
        f'        source: "{rust_escape(method.source)}",',
        f'        return_type: "{rust_escape(method.return_type)}",',
        "        params: &[",
        *write_params(method.params),
        "        ],",
        f"        is_static: {str(method.is_static).lower()},",
        "    },",
    ]


def write_params(params: tuple[ParamMetadata, ...]) -> list[str]:
    lines = []
    for param in params:
        default = (
            f'Some("{rust_escape(param.default_value)}")'
            if param.default_value is not None
            else "None"
        )
        lines.extend(
            [
                "            GeneratedParamMetadata {",
                f'                name: "{rust_escape(param.name)}",',
                f'                type_decl: "{rust_escape(param.type_decl)}",',
                f"                default_value: {default},",
                f"                optional: {str(param.optional).lower()},",
                f"                by_ref: {str(param.by_ref).lower()},",
                f"                variadic: {str(param.variadic).lower()},",
                "            },",
            ]
        )
    return lines


def rust_escape(value: str | None) -> str:
    if value is None:
        return ""
    return (
        value.replace("\\", "\\\\")
        .replace('"', '\\"')
        .replace("\n", "\\n")
        .replace("\r", "\\r")
        .replace("\t", "\\t")
    )


if __name__ == "__main__":
    raise SystemExit(main())
