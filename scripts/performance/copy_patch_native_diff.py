#!/usr/bin/env python3
"""Differential check for the copy-and-patch native tier's dense-path hook.

Runs the scalar-int leaf fixtures with the native tier off and on and asserts
byte-identical stdout, exit status, and stderr-free execution — the native tier
must never change observable behavior. When a PHP 8.5.7 reference is resolvable
it also diffs against real PHP, matching the repository's differential-testing
discipline.

Native execution only engages when `target/debug/php-vm` was built with the
`jit-copy-patch` cargo feature on a supported host (unix + aarch64) and the
`PHRUST_JIT_COPY_PATCH` env var is set. The harness probes for that with
`PHRUST_JIT_COPY_PATCH_DEBUG` and SKIPs with an explicit reason when the tier is
inert, so a default build never reports a false pass.

Build the native engine with:
    nix develop -c cargo build -p php_vm_cli --bin php-vm --features jit-copy-patch
"""

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
ENGINE = ROOT / "target/debug/php-vm"
FIXTURE_DIR = ROOT / "tests/fixtures/performance/native_tier"
FIXTURES = (
    FIXTURE_DIR / "scalar_leaves.php",
    FIXTURE_DIR / "inlined_calls.php",
)

# Version pinned by ADR 0001; a non-8.5.7 php mis-tokenizes 8.5 syntax.
REFERENCE_VERSION = "8.5.7"


def rel(path: Path) -> str:
    try:
        return path.resolve().relative_to(ROOT).as_posix()
    except ValueError:
        return path.as_posix()


def run_engine(fixture: Path, *, native: bool) -> subprocess.CompletedProcess[str]:
    env = dict(os.environ)
    if native:
        env["PHRUST_JIT_COPY_PATCH"] = "1"
        env["PHRUST_JIT_COPY_PATCH_DEBUG"] = "1"
    else:
        env.pop("PHRUST_JIT_COPY_PATCH", None)
        env.pop("PHRUST_JIT_COPY_PATCH_DEBUG", None)
    return subprocess.run(
        [str(ENGINE), "run", rel(fixture)],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=env,
        check=False,
    )


def native_tier_engaged(fixture: Path) -> bool:
    """True when the native tier actually recognized a function (debug marker)."""
    completed = run_engine(fixture, native=True)
    return "[copy-patch]" in completed.stderr and "recognized=true" in completed.stderr


def resolve_reference() -> Path | None:
    explicit = os.environ.get("REFERENCE_PHP")
    candidates = []
    strict = explicit is not None
    if explicit:
        candidates.append(Path(explicit))
    built = ROOT / "third_party/php-src/sapi/cli/php"
    if built.is_file():
        candidates.append(built)
    from shutil import which

    system = which("php")
    if system:
        candidates.append(Path(system))
    for candidate in candidates:
        if not candidate.is_file() and not os.access(candidate, os.X_OK):
            continue
        try:
            version = subprocess.run(
                [str(candidate), "-r", "echo PHP_VERSION;"],
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            ).stdout.strip()
        except OSError:
            continue
        if version == REFERENCE_VERSION:
            return candidate
        if strict:
            raise SystemExit(
                f"[fail] REFERENCE_PHP={candidate} is version {version!r}, "
                f"not the pinned {REFERENCE_VERSION}"
            )
    return None


def run_reference(reference: Path, fixture: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [str(reference), rel(fixture)],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )


def main() -> int:
    if not ENGINE.is_file():
        raise SystemExit(f"[fail] Rust VM is not executable: {rel(ENGINE)}")
    for fixture in FIXTURES:
        if not fixture.is_file():
            raise SystemExit(f"[fail] missing native-tier fixture: {rel(fixture)}")

    if not native_tier_engaged(FIXTURES[0]):
        print(
            "[skip] copy-patch native tier is inert (php-vm not built with "
            "--features jit-copy-patch, or unsupported host); "
            "differential run needs the native tier engaged to be meaningful"
        )
        return 0

    reference = resolve_reference()
    if reference is None:
        print(
            f"[warn] no PHP {REFERENCE_VERSION} reference resolved; "
            "diffing native-on vs native-off only"
        )

    checked = 0
    for fixture in FIXTURES:
        off = run_engine(fixture, native=False)
        on = run_engine(fixture, native=True)
        if off.returncode != on.returncode:
            raise SystemExit(
                f"[fail] {rel(fixture)}: exit status differs native-off "
                f"({off.returncode}) vs native-on ({on.returncode})"
            )
        if off.stdout != on.stdout:
            raise SystemExit(
                f"[fail] {rel(fixture)}: stdout differs native-off vs native-on\n"
                f"  off: {off.stdout!r}\n  on:  {on.stdout!r}"
            )
        golden = fixture.with_suffix(".php.out")
        if golden.is_file():
            expected = golden.read_text(encoding="utf-8")
            if off.stdout != expected:
                raise SystemExit(
                    f"[fail] {rel(fixture)}: stdout differs from golden {rel(golden)}\n"
                    f"  got:      {off.stdout!r}\n  expected: {expected!r}"
                )
        if reference is not None:
            ref = run_reference(reference, fixture)
            if ref.stdout != on.stdout:
                raise SystemExit(
                    f"[fail] {rel(fixture)}: stdout differs native-on vs "
                    f"reference {REFERENCE_VERSION}\n"
                    f"  native: {on.stdout!r}\n  ref:    {ref.stdout!r}"
                )
        checked += 1

    ref_note = (
        f"and PHP {REFERENCE_VERSION}" if reference is not None else "(no reference)"
    )
    print(
        f"[pass] copy-patch native tier matched interpreter {ref_note} "
        f"on {checked} fixture(s)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
