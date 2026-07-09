#!/usr/bin/env python3
"""Ratchet high-risk diagnostics/debug boundary patterns.

This is intentionally scoped to process, CLI, executor, server, VM include, and
runtime builtin boundaries. It is not a general Rust lint replacement.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]

SCANNED = [
    "crates/php_server/src/main.rs",
    "crates/php_server/src/config.rs",
    "crates/php_server/src/server.rs",
    "crates/php_executor/src/diagnostics.rs",
    "crates/php_executor/src/engine_compat.rs",
    "crates/php_executor/src/cache.rs",
    "crates/php_vm_cli/src/commands.rs",
    "crates/php_vm_cli/src/main.rs",
    "crates/php_vm_cli/src/bin/phrust_php.rs",
    "crates/php_vm/src/include.rs",
    "crates/php_vm/src/vm/result.rs",
    "crates/php_runtime/src/builtins/error.rs",
]

ALLOW_MARKER = "phrust-diagnostics-allow:"

INVARIANT_TEXT = [
    "mutex poisoned",
    "response builder is valid",
    "header is valid",
    "tokio runtime should initialize",
    "CLI JSON output should be serializable",
    "string serialization cannot fail",
    "JSON is valid",
    "workspace crates directory",
]

APPROVED_DIRECT_STDERR = [
    "eprintln!();",
    "eprintln!(\"{}\", ServerConfig::help_text())",
    "startup docroot=",
]

APPROVED_ERROR_CONVERSIONS = [
    ".write_all(",
    "write!(",
    "writeln!(",
    "stdout.flush()",
    "json_line()",
    "to_json()",
    "to_json_string(",
    "serde_json::to_string",
    "String::from_utf8",
]

PATTERNS = [
    ("direct-eprintln", re.compile(r"\beprintln!\s*\(")),
    ("unwrap-expect-panic", re.compile(r"\b(unwrap|expect|panic!)\s*\(")),
    ("boundary-string-error", re.compile(r"Err\((format!\(|\".*\"\.to_string\(\))")),
    ("map-err-to-string", re.compile(r"map_err\(\|error\| error\.to_string\(\)\)")),
    ("raw-debug-public", re.compile(r"\{\:\?\}")),
]


def is_test_tail(line: str) -> bool:
    return line.strip() == "mod tests {"


def has_inline_allow(lines: list[str], index: int) -> bool:
    current = lines[index]
    previous = lines[index - 1] if index > 0 else ""
    return ALLOW_MARKER in current or ALLOW_MARKER in previous


LEGACY_BASELINE = {
    # Existing CLI/reporting helpers still return String internally, but the
    # process boundary converts command errors to E_PHRUST_CLI_USAGE envelopes.
    # Keep this count visible and fail when it grows.
    ("crates/php_vm_cli/src/commands.rs", "boundary-string-error"): 8,
    ("crates/php_vm_cli/src/commands.rs", "map-err-to-string"): 2,
}


def line_window(lines: list[str], index: int) -> str:
    start = max(0, index - 14)
    end = min(len(lines), index + 3)
    return "\n".join(lines[start:end])


def approved(kind: str, line: str, window: str) -> str | None:
    if kind == "direct-eprintln":
        if any(text in window for text in APPROVED_DIRECT_STDERR):
            return "approved top-level server startup/help stderr"
    if kind == "unwrap-expect-panic":
        if any(text in line for text in INVARIANT_TEXT):
            return "approved invariant message"
    if kind == "map-err-to-string":
        if any(text in window for text in APPROVED_ERROR_CONVERSIONS):
            return "approved local render/write conversion"
    if kind == "boundary-string-error":
        if "E_PHP_" in window or "E_PHRUST_" in window:
            return "approved stable coded internal error"
        if "requires" in window or "unexpected" in window or "unknown" in window:
            return "approved CLI usage parser string converted at boundary"
    if kind == "raw-debug-public":
        if "status" in window or "version" in window or "trace" in window or "preload entry" in window:
            return "approved structured status/debug/preload context"
    return None


def main() -> int:
    findings: list[str] = []
    suppressions: list[str] = []
    legacy_counts: dict[tuple[str, str], int] = {}
    for relative in SCANNED:
        path = ROOT / relative
        lines = path.read_text(encoding="utf-8").splitlines()
        in_tests = False
        for index, line in enumerate(lines):
            if is_test_tail(line):
                in_tests = True
            if in_tests:
                continue
            if has_inline_allow(lines, index):
                suppressions.append(f"{relative}:{index + 1}: inline suppression")
                continue
            window = line_window(lines, index)
            for kind, pattern in PATTERNS:
                if not pattern.search(line):
                    continue
                reason = approved(kind, line, window)
                if reason:
                    suppressions.append(f"{relative}:{index + 1}: {kind}: {reason}")
                    continue
                key = (relative, kind)
                if key in LEGACY_BASELINE:
                    legacy_counts[key] = legacy_counts.get(key, 0) + 1
                    suppressions.append(
                        f"{relative}:{index + 1}: {kind}: legacy baseline; TODO migrate to envelope-backed error"
                    )
                    continue
                findings.append(f"{relative}:{index + 1}: {kind}: {line.strip()}")

    for key, allowed in LEGACY_BASELINE.items():
        count = legacy_counts.get(key, 0)
        if count > allowed:
            relative, kind = key
            findings.append(
                f"{relative}: {kind}: legacy baseline grew from {allowed} to {count}"
            )
        elif count < allowed:
            suppressions.append(
                f"{key[0]}: {key[1]}: legacy baseline reduced {allowed}->{count}; update audit baseline"
            )

    for suppression in suppressions:
        print(f"[allow] {suppression}")
    if findings:
        print("[fail] diagnostics audit found unsuppressed boundary patterns:", file=sys.stderr)
        for finding in findings:
            print(finding, file=sys.stderr)
        return 1
    print(f"[ok] diagnostics audit passed ({len(suppressions)} suppressions visible).")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
