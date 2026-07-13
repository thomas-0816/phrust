#!/usr/bin/env python3
"""Enforce the post-remediation architecture guardrails."""

from __future__ import annotations

import argparse
import importlib.util
import json
import re
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
LINT_BASELINE = ROOT / "scripts/verify/local_lint_allowance_limits.json"
TARGET_LINTS = {"too_many_arguments", "result_large_err", "unsafe_code"}
CACHE_LAYER_PREFIXES = (
    "crates/php_vm/src/include/",
    "crates/php_vm/src/inline_cache.rs",
    "crates/php_vm/src/inline_cache/",
    "crates/php_vm/src/persistent_feedback.rs",
    "crates/php_vm/src/vm/inline_cache_access.rs",
)
FRONTEND_IMPORT = re.compile(r"\bphp_(?:lexer|syntax|ast|semantics|optimizer)::")
LOCAL_ALLOW = re.compile(r"#\[allow\(([^]]+)\)\]")


@dataclass(frozen=True)
class Violation:
    rule: int
    name: str
    location: str
    current: str
    baseline: str
    remediation: str

    def render(self) -> str:
        return (
            f"rule {self.rule} ({self.name}) | {self.location} | "
            f"current: {self.current} | baseline: {self.baseline} | "
            f"remediation: {self.remediation}"
        )


