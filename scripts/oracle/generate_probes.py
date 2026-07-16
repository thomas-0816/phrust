#!/usr/bin/env python3
"""Generate bounded executable oracle probes from the API oracle index."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable


REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_API = REPO_ROOT / "target/oracle/api/php-source-api-symbols.jsonl"
DEFAULT_OUT = REPO_ROOT / "fixtures/runtime_semantics/oracle_generated"
DEFAULT_MANIFEST = REPO_ROOT / "tests/oracle/manifests/generated-probes.jsonl"
SMOKE_AREAS = {
    "api_surface",
    "reflection",
    "callable_dispatch",
    "reference_binding",
    "name_resolution",
    "frontend_lowering",
}
EXTERNAL_EXTENSION_CLASSES = {
    "curl": "network",
    "ftp": "network",
    "imap": "network",
    "ldap": "network",
    "mysqli": "database",
    "openssl": "crypto_certificates",
    "pcntl": "process_ipc",
    "pdo": "database",
    "pdo_mysql": "database",
    "pdo_pgsql": "database",
    "pgsql": "database",
    "readline": "process_ipc",
    "sockets": "network",
    "ssh2": "network",
    "sysvmsg": "process_ipc",
    "sysvsem": "process_ipc",
    "sysvshm": "process_ipc",
}
HERMETIC_EXTENSIONS = {
    "bcmath",
    "calendar",
    "ctype",
    "filter",
    "gmp",
    "hash",
    "iconv",
    "json",
    "mbstring",
    "pcre",
    "sodium",
    "tokenizer",
}
HERMETIC_STANDARD_SOURCE_MARKERS = {
    "array",
    "base64",
    "crc32",
    "html",
    "math",
    "quot_print",
    "string",
    "type",
    "url",
    "var",
}


@dataclass(frozen=True)
class Probe:
    area: str
    kind: str
    symbol: str
    source: str
    body: str
    selection: str = "smoke"
    expect: str = "pass"
    known_gap: str | None = None
    probe_case: str = "seed"
    support_evidence: bool = True
    environmental_class: str | None = None
    required_reference_extension: str | None = None

    @property
    def probe_id(self) -> str:
        digest = hashlib.sha1(
            f"{self.area}:{self.kind}:{self.symbol}:{self.source}:{self.probe_case}".encode("utf-8")
        ).hexdigest()[:10]
        return f"oracle-{slug(self.area)}-{slug(self.kind)}-{slug(self.symbol)}-{digest}"

    @property
    def filename(self) -> str:
        return f"{self.probe_id}.php"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--api", type=Path, default=DEFAULT_API)
    parser.add_argument("--out", type=Path, default=DEFAULT_OUT)
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--self-test-only", action="store_true")
    args = parser.parse_args()

    try:
        if args.self_test or args.self_test_only:
            run_self_tests()
        if args.self_test_only:
            return 0

        rows = load_api_rows(args.api)
        probes = build_probes(rows)
        write_probes(probes, args.out, args.manifest)
        lint_with_reference(probes, args.out)
    except Exception as error:  # noqa: BLE001 - script boundary.
        print(f"oracle probe generation error: {error}", file=sys.stderr)
        return 1

    print(f"[ok] wrote {len(probes)} oracle probes under {relative(args.out)}")
    print(f"[ok] wrote {relative(args.manifest)}")
    return 0


def load_api_rows(path: Path) -> list[dict]:
    if not path.is_file():
        return []
    rows = []
    for line_number, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not raw.strip():
            continue
        try:
            rows.append(json.loads(raw))
        except json.JSONDecodeError as error:
            raise ValueError(f"{path}:{line_number}: invalid JSONL: {error}") from error
    return rows


def build_probes(rows: list[dict]) -> list[Probe]:
    symbols = index_symbols(rows)
    probes = [
        Probe(
            area="api_surface",
            kind="function",
            symbol="strlen",
            source=symbols.get("function:strlen", "php-src"),
            body='echo function_exists("strlen") ? "function\\n" : "missing\\n";',
        ),
        Probe(
            area="api_surface",
            kind="class",
            symbol="Exception",
            source=symbols.get("class:exception", "reference-php"),
            body='echo class_exists("Exception") ? "class\\n" : "missing\\n";',
        ),
        Probe(
            area="api_surface",
            kind="constant",
            symbol="PHP_VERSION",
            source=symbols.get("constant:php_version", "reference-php"),
            body='echo defined("PHP_VERSION") ? "constant\\n" : "missing\\n";',
        ),
        Probe(
            area="reflection",
            kind="function",
            symbol="strlen",
            source=symbols.get("function:strlen", "php-src"),
            body=(
                '$ref = new ReflectionFunction("strlen");\n'
                'echo $ref->getName(), ":", $ref->getNumberOfParameters(), "\\n";\n'
                '$param = $ref->getParameters()[0];\n'
                'echo $param->getName(), ":", $param->isOptional() ? "optional" : "required", "\\n";'
            ),
        ),
        Probe(
            area="reflection",
            kind="parameter",
            symbol=first_by_ref_symbol(rows),
            source="php-src",
            body=by_ref_reflection_body(first_by_ref_symbol(rows)),
        ),
        Probe(
            area="callable_dispatch",
            kind="function",
            symbol="variable-function",
            source="seed",
            body='$fn = "strlen"; echo $fn("abcd"), "\\n";',
        ),
        Probe(
            area="callable_dispatch",
            kind="function",
            symbol="call-user-func-array",
            source="seed",
            body='echo call_user_func_array("strlen", ["abc"]), "\\n";',
        ),
        Probe(
            area="reference_binding",
            kind="reference",
            symbol="local-variable",
            source="seed",
            body='$a = 1; $b =& $a; $b = 4; echo $a, ":", $b, "\\n";',
        ),
        Probe(
            area="reference_binding",
            kind="reference",
            symbol="array-dimension",
            source="seed",
            body='$items = [1]; $alias =& $items[0]; $alias = 9; echo $items[0], "\\n";',
        ),
        Probe(
            area="reference_binding",
            kind="reference",
            symbol="object-property",
            source="seed",
            body=(
                'class OracleReferenceBox { public int $value = 1; }\n'
                '$box = new OracleReferenceBox();\n'
                '$alias =& $box->value;\n'
                '$alias = 8;\n'
                'echo $box->value, "\\n";'
            ),
        ),
        Probe(
            area="name_resolution",
            kind="class_constant",
            symbol="imported-class-constant",
            source="seed",
            body=(
                'namespace OracleProbe\\Lib { class Box { public const VALUE = "ok"; } }\n'
                'namespace OracleProbe\\App { use OracleProbe\\Lib\\Box; echo Box::VALUE, "\\n"; }'
            ),
        ),
        Probe(
            area="frontend_lowering",
            kind="destructuring",
            symbol="list-holes",
            source="seed",
            body='[$first, , $third] = [1, 2, 3]; echo $first, ":", $third, "\\n";',
        ),
        Probe(
            area="frontend_lowering",
            kind="interpolation",
            symbol="property-dimension",
            source="seed",
            body=(
                'class OracleInterpolationBox { public array $items = ["k" => "v"]; }\n'
                '$box = new OracleInterpolationBox();\n'
                'echo "{$box->items[\'k\']}\\n";'
            ),
        ),
        Probe(
            area="frontend_lowering",
            kind="dynamic_static_method",
            symbol="class-variable-method",
            source="seed",
            body=(
                'class OracleStaticCallBox { public static function label(): string { return "ok"; } }\n'
                '$class = OracleStaticCallBox::class;\n'
                '$method = "label";\n'
                'echo $class::$method(), "\\n";'
            ),
        ),
    ]
    probes.extend(full_only_probes(rows))
    probes.extend(builtin_function_probes(rows))
    probes.extend(internal_descriptor_probes(rows))
    return dedupe_probes(probes)


def internal_descriptor_probes(rows: list[dict]) -> list[Probe]:
    """Classify every target-registered internal class descriptor member."""
    probes: list[Probe] = []
    class_kinds = {"class", "interface", "trait", "enum"}
    member_kinds = {"method", "property", "class_constant"}
    for row in rows:
        kind = str(row.get("kind") or "")
        rust = row.get("rust_registry") or {}
        if kind in class_kinds:
            if not rust.get("class_registered"):
                continue
            class_name = str(row["name"])
            symbol = class_name
            body = (
                f"$class = {php_string(class_name)};\n"
                "$available = class_exists($class, false) "
                "|| interface_exists($class, false) "
                "|| trait_exists($class, false) "
                "|| (function_exists('enum_exists') && enum_exists($class, false));\n"
                'echo $available ? "available\\n" : "missing\\n";'
            )
        elif kind in member_kinds:
            if not rust.get("class_registered"):
                continue
            class_name = str(row.get("class") or row.get("owner") or "")
            if not class_name:
                continue
            name = str(row["name"])
            symbol = f"{class_name}::{name}"
            if kind == "method":
                predicate = "method_exists($class, $member)"
            elif kind == "property":
                predicate = "(new ReflectionClass($class))->hasProperty($member)"
            else:
                predicate = "defined($class . '::' . $member)"
            body = (
                f"$class = {php_string(class_name)};\n"
                f"$member = {php_string(name)};\n"
                "$classAvailable = class_exists($class, false) "
                "|| interface_exists($class, false) "
                "|| trait_exists($class, false) "
                "|| (function_exists('enum_exists') && enum_exists($class, false));\n"
                f'$available = $classAvailable && {predicate};\n'
                'echo $available ? "available\\n" : "missing\\n";'
            )
        else:
            continue
        extension = str(row.get("extension") or "core").lower()
        probes.append(
            Probe(
                area="internal_api_contract",
                kind=kind,
                symbol=symbol,
                source=str(row.get("source") or "php-src"),
                selection="full",
                probe_case="descriptor-availability",
                support_evidence=False,
                environmental_class=EXTERNAL_EXTENSION_CLASSES.get(extension),
                required_reference_extension=required_reference_extension(row),
                body=body,
            )
        )
    return probes


def builtin_function_probes(rows: list[dict]) -> list[Probe]:
    """Generate a bounded, metadata-derived probe set for each runtime builtin."""
    probes: list[Probe] = []
    for row in rows:
        if row.get("kind") != "function":
            continue
        rust = row.get("rust_registry") or {}
        runtime_builtin = bool(rust.get("runtime_builtin"))
        if not runtime_builtin and not rust.get("present"):
            continue
        name = str(row["name"])
        extension = str(row.get("extension") or "core").lower()
        source = str(row.get("source") or "php-src")
        signature = row.get("signature") or {}
        params = signature.get("parameters") or []
        environmental_class = EXTERNAL_EXTENSION_CLASSES.get(extension)
        required_reference_extension = (
            None if extension in {"core", "standard"} else extension
        )
        probes.append(
            Probe(
                area="builtin_contract",
                kind="function",
                symbol=name,
                source=source,
                selection="full",
                probe_case="availability",
                support_evidence=False,
                environmental_class=environmental_class,
                required_reference_extension=required_reference_extension,
                body=(
                    f'$name = {php_string(name)};\n'
                    'echo function_exists($name) ? "available\\n" : "missing\\n";'
                ),
            )
        )
        if not runtime_builtin:
            continue
        contract_body = invalid_contract_body(name, params)
        if contract_body is not None:
            probes.append(
                Probe(
                    area="builtin_contract",
                    kind="function",
                    symbol=name,
                    source=source,
                    selection="full",
                    probe_case="binder-diagnostic",
                    support_evidence=False,
                    environmental_class=environmental_class,
                    required_reference_extension=required_reference_extension,
                    body=contract_body,
                )
            )
        if environmental_class or not hermetic_function(row):
            continue
        required = [param for param in params if not param.get("optional") and not param.get("variadic")]
        if all(sample_expression(param.get("type")) is not None for param in required):
            probes.append(
                call_probe(
                    row,
                    probe_case="required-defaults",
                    parameters=required,
                    named=False,
                )
            )
            optional = next(
                (
                    param
                    for param in params
                    if param.get("optional")
                    and not param.get("variadic")
                    and sample_expression(param.get("type")) is not None
                ),
                None,
            )
            if optional is not None:
                probes.append(
                    call_probe(
                        row,
                        probe_case="optional-explicit",
                        parameters=[*required, optional],
                        named=False,
                    )
                )
            if required:
                probes.append(
                    call_probe(
                        row,
                        probe_case="named-required",
                        parameters=required,
                        named=True,
                    )
                )
        variadic = next((param for param in params if param.get("variadic")), None)
        if variadic is not None and sample_expression(variadic.get("type")) is not None:
            arguments = [*required, variadic]
            if all(sample_expression(param.get("type")) is not None for param in arguments):
                probes.append(
                    call_probe(
                        row,
                        probe_case="variadic",
                        parameters=arguments,
                        named=False,
                    )
                )
        typed = next((param for param in required if invalid_expression(param.get("type")) is not None), None)
        if typed is not None:
            probes.append(invalid_type_probe(row, typed))
    return probes


def invalid_contract_body(name: str, params: list[dict]) -> str | None:
    required = [param for param in params if not param.get("optional") and not param.get("variadic")]
    if required:
        call = f"\\{name}()"
    elif not any(param.get("variadic") for param in params):
        call = f"\\{name}(__phrust_probe_unknown: 1)"
    else:
        return None
    return caught_call_body(call, [])


def hermetic_function(row: dict) -> bool:
    extension = str(row.get("extension") or "core").lower()
    if extension in HERMETIC_EXTENSIONS:
        return True
    if extension != "standard":
        return False
    source_name = Path(str(row.get("source") or "")).stem.lower()
    return any(marker in source_name for marker in HERMETIC_STANDARD_SOURCE_MARKERS)


def call_probe(
    row: dict,
    *,
    probe_case: str,
    parameters: list[dict],
    named: bool,
) -> Probe:
    declarations: list[str] = []
    arguments: list[str] = []
    reference_variables: list[str] = []
    for index, param in enumerate(parameters):
        expression = sample_expression(param.get("type"))
        if expression is None:
            raise ValueError(f"no sample expression for {param.get('type')!r}")
        if param.get("by_ref"):
            variable = f"$arg{index}"
            declarations.append(f"{variable} = {expression};")
            expression = variable
            reference_variables.append(variable)
        if named and not param.get("variadic"):
            expression = f"{param['name']}: {expression}"
        arguments.append(expression)
    name = str(row["name"])
    call = f"\\{name}({', '.join(arguments)})"
    return Probe(
        area="builtin_behavior",
        kind="function",
        symbol=name,
        source=str(row.get("source") or "php-src"),
        selection="full",
        probe_case=probe_case,
        support_evidence=True,
        required_reference_extension=required_reference_extension(row),
        body=caught_call_body(call, declarations, reference_variables),
    )


def invalid_type_probe(row: dict, target: dict) -> Probe:
    params = (row.get("signature") or {}).get("parameters") or []
    required = [param for param in params if not param.get("optional") and not param.get("variadic")]
    arguments = []
    declarations = []
    for index, param in enumerate(required):
        expression = invalid_expression(param.get("type")) if param is target else sample_expression(param.get("type"))
        if expression is None:
            expression = "null"
        if param.get("by_ref"):
            variable = f"$arg{index}"
            declarations.append(f"{variable} = {expression};")
            expression = variable
        arguments.append(expression)
    name = str(row["name"])
    return Probe(
        area="builtin_behavior",
        kind="function",
        symbol=name,
        source=str(row.get("source") or "php-src"),
        selection="full",
        probe_case=f"invalid-type-{target.get('name', 'argument')}",
        support_evidence=True,
        required_reference_extension=required_reference_extension(row),
        body=caught_call_body(f"\\{name}({', '.join(arguments)})", declarations),
    )


def caught_call_body(
    call: str, declarations: list[str], reference_variables: list[str] | None = None
) -> str:
    lines = [
        *declarations,
        "try {",
        f"    $result = {call};",
        '    echo "return:\\n";',
        "    var_dump($result);",
    ]
    for variable in reference_variables or []:
        lines.extend(['    echo "writeback:\\n";', f"    var_dump({variable});"])
    lines.extend(
        [
            "} catch (Throwable $error) {",
            '    echo "throw:", get_class($error), ":", $error->getMessage(), "\\n";',
            "}",
        ]
    )
    return "\n".join(lines)


def sample_expression(type_decl: object) -> str | None:
    types = normalized_types(type_decl)
    samples = {
        "array": "[]",
        "bool": "false",
        "boolean": "false",
        "callable": '"strlen"',
        "false": "false",
        "float": "0.0",
        "int": "0",
        "integer": "0",
        "iterable": "[]",
        "mixed": "null",
        "null": "null",
        "object": "(object)[]",
        "resource": 'fopen("php://memory", "r+")',
        "string": '""',
        "true": "true",
    }
    for type_name in types:
        if type_name in samples:
            return samples[type_name]
    return None


def invalid_expression(type_decl: object) -> str | None:
    types = set(normalized_types(type_decl))
    if not types or "mixed" in types:
        return None
    if "array" not in types and "iterable" not in types and "object" not in types:
        return "[]"
    if "string" not in types:
        return '"phrust-invalid-type"'
    return None


def normalized_types(type_decl: object) -> list[str]:
    value = str(type_decl or "mixed").lower().replace("?", "null|")
    value = value.replace("(", "").replace(")", "")
    return [part.strip().lstrip("\\") for part in re.split(r"[|&]", value) if part.strip()]


def required_reference_extension(row: dict) -> str | None:
    extension = str(row.get("extension") or "core").lower()
    return None if extension in {"core", "standard"} else extension


def php_string(value: str) -> str:
    return json.dumps(value, ensure_ascii=False)


def full_only_probes(rows: list[dict]) -> list[Probe]:
    return [
        Probe(
            area="api_surface",
            kind="extension",
            symbol="standard",
            source="reference-php",
            selection="full",
            body='echo extension_loaded("standard") ? "extension\\n" : "missing\\n";',
        ),
        Probe(
            area="api_surface",
            kind="defined-functions",
            symbol="strlen",
            source="reference-php",
            selection="full",
            body=(
                '$functions = get_defined_functions()["internal"];\n'
                'echo in_array("strlen", $functions, true) ? "listed\\n" : "missing\\n";'
            ),
        ),
        Probe(
            area="callable_dispatch",
            kind="method",
            symbol="dynamic-class-variable-static-call",
            source="seed",
            selection="full",
            body=(
                'class OracleCallableBox { public static function wrap($value) { return "[" . $value . "]"; } }\n'
                '$class = OracleCallableBox::class;\n'
                '$callable = [$class, "wrap"];\n'
                'echo $callable("x"), "\\n";'
            ),
        ),
        Probe(
            area="reference_binding",
            kind="callback",
            symbol="callback-requires-reference",
            source="seed",
            selection="full",
            body=(
                'function oracle_probe_mutate(&$value) { $value = $value + 1; }\n'
                '$value = 1;\n'
                'call_user_func_array("oracle_probe_mutate", [&$value]);\n'
                'echo $value, "\\n";'
            ),
        ),
        Probe(
            area="reference_binding",
            kind="callback",
            symbol="callback-non-lvalue-negative",
            source="seed",
            selection="full",
            body=(
                'function oracle_probe_needs_ref(&$value) { echo "called\\n"; }\n'
                'try {\n'
                '    call_user_func_array("oracle_probe_needs_ref", [1]);\n'
                '} catch (Throwable $error) {\n'
                '    echo get_class($error), "\\n";\n'
                '}'
            ),
        ),
        Probe(
            area="frontend_lowering",
            kind="destructuring",
            symbol="nested-keyed",
            source="seed",
            selection="full",
            body='$row = ["a" => [1, 2], "b" => 3]; ["a" => [$x, $y], "b" => $z] = $row; echo $x, ":", $y, ":", $z, "\\n";',
        ),
        Probe(
            area="name_resolution",
            kind="array-key",
            symbol="class-constant-key",
            source="seed",
            selection="full",
            body='class OracleKeyBox { public const KEY = "answer"; } $items = [OracleKeyBox::KEY => 42]; echo $items["answer"], "\\n";',
        ),
    ]


def index_symbols(rows: list[dict]) -> dict[str, str]:
    symbols = {}
    for row in rows:
        key = f"{row.get('kind')}:{str(row.get('name', '')).lower()}"
        symbols.setdefault(key, row.get("source") or row.get("provenance", ["oracle"])[0])
    return symbols


def first_by_ref_symbol(rows: list[dict]) -> str:
    for row in rows:
        if row.get("kind") != "function" or row.get("status") != "matched":
            continue
        rust = row.get("rust_registry") or {}
        if not rust.get("runtime_builtin"):
            continue
        for param in (row.get("signature") or {}).get("parameters", []):
            if param.get("by_ref"):
                return row["name"]
    for row in rows:
        if row.get("kind") != "function":
            continue
        for param in (row.get("signature") or {}).get("parameters", []):
            if param.get("by_ref"):
                return row["name"]
    return "array_pop"


def by_ref_reflection_body(symbol: str) -> str:
    return (
        f'$ref = new ReflectionFunction("{symbol}");\n'
        '$found = "none";\n'
        'foreach ($ref->getParameters() as $param) {\n'
        '    if ($param->isPassedByReference()) { $found = $param->getName(); break; }\n'
        '}\n'
        'echo $ref->getName(), ":", $found, "\\n";'
    )


def dedupe_probes(probes: Iterable[Probe]) -> list[Probe]:
    by_id = {}
    for probe in probes:
        by_id[probe.probe_id] = probe
    return [by_id[key] for key in sorted(by_id)]


def write_probes(probes: list[Probe], out_dir: Path, manifest: Path) -> None:
    if out_dir.exists():
        shutil.rmtree(out_dir)
    (out_dir / "smoke").mkdir(parents=True)
    (out_dir / "full").mkdir(parents=True)
    manifest.parent.mkdir(parents=True, exist_ok=True)

    manifest_rows = []
    for probe in probes:
        directory = out_dir / probe.selection
        path = directory / probe.filename
        path.write_text(render_probe(probe), encoding="utf-8")
        manifest_rows.append(
            {
                "id": probe.probe_id,
                "path": relative(path),
                "selection": probe.selection,
                "area": probe.area,
                "kind": probe.kind,
                "symbol": probe.symbol,
                "source": probe.source,
                "expect": probe.expect,
                "known_gap": probe.known_gap,
                "probe_case": probe.probe_case,
                "support_evidence": probe.support_evidence,
                "environmental_class": probe.environmental_class,
                "required_reference_extension": probe.required_reference_extension,
            }
        )
    with manifest.open("w", encoding="utf-8") as handle:
        for row in sorted(manifest_rows, key=lambda item: item["id"]):
            handle.write(json.dumps(row, sort_keys=True, separators=(",", ":")) + "\n")


def render_probe(probe: Probe) -> str:
    known_gap = f" known_gap={probe.known_gap}" if probe.known_gap else ""
    required_extension = (
        f" requires_ref_extension={probe.required_reference_extension}"
        if probe.required_reference_extension
        else ""
    )
    reference_contract = (
        "php_ref_required=0 php_ref_optional_reason=missing_reference_extension"
        if probe.required_reference_extension
        else "php_ref_required=1"
    )
    return (
        "<?php\n"
        f"// oracle-probe: id={probe.probe_id} area={probe.area} kind={probe.kind} "
        f"symbol={probe.symbol} source={probe.source} expect={probe.expect}\n"
        f"// runtime-semantics: category=oracle_generated expect={probe.expect} "
        f"{reference_contract}{known_gap} oracle_probe_id={probe.probe_id} "
        f"failure_category={probe.area}{required_extension}\n"
        f"{probe.body}\n"
    )


def lint_with_reference(probes: list[Probe], out_dir: Path) -> None:
    reference = resolve_reference_php()
    if reference is None:
        print("[skip] REFERENCE_PHP unavailable; generated probe syntax lint skipped")
        return
    for probe in probes:
        path = out_dir / probe.selection / probe.filename
        completed = subprocess.run(
            [str(reference), "-l", str(path)],
            cwd=REPO_ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
        if completed.returncode != 0:
            raise ValueError(f"generated probe does not pass php -l: {relative(path)}\n{completed.stderr}")


def resolve_reference_php() -> Path | None:
    candidates = []
    if os.environ.get("REFERENCE_PHP"):
        candidates.append(Path(os.environ["REFERENCE_PHP"]))
    candidates.extend(
        [
            REPO_ROOT / "third_party/php-src/sapi/cli/php",
            Path("/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php"),
        ]
    )
    for candidate in candidates:
        if candidate.is_file():
            return candidate.resolve()
    return None


def run_self_tests() -> None:
    rows = [
        {
            "kind": "function",
            "name": "array_pop",
            "extension": "json",
            "source": "fixture.stub.php",
            "rust_registry": {"runtime_builtin": True},
            "signature": {
                "parameters": [
                    {
                        "name": "array",
                        "type": "array",
                        "default": None,
                        "optional": False,
                        "by_ref": True,
                        "variadic": False,
                    }
                ],
                "return_type": "mixed",
            },
        },
        {
            "kind": "method",
            "class": "ArrayObject",
            "name": "count",
            "extension": "spl",
            "source": "fixture.stub.php",
            "rust_registry": {"class_registered": True, "present": True},
            "signature": {"parameters": [], "return_type": "int"},
        },
    ]
    first = [probe.probe_id for probe in build_probes(rows)]
    second = [probe.probe_id for probe in build_probes(rows)]
    if first != second:
        raise AssertionError("probe IDs are not stable across runs")
    by_ref = [probe for probe in build_probes(rows) if probe.area == "reflection" and probe.kind == "parameter"]
    if not by_ref or "isPassedByReference" not in by_ref[0].body:
        raise AssertionError("by-reference reflection probe was not generated")
    builtin = [probe for probe in build_probes(rows) if probe.area.startswith("builtin_")]
    cases = {probe.probe_case for probe in builtin}
    if not {"availability", "binder-diagnostic", "required-defaults"}.issubset(cases):
        raise AssertionError(f"builtin contract cases missing: {sorted(cases)}")
    methods = [
        probe
        for probe in build_probes(rows)
        if probe.area == "internal_api_contract" and probe.kind == "method"
    ]
    if len(methods) != 1 or "method_exists" not in methods[0].body:
        raise AssertionError("registered method descriptor probe was not generated exactly once")

    reference = resolve_reference_php()
    if reference is None:
        return
    with tempfile.TemporaryDirectory() as temp_dir:
        out = Path(temp_dir) / "oracle_generated"
        manifest = Path(temp_dir) / "manifest.jsonl"
        probes = build_probes(rows)
        write_probes(probes, out, manifest)
        lint_with_reference(probes, out)


def slug(value: str) -> str:
    output = []
    for char in value.lower():
        if char.isalnum():
            output.append(char)
        elif output and output[-1] != "-":
            output.append("-")
    return "".join(output).strip("-") or "symbol"


def relative(path: Path) -> str:
    try:
        return path.relative_to(REPO_ROOT).as_posix()
    except ValueError:
        return str(path)


if __name__ == "__main__":
    raise SystemExit(main())
