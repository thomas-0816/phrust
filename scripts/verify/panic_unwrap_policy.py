#!/usr/bin/env python3
"""Production panic/unwrap ratchet for runtime, VM, executor, and server code."""

from __future__ import annotations

import json
import re
import sys
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
ALLOWLIST = ROOT / "scripts/verify/panic_unwrap_allowlist.jsonl"
SCAN_DIRS = [
    ROOT / "crates/php_runtime/src",
    ROOT / "crates/php_vm/src",
    ROOT / "crates/php_executor/src",
    ROOT / "crates/php_server/src",
]
PATTERN_RE = re.compile(
    r"\b(?:panic|todo|unimplemented)!\s*\(|\.unwrap\s*\(\s*\)|\.expect\s*\(\s*\""
)


@dataclass(frozen=True)
class Finding:
    path: str
    line_number: int
    line: str


class PolicyError(Exception):
    pass


def rel(path: Path) -> str:
    return path.relative_to(ROOT).as_posix()


def load_allowlist() -> list[dict]:
    entries: list[dict] = []
    for index, line in enumerate(ALLOWLIST.read_text(encoding="utf-8").splitlines(), start=1):
        stripped = line.strip()
        if not stripped:
            continue
        try:
            entry = json.loads(stripped)
        except json.JSONDecodeError as error:
            raise PolicyError(f"allowlist line {index} is not valid JSON: {error}") from error
        missing = {"pattern", "category", "reason"} - set(entry)
        if missing:
            raise PolicyError(
                f"allowlist line {index} missing fields: {', '.join(sorted(missing))}"
            )
        if ("path" in entry) == ("path_prefix" in entry):
            raise PolicyError(
                f"allowlist line {index} must include exactly one of path or path_prefix"
            )
        entries.append(entry)
    return entries


def strip_cfg_test_modules(text: str) -> list[str]:
    lines = text.splitlines()
    result: list[str] = []
    pending_cfg_test = False
    for line in lines:
        stripped = line.strip()
        if stripped.startswith("#[cfg(test)]"):
            pending_cfg_test = True
            result.append("")
            continue
        if pending_cfg_test and (
            not stripped
            or stripped.startswith("#[")
            or stripped.startswith("//")
            or stripped.startswith("/*")
        ):
            result.append("")
            continue
        if pending_cfg_test and re.match(r"(?:pub\s+)?mod\s+tests\s*\{", stripped):
            pending_cfg_test = False
            result.append("")
            result.extend([""] * (len(lines) - len(result)))
            break
        pending_cfg_test = False
        result.append(line)
    return result


def scan() -> list[Finding]:
    findings: list[Finding] = []
    for scan_dir in SCAN_DIRS:
        for path in sorted(scan_dir.rglob("*.rs")):
            relative = rel(path)
            if "/benches/" in relative or relative.endswith("/tests.rs"):
                continue
            for line_number, line in enumerate(
                strip_cfg_test_modules(path.read_text(encoding="utf-8")), start=1
            ):
                stripped = line.strip()
                if not stripped or stripped.startswith("//"):
                    continue
                if "self.expect(" in stripped:
                    continue
                if PATTERN_RE.search(stripped):
                    findings.append(Finding(relative, line_number, stripped))
    return findings


def is_allowed(finding: Finding, entries: list[dict]) -> bool:
    for entry in entries:
        path_matches = entry.get("path") == finding.path or finding.path.startswith(
            entry.get("path_prefix", "\0")
        )
        if path_matches and entry["pattern"] in finding.line:
            return True
    return False


def main() -> int:
    try:
        entries = load_allowlist()
        findings = scan()
    except (OSError, PolicyError) as error:
        print(f"[fail] panic/unwrap policy: {error}", file=sys.stderr)
        return 1
    violations = [finding for finding in findings if not is_allowed(finding, entries)]
    if violations:
        print("[fail] panic/unwrap policy: unallowlisted production uses:", file=sys.stderr)
        for finding in violations[:80]:
            print(f"  - {finding.path}:{finding.line_number}: {finding.line}", file=sys.stderr)
        if len(violations) > 80:
            print(f"  ... {len(violations) - 80} more", file=sys.stderr)
        return 1
    print(f"[ok] panic/unwrap policy scanned {len(findings)} production uses")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