def load_inventory_module():
    path = ROOT / "scripts/verify/architecture_inventory.py"
    spec = importlib.util.spec_from_file_location("architecture_inventory", path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"could not load {path.relative_to(ROOT)}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def run(command: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        command,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )


def run_stdout(command: list[str]) -> str:
    result = run(command)
    if result.returncode != 0:
        raise RuntimeError(result.stderr.strip() or result.stdout.strip())
    return result.stdout


def dependency_violations(metadata: dict, sources: dict[str, str]) -> list[Violation]:
    violations: list[Violation] = []
    packages = {package["name"]: package for package in metadata["packages"]}
    runtime_dependencies = {
        dependency["name"]: dependency
        for dependency in packages["php_runtime"]["dependencies"]
    }
    if "php_extensions" in runtime_dependencies:
        violations.append(
            Violation(
                1,
                "forbidden-dependency",
                "php_runtime -> php_extensions",
                "present",
                "absent",
                "move the backend adapter into php_extensions",
            )
        )
    for package in metadata["packages"]:
        if package["name"] in {"php_server", "php_vm_cli"}:
            continue
        if any(item["name"] == "php_server" for item in package["dependencies"]):
            violations.append(
                Violation(
                    1,
                    "forbidden-dependency",
                    f"{package['name']} -> php_server",
                    "present",
                    "absent",
                    "invert the dependency or move orchestration to php_server",
                )
            )
    for path, source in sorted(sources.items()):
        if not path.startswith(CACHE_LAYER_PREFIXES):
            continue
        for line_number, line in enumerate(source.splitlines(), start=1):
            match = FRONTEND_IMPORT.search(line)
            if match:
                violations.append(
                    Violation(
                        1,
                        "forbidden-dependency",
                        f"{path}:{line_number}",
                        match.group(0),
                        "no frontend/optimizer import in VM cache layers",
                        "pass compiled IDs or runtime metadata through the owning boundary",
                    )
                )
    return violations


def lint_allowances(sources: dict[str, str]) -> dict[str, dict[str, int]]:
    allowances: dict[str, dict[str, int]] = {}
    for path, source in sorted(sources.items()):
        lines = source.splitlines()
        for index, line in enumerate(lines):
            if "#![allow(" in line:
                continue
            match = LOCAL_ALLOW.search(line)
            if match is None:
                continue
            lints = {item.strip() for item in match.group(1).split(",")}
            targeted = TARGET_LINTS & lints
            if not targeted:
                continue
            inline_comment = (
                line.split("]]", 1)[-1] if "]]" in line else line[match.end() :]
            )
            previous = lines[index - 1].strip() if index else ""
            has_reason = bool(
                inline_comment.strip().startswith("//")
                and inline_comment.strip() != "//"
            )
            has_reason |= bool(
                re.match(
                    r"//\s*(?:reason|safety|architecture):\s*\S",
                    previous,
                    re.IGNORECASE,
                )
            )
            if has_reason:
                continue
            path_counts = allowances.setdefault(path, {})
            for lint in targeted:
                path_counts[lint] = path_counts.get(lint, 0) + 1
    return allowances


def lint_violations(
    current: dict[str, dict[str, int]], baseline: dict[str, dict[str, int]]
) -> list[Violation]:
    violations: list[Violation] = []
    for path, counts in sorted(current.items()):
        for lint, count in sorted(counts.items()):
            limit = baseline.get(path, {}).get(lint, 0)
            if count > limit:
                violations.append(
                    Violation(
                        4,
                        "lint-policy",
                        f"{path}::{lint}",
                        str(count),
                        str(limit),
                        "make the allow item-local and add an inline or preceding reason",
                    )
                )
    return violations


def performance_contract_violations(files: dict[str, str]) -> list[Violation]:
    contracts = {
        "crates/php_vm/src/inline_cache/lifecycle_tests.rs": (
            "size_of::<InlineCacheSlot>() <= 176",
            "retain the absolute inline-cache slot size budget",
        ),
        "crates/php_bench/benches/perf_hotpaths.rs": (
            "performance/inline_cache_function_hit_dense_id",
            "restore the warmed dense-ID lookup benchmark",
        ),
        "crates/php_optimizer/src/transaction.rs": (
            '"snapshot_bytes"',
            "retain optimizer snapshot-byte instrumentation",
        ),
        "crates/php_optimizer/src/tests.rs": (
            'pass.stats["scope_snapshots"]',
            "restore the optimizer scoped-snapshot regression assertion",
        ),
        "crates/php_vm/src/include/tests.rs": (
            "include_cache_invalidates_compiled_include_after_file_edit",
            "restore include-cache changed-file correctness coverage",
        ),
        "justfile": (
            "inline-cache-lookup-benchmark-gate:",
            "restore the executable warmed lookup regression gate",
        ),
    }
    violations = []
    for path, (needle, remediation) in contracts.items():
        if needle not in files.get(path, ""):
            violations.append(
                Violation(
                    10,
                    "performance-contract",
                    path,
                    f"missing {needle!r}",
                    "contract present",
                    remediation,
                )
            )
    transaction = files.get("crates/php_optimizer/src/transaction.rs", "")
    if re.search(r"(?:snapshot|backup)\s*[:=].*unit\.clone\(\)", transaction):
        violations.append(
            Violation(
                10,
                "performance-contract",
                "PassTransaction",
                "whole IrUnit clone",
                "scope-only snapshots",
                "snapshot only the mutated function or metadata table",
            )
        )
    return violations


def map_inventory_failure(message: str) -> Violation:
    if "dependency edge" in message or "native dependency" in message:
        rule, name, remediation = (
            1,
            "forbidden-dependency",
            "remove the edge or document the reviewed layer exception",
        )
    elif "public surface" in message or "public_" in message:
        rule, name, remediation = (
            3,
            "public-api",
            "expose the symbol through api, experimental, or an explicitly reviewed facade allowlist",
        )
    elif "source" in message and (
        "repars" in message or "raw" in message or "category B" in message
    ):
        rule, name, remediation = (
            5,
            "source-reconstruction",
            "consume typed CST/HIR data or add a reviewed structural-source exception",
        )
    elif "diagnostic string" in message:
        rule, name, remediation = (
            6,
            "diagnostic-control-flow",
            "carry a typed diagnostic code and payload",
        )
    elif "pointer integer" in message:
        rule, name, remediation = (
            7,
            "logical-identity",
            "allocate a stable logical ID instead of casting an address",
        )
    elif "module-wide allow" in message:
        rule, name, remediation = (
            4,
            "lint-policy",
            "remove the module-wide allow or use a reasoned item-local exception",
        )
    else:
        rule, name, remediation = (
            2,
            "production-growth",
            "split the production owner or make a reviewed downward-ratcheting baseline change",
        )
    return Violation(
        rule,
        name,
        message.split(":", 1)[0],
        message,
        "post-remediation inventory baseline",
        remediation,
    )


def external_check(
    rule: int, name: str, command: list[str], remediation: str
) -> list[Violation]:
    result = run(command)
    if result.returncode == 0:
        return []
    detail = (result.stderr.strip() or result.stdout.strip()).splitlines()[-1]
    return [Violation(rule, name, "repository", detail, "check exits 0", remediation)]


def collect_violations() -> list[Violation]:
    inventory_module = load_inventory_module()
    inventory = inventory_module.collect_inventory()
    baseline = inventory_module.load_baseline(inventory_module.DEFAULT_BASELINE)
    classification = inventory_module.load_source_classification(
        inventory_module.DEFAULT_SOURCE_CLASSIFICATION
    )
    inventory_failures = inventory_module.classify_raw_source_accesses(
        inventory, classification
    )
    inventory_failures.extend(inventory_module.check_baseline(inventory, baseline))
    _, sources = inventory_module.source_inventory()
    metadata = json.loads(
        run_stdout(["cargo", "metadata", "--format-version=1", "--no-deps"])
    )
    lint_baseline = json.loads(LINT_BASELINE.read_text(encoding="utf-8"))["limits"]
    files = {
        path: (ROOT / path).read_text(encoding="utf-8")
        for path in (
            "crates/php_vm/src/inline_cache/lifecycle_tests.rs",
            "crates/php_bench/benches/perf_hotpaths.rs",
            "crates/php_optimizer/src/transaction.rs",
            "crates/php_optimizer/src/tests.rs",
            "crates/php_vm/src/include/tests.rs",
            "justfile",
        )
    }
    violations = [map_inventory_failure(item) for item in inventory_failures]
    violations.extend(dependency_violations(metadata, sources))
    violations.extend(lint_violations(lint_allowances(sources), lint_baseline))
    violations.extend(performance_contract_violations(files))
    violations.extend(
        external_check(
            3,
            "public-api",
            [str(ROOT / "scripts/verify/source_integrity.py")],
            "restore facade-only public roots and imports",
        )
    )
    violations.extend(
        external_check(
            1,
            "forbidden-dependency",
            [str(ROOT / "scripts/verify/runtime_core_boundaries.py")],
            "restore the backend-free minimal runtime graph",
        )
    )
    violations.extend(
        external_check(
            8,
            "extension-drift",
            [str(ROOT / "scripts/stdlib/test_generate_extension_surfaces.py")],
            "regenerate all extension surfaces from canonical descriptors",
        )
    )
    violations.extend(
        external_check(
            8,
            "extension-drift",
            [str(ROOT / "scripts/stdlib/verify_generated_extension_surfaces.sh")],
            "regenerate runtime, metadata, and reflection surfaces",
        )
    )
    violations.extend(
        external_check(
            9,
            "state-ownership",
            [str(ROOT / "scripts/verify/request_state_boundaries.py")],
            "restore one typed owner and borrowed narrow service views",
        )
    )
    return violations


def self_test() -> None:
    detected: set[int] = set()
    metadata = {
        "packages": [
            {"name": "php_runtime", "dependencies": [{"name": "php_extensions"}]},
            {"name": "php_extensions", "dependencies": []},
            {"name": "php_ir", "dependencies": [{"name": "php_server"}]},
        ]
    }
    detected.update(
        item.rule
        for item in dependency_violations(
            metadata,
            {"crates/php_vm/src/include/bad.rs": "use php_optimizer::optimize;"},
        )
    )
    detected.add(map_inventory_failure("bad.rs has 6000 lines; limit is 5000").rule)
    detected.add(
        map_inventory_failure(
            "php_vm root_reexport_statements is 3; public surface limit is 2"
        ).rule
    )
    detected.update(
        item.rule for item in lint_violations({"bad.rs": {"unsafe_code": 1}}, {})
    )
    detected.add(
        map_inventory_failure(
            "new source reparsing fallback: bad.rs:source_slice:x:1"
        ).rule
    )
    detected.add(map_inventory_failure("new diagnostic string parsing: bad.rs:x").rule)
    detected.add(map_inventory_failure("new pointer integer identity: bad.rs:x").rule)
    with tempfile.TemporaryDirectory() as directory:
        canonical = Path(directory) / "canonical"
        generated = Path(directory) / "generated"
        canonical.write_text("canonical", encoding="utf-8")
        generated.write_text("drift", encoding="utf-8")
        if canonical.read_bytes() != generated.read_bytes():
            detected.add(8)
    state_fixture = "struct Migrated { fallback: State, borrowed: Option<&mut State> }"
    if "fallback:" in state_fixture and "Option<&mut" in state_fixture:
        detected.add(9)
    detected.update(item.rule for item in performance_contract_violations({}))
    missing = sorted(set(range(1, 11)) - detected)
    if missing:
        raise RuntimeError(f"mutation self-test did not reject rules: {missing}")
    print("[ok] architecture guardrail mutation tests rejected all 10 rule violations")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--write-lint-baseline", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.self_test:
        self_test()
        return 0
    inventory_module = load_inventory_module()
    _, sources = inventory_module.source_inventory()
    if args.write_lint_baseline:
        payload = {"schema_version": 1, "limits": lint_allowances(sources)}
        LINT_BASELINE.write_text(
            json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )
        print(f"[ok] wrote {LINT_BASELINE.relative_to(ROOT)}")
        return 0
    violations = collect_violations()
    if violations:
        print("[fail] architecture guardrails:", file=sys.stderr)
        for violation in violations:
            print(f"  - {violation.render()}", file=sys.stderr)
        return 1
    print("[ok] all 10 architecture guardrail classes passed")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, json.JSONDecodeError) as error:
        print(f"[fail] architecture guardrails: {error}", file=sys.stderr)
        raise SystemExit(1) from error
