#!/usr/bin/env python3
"""Contract tests for the focused parallel rustc workspace wrapper."""

from __future__ import annotations

import json
import os
from pathlib import Path
import stat
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
WRAPPER = ROOT / "scripts/development/parallel_php_vm_rustc.sh"


class ParallelRustcWrapperTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary_directory = tempfile.TemporaryDirectory()
        self.fake_rustc = Path(self.temporary_directory.name) / "fake-rustc.py"
        self.fake_rustc.write_text(
            """#!/usr/bin/env python3
import json
import os
import sys
print(json.dumps({
    "argv": sys.argv[1:],
    "bootstrap": os.environ.get("RUSTC_BOOTSTRAP"),
    "cache_wrapped": os.environ.get("PHRUST_TEST_CACHE_WRAPPED"),
}))
""",
            encoding="utf-8",
        )
        self.fake_rustc.chmod(self.fake_rustc.stat().st_mode | stat.S_IXUSR)

    def tearDown(self) -> None:
        self.temporary_directory.cleanup()

    def run_wrapper(self, crate: str, threads: str = "7") -> subprocess.CompletedProcess[str]:
        environment = os.environ.copy()
        environment.pop("RUSTC_BOOTSTRAP", None)
        environment.pop("PHRUST_RUSTC_CACHE_WRAPPER", None)
        environment["PHRUST_RUSTC_THREADS"] = threads
        return subprocess.run(
            [
                str(WRAPPER),
                str(self.fake_rustc),
                "--crate-name",
                crate,
                "source.rs",
            ],
            cwd=ROOT,
            env=environment,
            check=False,
            capture_output=True,
            text=True,
        )

    def test_workspace_crates_receive_parallel_frontend_threads(self) -> None:
        for crate in ("php_lexer", "php_jit", "php_vm", "phrust_server"):
            with self.subTest(crate=crate):
                result = self.run_wrapper(crate)
                self.assertEqual(result.returncode, 0, result.stderr)
                invocation = json.loads(result.stdout)
                self.assertEqual(invocation["bootstrap"], crate)
                self.assertIn("-Zthreads=7", invocation["argv"])

    def test_other_crates_are_forwarded_without_unstable_flags(self) -> None:
        result = self.run_wrapper("serde")
        self.assertEqual(result.returncode, 0, result.stderr)
        invocation = json.loads(result.stdout)
        self.assertIsNone(invocation["bootstrap"])
        self.assertNotIn("-Zthreads=7", invocation["argv"])

    def test_default_thread_count_is_bounded(self) -> None:
        environment = os.environ.copy()
        environment.pop("RUSTC_BOOTSTRAP", None)
        environment.pop("PHRUST_RUSTC_CACHE_WRAPPER", None)
        environment.pop("PHRUST_RUSTC_THREADS", None)
        result = subprocess.run(
            [
                str(WRAPPER),
                str(self.fake_rustc),
                "--crate-name",
                "php_vm",
                "source.rs",
            ],
            cwd=ROOT,
            env=environment,
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        invocation = json.loads(result.stdout)
        online_cpus = int(subprocess.check_output(["getconf", "_NPROCESSORS_ONLN"], text=True))
        self.assertIn(f"-Zthreads={min(online_cpus, 20)}", invocation["argv"])

    def test_cache_wrapper_is_composed_inside_parallel_wrapper(self) -> None:
        fake_cache = Path(self.temporary_directory.name) / "fake-cache.sh"
        fake_cache.write_text(
            "#!/usr/bin/env bash\nset -euo pipefail\nexport PHRUST_TEST_CACHE_WRAPPED=1\nexec \"$@\"\n",
            encoding="utf-8",
        )
        fake_cache.chmod(fake_cache.stat().st_mode | stat.S_IXUSR)
        environment = os.environ.copy()
        environment["PHRUST_RUSTC_THREADS"] = "7"
        environment["PHRUST_RUSTC_CACHE_WRAPPER"] = str(fake_cache)
        result = subprocess.run(
            [
                str(WRAPPER),
                str(self.fake_rustc),
                "--crate-name",
                "php_vm",
                "source.rs",
            ],
            cwd=ROOT,
            env=environment,
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        invocation = json.loads(result.stdout)
        self.assertIn("-Zthreads=7", invocation["argv"])
        self.assertEqual(invocation["cache_wrapped"], "1")

    def test_invalid_thread_count_is_rejected_before_rustc_runs(self) -> None:
        result = self.run_wrapper("php_vm", "all")
        self.assertEqual(result.returncode, 2)
        self.assertIn("must be a positive integer", result.stderr)
        self.assertEqual(result.stdout, "")


if __name__ == "__main__":
    unittest.main()
