#!/usr/bin/env python3
"""Focused failure and determinism tests for extension surface generation."""

from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

import generate_extension_surfaces as surfaces


def descriptor(name: str, function_name: str = "probe") -> dict:
    return {
        "schema_version": 1,
        "name": name,
        "version": "8.5.7",
        "enabled_by_default": True,
        "dependencies": [],
        "capabilities": [],
        "state_slot": None,
        "functions": [
            {
                "name": function_name,
                "visibility": "php",
                "implementations": [{"kind": "vm"}],
                "signature_gap": "test fixture",
            }
        ],
        "classes": [],
        "constants": [],
    }


class ExtensionSurfaceGeneratorTests(unittest.TestCase):
    def test_generation_is_deterministic(self) -> None:
        index = {
            "schema_version": 1,
            "php_version": "8.5.7",
            "extensions": ["test"],
            "runtime_module_order": [],
        }
        descriptors = [descriptor("test")]
        surfaces.validate_descriptors(index, descriptors)
        with tempfile.TemporaryDirectory() as first, tempfile.TemporaryDirectory() as second:
            surfaces.generate(index, descriptors, {}, Path(first))
            surfaces.generate(index, descriptors, {}, Path(second))
            self.assertEqual(tree(Path(first)), tree(Path(second)))

    def test_duplicate_function_owner_fails(self) -> None:
        index = {
            "schema_version": 1,
            "extensions": ["first", "second"],
        }
        with self.assertRaisesRegex(surfaces.DescriptorError, "duplicate function"):
            surfaces.validate_descriptors(
                index, [descriptor("first"), descriptor("second")]
            )

    def test_missing_implementation_mapping_fails(self) -> None:
        item = descriptor("test")
        item["functions"][0]["implementations"] = []
        with self.assertRaisesRegex(surfaces.DescriptorError, "missing implementation"):
            surfaces.validate_descriptors(
                {"schema_version": 1, "extensions": ["test"]}, [item]
            )

    def test_platform_constant_uses_target_value(self) -> None:
        rendered = surfaces.render_constant(
            "core",
            {
                "name": "PHP_OS",
                "value": {"kind": "string", "value": "extraction-host"},
                "deprecation": None,
            },
        )
        self.assertIn("crate::constants::PHP_OS", rendered)
        self.assertNotIn("extraction-host", rendered)


def tree(root: Path) -> dict[str, bytes]:
    return {
        path.relative_to(root).as_posix(): path.read_bytes()
        for path in sorted(root.rglob("*"))
        if path.is_file()
    }


if __name__ == "__main__":
    unittest.main()
