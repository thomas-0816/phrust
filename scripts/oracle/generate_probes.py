#!/usr/bin/env python3
"""Generate bounded executable oracle probes from the API oracle index."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
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

    @property
    def probe_id(self) -> str:
        digest = hashlib.sha1(
            f"{self.area}:{self.kind}:{self.symbol}:{self.source}".encode("utf-8")
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
    return dedupe_probes(probes)


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
            }
        )
    with manifest.open("w", encoding="utf-8") as handle:
        for row in sorted(manifest_rows, key=lambda item: item["id"]):
            handle.write(json.dumps(row, sort_keys=True, separators=(",", ":")) + "\n")


def render_probe(probe: Probe) -> str:
    known_gap = f" known_gap={probe.known_gap}" if probe.known_gap else ""
    return (
        "<?php\n"
        f"// oracle-probe: id={probe.probe_id} area={probe.area} kind={probe.kind} "
        f"symbol={probe.symbol} source={probe.source} expect={probe.expect}\n"
        f"// runtime-semantics: category=oracle_generated expect={probe.expect} "
        f"php_ref_required=1{known_gap} oracle_probe_id={probe.probe_id} "
        f"failure_category={probe.area}\n"
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
            "source": "fixture.stub.php",
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
        }
    ]
    first = [probe.probe_id for probe in build_probes(rows)]
    second = [probe.probe_id for probe in build_probes(rows)]
    if first != second:
        raise AssertionError("probe IDs are not stable across runs")
    by_ref = [probe for probe in build_probes(rows) if probe.area == "reflection" and probe.kind == "parameter"]
    if not by_ref or "isPassedByReference" not in by_ref[0].body:
        raise AssertionError("by-reference reflection probe was not generated")

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
