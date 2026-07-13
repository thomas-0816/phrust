#!/usr/bin/env python3
"""Validate canonical extension ownership against arginfo and runtime mappings."""

from __future__ import annotations

import json
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
SCHEMA_DIR = ROOT / "fixtures/stdlib/extensions"
ARGINFO = ROOT / "crates/php_std/src/generated/arginfo.rs"
REPORT_DIR = ROOT / "target/stdlib/registry-drift"
REPORT_JSON = REPORT_DIR / "report.json"
REPORT_MD = REPORT_DIR / "report.md"

sys.path.insert(0, str(ROOT / "scripts/stdlib"))
import generate_extension_surfaces as surfaces  # noqa: E402


def main() -> int:
    try:
        index, descriptors = surfaces.load_descriptors(SCHEMA_DIR)
        arginfo = surfaces.load_arginfo(ARGINFO)
        surfaces.validate_arginfo(descriptors, arginfo)
        surfaces.validate_runtime_mappings(descriptors)
        if (ROOT / "crates/php_std/src/extensions.rs").exists():
            raise surfaces.DescriptorError(
                "legacy php_std/extensions.rs must not reintroduce a second metadata owner"
            )
    except (OSError, surfaces.DescriptorError) as error:
        print(f"[fail] stdlib registry drift: {error}", file=sys.stderr)
        return 1

    functions = [
        (descriptor["name"], function)
        for descriptor in descriptors
        for function in descriptor["functions"]
    ]
    classes = [item for descriptor in descriptors for item in descriptor["classes"]]
    constants = [item for descriptor in descriptors for item in descriptor["constants"]]
    gaps = [
        {"extension": extension, "name": function["name"], "reason": function["signature_gap"]}
        for extension, function in functions
        if "signature_gap" in function
    ]
    payload = {
        "schema_version": index["schema_version"],
        "extension_count": len(descriptors),
        "function_count": len(functions),
        "class_count": len(classes),
        "constant_count": len(constants),
        "runtime_mapping_count": sum(
            implementation["kind"] in {"runtime", "extension"}
            for _, function in functions
            for implementation in function["implementations"]
        ),
        "vm_mapping_count": sum(
            implementation["kind"] == "vm"
            for _, function in functions
            for implementation in function["implementations"]
        ),
        "signature_gaps": gaps,
        "drift": [],
    }
    REPORT_DIR.mkdir(parents=True, exist_ok=True)
    REPORT_JSON.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    REPORT_MD.write_text(render_markdown(payload), encoding="utf-8")
    print(f"[ok] stdlib registry drift report written to {REPORT_MD.relative_to(ROOT)}")
    return 0


def render_markdown(payload: dict) -> str:
    return "\n".join(
        [
            "# Canonical Extension Registry",
            "",
            f"- Extensions: {payload['extension_count']}",
            f"- Functions: {payload['function_count']}",
            f"- Classes: {payload['class_count']}",
            f"- Constants: {payload['constant_count']}",
            f"- Explicit runtime mappings: {payload['runtime_mapping_count']}",
            f"- VM-mediated mappings: {payload['vm_mapping_count']}",
            f"- Reviewed signature gaps: {len(payload['signature_gaps'])}",
            "- Metadata/runtime drift: 0",
            "",
        ]
    )


if __name__ == "__main__":
    raise SystemExit(main())
