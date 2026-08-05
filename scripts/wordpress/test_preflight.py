from __future__ import annotations

import argparse
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))

import preflight


class WordPressReferencePreflightTests(unittest.TestCase):
    def args(self, root: Path, reference: Path) -> argparse.Namespace:
        return argparse.Namespace(
            wordpress_dir=str(root),
            docroot=str(root),
            reference_php=str(reference),
            require_reference=True,
            phrust_binary=str(root / "php-vm"),
            phrust_server=str(root / "phrust-server"),
            db_enabled=False,
            db_dsn_env="PHRUST_TEST_UNUSED_DSN",
            listen="127.0.0.1:0",
            out="",
        )

    def report(self, extensions: set[str]) -> dict[str, object]:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            reference = root / "php"
            reference.touch(mode=0o755)
            (root / "php-vm").touch(mode=0o755)
            (root / "phrust-server").touch(mode=0o755)
            with (
                mock.patch.object(preflight, "wordpress_shape_blockers", return_value=[]),
                mock.patch.object(preflight, "executable", return_value=True),
                mock.patch.object(preflight, "binary_is_stale", return_value=False),
                mock.patch.object(preflight, "is_port_available", return_value=True),
                mock.patch.object(preflight, "reference_version", return_value="8.5.7"),
                mock.patch.object(
                    preflight,
                    "reference_loaded_extensions",
                    return_value=extensions,
                ),
            ):
                return preflight.build_report(self.args(root, reference))

    def test_rejects_reference_php_without_mysqli(self) -> None:
        report = self.report({"core", "tokenizer"})
        self.assertEqual(report["status"], "fail")
        self.assertIn(
            "reference_php_missing_mysqli",
            report["environment_blockers"],
        )

    def test_accepts_and_publishes_mysqli_capability(self) -> None:
        report = self.report({"core", "mysqli", "mysqlnd", "tokenizer"})
        self.assertEqual(report["status"], "ok")
        self.assertEqual(
            report["inputs"]["reference_php_extensions"],
            ["core", "mysqli", "mysqlnd", "tokenizer"],
        )


if __name__ == "__main__":
    unittest.main()
