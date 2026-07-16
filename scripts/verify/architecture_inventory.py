#!/usr/bin/env python3
"""Generate and enforce the source-derived architecture inventory."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
from collections.abc import Iterable
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_BASELINE = ROOT / "scripts/verify/architecture_inventory_baseline.json"
DEFAULT_SOURCE_CLASSIFICATION = (
    ROOT / "scripts/verify/frontend_source_text_inventory.json"
)
DEFAULT_REPORT_JSON = ROOT / "target/architecture/inventory.json"
DEFAULT_REPORT_MD = ROOT / "target/architecture/inventory.md"

DEFAULT_MAX_FILE_LINES = 5_000
DEFAULT_MAX_FILE_BYTES = 200_000

SOURCE_REPARSE_PATTERNS = {
    "source_slice": re.compile(r"(?:&?source|source_text)\s*\["),
    "source_lines": re.compile(r"\bsource\.lines\(\)"),
    "textual_split": re.compile(
        r"\b(?:source|source_text|rest|text|value)\.split_(?:once|whitespace)\("
    ),
    "source_text_slice_method": re.compile(r"\b(?:self\.)?source_text\.slice\("),
    "removed_structural_helper": re.compile(
        r"\b(?:global_names_from_stmt_source|simple_construct_[A-Za-z0-9_]*_source|"
        r"dynamic_member_[A-Za-z0-9_]*_source|"
        r"define_constant_initializers_from_source|"
        r"source_constant_from_default_source)\b"
    ),
}
RAW_SOURCE_ACCESS_PATTERNS = {
    "source_text_member": re.compile(r"\bself\.source_text\b"),
    "token_text_reconstruction": re.compile(r"\bsource_text_no_trivia\b"),
}
POINTER_IDENTITY_PATTERN = re.compile(
    r"(?:Arc|Rc)::as_ptr\([^\n]+\)\s*(?:\.cast::<[^>]+>\(\))?\s+as\s+(?:u|i)size"
)
DIAGNOSTIC_PARSE_PATTERNS = {
    "diagnostic_code_prefix_parse": re.compile(r"strip_prefix\(\"E_[A-Z0-9_]+"),
    "diagnostic_target_parse": re.compile(
        r"(?:include_diagnostic_target|include_failure_target_and_reason)"
        r"\(diagnostic\.message\(\)\)"
    ),
}

BENCHMARK_TARGETS = {
    "vm_dispatch": "benchmark-suite",
    "include_cache": "inline-cache-smoke",
    "compiled_cache": "cache-roundtrip",
    "compiler": "optimizer-diff",
    "server": "server-benchmark-smoke",
    "application": "app-flow-smoke",
    "front_controller": "front-controller-hotpath-smoke",
    "wordpress": "wordpress-root-benchmark",
}


class InventoryError(Exception):
    """An actionable inventory collection or baseline error."""


def run(*command: str) -> str:
    result = subprocess.run(
        command,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode != 0:
        detail = result.stderr.strip() or result.stdout.strip()
        raise InventoryError(f"{' '.join(command)} failed: {detail}")
    return result.stdout


def tracked_rust_files() -> list[Path]:
    output = run(
        "git",
        "ls-files",
        "--cached",
        "--others",
        "--exclude-standard",
        "--",
        "*.rs",
    )
    return [
        Path(line)
        for line in output.splitlines()
        if line and (ROOT / line).is_file()
    ]


def classify_rust_file(path: Path, content: bytes) -> str:
    parts = path.parts
    if parts and parts[0] == "third_party":
        return "vendored"
    if "generated" in parts or b"@generated" in content[:512]:
        return "generated"
    if any(part in {"tests", "benches", "examples", "fixtures"} for part in parts):
        return "test"
    if path.name in {"tests.rs", "test.rs"}:
        return "test"
    if len(parts) >= 4 and parts[0] == "crates" and parts[2] == "src":
        return "production"
    return "tooling"


def source_inventory() -> tuple[dict, dict[str, str]]:
    rows: list[dict] = []
    sources: dict[str, str] = {}
    categories: dict[str, list[str]] = {
        "production": [],
        "generated": [],
        "vendored": [],
        "test": [],
        "tooling": [],
    }
    for relative in tracked_rust_files():
        absolute = ROOT / relative
        content = absolute.read_bytes()
        category = classify_rust_file(relative, content)
        relative_text = relative.as_posix()
        categories[category].append(relative_text)
        if category != "production":
            continue
        text = content.decode("utf-8")
        sources[relative_text] = text
        rows.append(
            {
                "path": relative_text,
                "lines": len(text.splitlines()),
                "bytes": len(content),
            }
        )
    rows.sort(key=lambda row: (-row["lines"], -row["bytes"], row["path"]))
    for paths in categories.values():
        paths.sort()
    return (
        {
            "classification": {
                "rules": {
                    "production": (
                        "tracked crates/*/src/**/*.rs excluding generated and test modules"
                    ),
                    "generated": "generated path component or @generated file header",
                    "vendored": "tracked Rust source below third_party/",
                    "test": "tests, benches, examples, fixtures, test.rs, or tests.rs",
                    "tooling": "other tracked Rust source",
                },
                "counts": {name: len(paths) for name, paths in sorted(categories.items())},
                "generated_files": categories["generated"],
                "vendored_files": categories["vendored"],
                "vendored_roots": sorted(
                    {str(Path(path).parts[0]) for path in categories["vendored"]}
                ),
            },
            "production": {
                "file_count": len(rows),
                "total_lines": sum(row["lines"] for row in rows),
                "total_bytes": sum(row["bytes"] for row in rows),
                "files": rows,
                "top_20": rows[:20],
            },
        },
        sources,
    )


def cargo_metadata() -> dict:
    return json.loads(run("cargo", "metadata", "--format-version=1", "--locked"))


def dependency_inventory(metadata: dict) -> dict:
    packages_by_id = {package["id"]: package for package in metadata["packages"]}
    workspace_ids = set(metadata["workspace_members"])
    workspace_names = {
        packages_by_id[package_id]["name"] for package_id in workspace_ids
    }
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}

    edges: list[dict] = []
    platform_by_crate: dict[str, list[dict]] = {}
    for package_id in sorted(workspace_ids, key=lambda item: packages_by_id[item]["name"]):
        package = packages_by_id[package_id]
        crate = package["name"]
        for dependency in package.get("dependencies", []):
            if dependency["name"] in workspace_names:
                edges.append(
                    {
                        "from": crate,
                        "to": dependency["name"],
                        "kind": dependency["kind"] or "normal",
                        "optional": dependency["optional"],
                    }
                )
            if dependency.get("target"):
                platform_by_crate.setdefault(crate, []).append(
                    {
                        "name": dependency["name"],
                        "kind": dependency["kind"] or "normal",
                        "target": dependency["target"],
                    }
                )
    edges.sort(key=lambda row: (row["from"], row["to"], row["kind"], row["optional"]))
    for dependencies in platform_by_crate.values():
        dependencies.sort(key=lambda row: (row["name"], row["kind"], row["target"]))

    native_ids = {
        package_id
        for package_id, package in packages_by_id.items()
        if package.get("links")
    }
    native_by_crate: dict[str, list[dict]] = {}
    for workspace_id in sorted(workspace_ids, key=lambda item: packages_by_id[item]["name"]):
        seen: set[str] = set()
        pending = [workspace_id]
        native: dict[tuple[str, str], dict] = {}
        while pending:
            package_id = pending.pop()
            if package_id in seen:
                continue
            seen.add(package_id)
            if package_id in native_ids and package_id != workspace_id:
                package = packages_by_id[package_id]
                native[(package["name"], package["links"])] = {
                    "name": package["name"],
                    "links": package["links"],
                }
            pending.extend(dependency["pkg"] for dependency in nodes[package_id]["deps"])
        native_by_crate[packages_by_id[workspace_id]["name"]] = [
            native[key] for key in sorted(native)
        ]

    return {
        "workspace_edges": edges,
        "native_dependencies": native_by_crate,
        "platform_dependencies": {
            crate: platform_by_crate.get(crate, [])
            for crate in sorted(workspace_names)
        },
    }


def brace_depth_before_lines(text: str) -> Iterable[tuple[int, str]]:
    depth = 0
    in_block_comment = False
    for line in text.splitlines():
        yield depth, line
        index = 0
        in_string = False
        escaped = False
        while index < len(line):
            pair = line[index : index + 2]
            if in_block_comment:
                if pair == "*/":
                    in_block_comment = False
                    index += 2
                    continue
                index += 1
                continue
            if not in_string and pair == "/*":
                in_block_comment = True
                index += 2
                continue
            if not in_string and pair == "//":
                break
            character = line[index]
            if character == '"' and not escaped:
                in_string = not in_string
            if not in_string:
                if character == "{":
                    depth += 1
                elif character == "}":
                    depth = max(0, depth - 1)
            escaped = character == "\\" and not escaped
            if character != "\\":
                escaped = False
            index += 1


def public_surface(source: str) -> dict:
    counts = {
        "public_module_declarations": 0,
        "public_reexport_statements": 0,
        "root_public_module_declarations": 0,
        "root_reexport_statements": 0,
    }
    for depth, line in brace_depth_before_lines(source):
        stripped = line.strip()
        if re.match(r"pub\s+mod\s+[A-Za-z_][A-Za-z0-9_]*", stripped):
            counts["public_module_declarations"] += 1
            if depth == 0:
                counts["root_public_module_declarations"] += 1
        if re.match(r"pub\s+use\s+", stripped):
            counts["public_reexport_statements"] += 1
            if depth == 0:
                counts["root_reexport_statements"] += 1
    return counts


def finding_id(path: str, category: str, text: str) -> str:
    normalized = " ".join(text.strip().split())
    digest = hashlib.sha256(normalized.encode("utf-8")).hexdigest()[:12]
    return f"{path}:{category}:{digest}"


def collect_findings(
    sources: dict[str, str],
    patterns: dict[str, re.Pattern[str]],
    path_prefixes: tuple[str, ...] | None = None,
) -> list[dict]:
    findings: list[dict] = []
    occurrences: dict[str, int] = {}
    for path, source in sorted(sources.items()):
        if path_prefixes and not path.startswith(path_prefixes):
            continue
        for line_number, line in enumerate(source.splitlines(), start=1):
            for category, pattern in patterns.items():
                if not pattern.search(line):
                    continue
                base_id = finding_id(path, category, line)
                occurrence = occurrences.get(base_id, 0) + 1
                occurrences[base_id] = occurrence
                findings.append(
                    {
                        "id": f"{base_id}:{occurrence}",
                        "path": path,
                        "line": line_number,
                        "category": category,
                        "text": line.strip(),
                    }
                )
    return findings


def module_wide_allows(sources: dict[str, str]) -> list[dict]:
    pattern = re.compile(r"^#!\[allow\(([^]]+)\)\]")
    rows: list[dict] = []
    occurrences: dict[str, int] = {}
    for path, source in sorted(sources.items()):
        for line_number, line in enumerate(source.splitlines(), start=1):
            match = pattern.match(line.strip())
            if not match:
                continue
            category = match.group(1).strip()
            base_id = finding_id(path, "module_wide_allow", line)
            occurrence = occurrences.get(base_id, 0) + 1
            occurrences[base_id] = occurrence
            rows.append(
                {
                    "id": f"{base_id}:{occurrence}",
                    "path": path,
                    "line": line_number,
                    "allow": category,
                }
            )
    return rows


def pointer_findings(sources: dict[str, str]) -> list[dict]:
    return collect_findings(
        sources,
        {"pointer_integer_identity": POINTER_IDENTITY_PATTERN},
    )


def benchmark_commands() -> list[dict]:
    justfile = (ROOT / "justfile").read_text(encoding="utf-8")
    targets = {
        match.group(1)
        for match in re.finditer(r"^([A-Za-z0-9_-]+)(?:\s+[^:]*)?:\s*$", justfile, re.MULTILINE)
    }
    rows = [
        {
            "category": "compile_clean",
            "command": (
                "nix develop -c cargo build -p php_runtime -p php_vm "
                "-p php_executor -p php_server"
            ),
            "measurement": "three clean package rebuilds with warm dependencies; median and range",
        },
        {
            "category": "compile_incremental",
            "command": (
                "nix develop -c cargo build -p php_runtime -p php_vm "
                "-p php_executor -p php_server"
            ),
            "measurement": "three incremental root-touch rebuilds; median and range",
        },
        {
            "category": "binary_size",
            "command": "nix develop -c cargo build --release -p php_vm_cli -p php_server",
            "measurement": "target/release/php-vm, phrust-php, and phrust-server bytes",
        },
    ]
    for category, target in BENCHMARK_TARGETS.items():
        if target in targets:
            rows.append(
                {
                    "category": category,
                    "command": f"nix develop -c just {target}",
                    "measurement": "repository-owned benchmark or smoke report",
                }
            )
    return rows


def collect_inventory() -> dict:
    source_data, sources = source_inventory()
    metadata = cargo_metadata()
    public_surfaces = {}
    for crate in ("php_runtime", "php_vm"):
        path = f"crates/{crate}/src/lib.rs"
        public_surfaces[crate] = public_surface(sources[path])
    return {
        "schema_version": 1,
        "source_revision": run("git", "rev-parse", "HEAD").strip(),
        "rust_sources": source_data,
        "dependencies": dependency_inventory(metadata),
        "public_surfaces": public_surfaces,
        "module_wide_allows": module_wide_allows(sources),
        "source_reparsing": collect_findings(
            sources,
            SOURCE_REPARSE_PATTERNS,
            (
                "crates/php_executor/src/",
                "crates/php_ir/src/lower/",
                "crates/php_semantics/src/lower/",
            ),
        ),
        "raw_source_accesses": collect_findings(
            sources,
            RAW_SOURCE_ACCESS_PATTERNS,
            ("crates/php_ir/src/lower/", "crates/php_semantics/src/lower/"),
        ),
        "pointer_integer_identity": pointer_findings(sources),
        "diagnostic_string_parsing": collect_findings(
            sources, DIAGNOSTIC_PARSE_PATTERNS
        ),
        "benchmark_commands": benchmark_commands(),
    }


def allowlist(
    rows: list[dict], reason: str, previous_rows: list[dict] | None = None
) -> list[dict]:
    previous_reasons = {
        row["id"]: row["reason"]
        for row in previous_rows or []
        if isinstance(row.get("id"), str) and isinstance(row.get("reason"), str)
    }
    return [
        {"id": row["id"], "reason": previous_reasons.get(row["id"], reason)}
        for row in rows
    ]


def native_ids(inventory: dict) -> list[str]:
    rows = []
    for crate, dependencies in inventory["dependencies"]["native_dependencies"].items():
        for dependency in dependencies:
            rows.append(f"{crate}:{dependency['name']}:{dependency['links']}")
    return sorted(rows)


def platform_ids(inventory: dict) -> list[str]:
    rows = []
    for crate, dependencies in inventory["dependencies"]["platform_dependencies"].items():
        for dependency in dependencies:
            rows.append(
                f"{crate}:{dependency['name']}:{dependency['kind']}:{dependency['target']}"
            )
    return sorted(rows)


def edge_ids(inventory: dict) -> list[str]:
    return sorted(
        f"{row['from']}:{row['to']}:{row['kind']}:{str(row['optional']).lower()}"
        for row in inventory["dependencies"]["workspace_edges"]
    )


def baseline_for(inventory: dict, previous: dict | None = None) -> dict:
    previous = previous or {}
    production_files = inventory["rust_sources"]["production"]["files"]
    large_files = {
        row["path"]: {"max_lines": row["lines"], "max_bytes": row["bytes"]}
        for row in sorted(production_files, key=lambda item: item["path"])
        if row["lines"] > DEFAULT_MAX_FILE_LINES
        or row["bytes"] > DEFAULT_MAX_FILE_BYTES
    }
    return {
        "schema_version": 1,
        "tolerances": {
            "default_max_file_lines": DEFAULT_MAX_FILE_LINES,
            "default_max_file_bytes": DEFAULT_MAX_FILE_BYTES,
            "tracked_large_file_growth_lines": 0,
            "tracked_large_file_growth_bytes": 0,
            "public_surface_growth": 0,
        },
        "large_file_limits": large_files,
        "generated_file_allowlist": inventory["rust_sources"]["classification"][
            "generated_files"
        ],
        "vendored_file_allowlist": inventory["rust_sources"]["classification"][
            "vendored_files"
        ],
        "workspace_dependency_edges": edge_ids(inventory),
        "native_dependency_allowlist": native_ids(inventory),
        "platform_dependency_allowlist": platform_ids(inventory),
        "public_surface_limits": inventory["public_surfaces"],
        "module_wide_allowlist": allowlist(
            inventory["module_wide_allows"],
            "pre-remediation module-wide lint debt; remove when the owning module is split",
            previous.get("module_wide_allowlist"),
        ),
        "source_reparsing_allowlist": allowlist(
            inventory["source_reparsing"],
            "pre-remediation typed frontend information loss; remove in Prompt 12",
            previous.get("source_reparsing_allowlist"),
        ),
        "pointer_integer_identity_allowlist": allowlist(
            inventory["pointer_integer_identity"],
            "pre-remediation pointer identity; server cache identity is removed in Prompt 15",
            previous.get("pointer_integer_identity_allowlist"),
        ),
        "diagnostic_string_parsing_allowlist": allowlist(
            inventory["diagnostic_string_parsing"],
            "pre-remediation diagnostic display parsing; remove in Prompt 02 or Prompt 15",
            previous.get("diagnostic_string_parsing_allowlist"),
        ),
    }


def load_baseline(path: Path) -> dict:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except OSError as error:
        raise InventoryError(f"could not read baseline {path}: {error}") from error
    except json.JSONDecodeError as error:
        raise InventoryError(f"invalid baseline JSON in {path}: {error}") from error


def load_source_classification(path: Path) -> dict:
    classification = load_baseline(path)
    if classification.get("schema_version") != 1:
        raise InventoryError(
            "frontend source-text inventory schema_version must be 1"
        )
    return classification


def classify_raw_source_accesses(inventory: dict, classification: dict) -> list[str]:
    entries = classification.get("entries")
    if not isinstance(entries, list):
        raise InventoryError("frontend source-text inventory entries must be a list")
    by_id = {}
    failures = []
    for entry in entries:
        if not isinstance(entry, dict) or not isinstance(entry.get("id"), str):
            raise InventoryError("frontend source-text inventory entries need string ids")
        entry_id = entry["id"]
        if entry_id in by_id:
            raise InventoryError(f"duplicate frontend source-text inventory id: {entry_id}")
        category = entry.get("category")
        if category not in {"A", "B", "C"}:
            raise InventoryError(
                f"frontend source-text inventory {entry_id} has invalid category {category!r}"
            )
        if category == "B":
            failures.append(f"category B structural source recovery remains: {entry_id}")
        if not isinstance(entry.get("purpose"), str) or not entry["purpose"].strip():
            raise InventoryError(
                f"frontend source-text inventory {entry_id} needs a purpose"
            )
        by_id[entry_id] = entry

    current_ids = {row["id"] for row in inventory["raw_source_accesses"]}
    classified_ids = set(by_id)
    failures.extend(
        f"unclassified raw source access: {entry_id}"
        for entry_id in sorted(current_ids - classified_ids)
    )
    failures.extend(
        f"stale raw source classification: {entry_id}"
        for entry_id in sorted(classified_ids - current_ids)
    )
    for row in inventory["raw_source_accesses"]:
        entry = by_id.get(row["id"])
        if entry is not None:
            row["classification"] = entry["category"]
            row["purpose"] = entry["purpose"]
    return failures


def unexpected_ids(current: list[str], allowed: list[str], label: str) -> list[str]:
    extra = sorted(set(current) - set(allowed))
    return [f"new {label}: {item}" for item in extra]


def check_baseline(inventory: dict, baseline: dict) -> list[str]:
    if baseline.get("schema_version") != inventory["schema_version"]:
        return [
            "baseline schema_version does not match inventory schema_version "
            f"({baseline.get('schema_version')} != {inventory['schema_version']})"
        ]
    failures: list[str] = []
    tolerances = baseline["tolerances"]
    default_lines = tolerances["default_max_file_lines"]
    default_bytes = tolerances["default_max_file_bytes"]
    line_growth = tolerances["tracked_large_file_growth_lines"]
    byte_growth = tolerances["tracked_large_file_growth_bytes"]
    large_limits = baseline["large_file_limits"]
    for row in inventory["rust_sources"]["production"]["files"]:
        limits = large_limits.get(row["path"])
        max_lines = default_lines if limits is None else limits["max_lines"] + line_growth
        max_bytes = default_bytes if limits is None else limits["max_bytes"] + byte_growth
        if row["lines"] > max_lines:
            failures.append(
                f"{row['path']} has {row['lines']} lines; limit is {max_lines}"
            )
        if row["bytes"] > max_bytes:
            failures.append(
                f"{row['path']} has {row['bytes']} bytes; limit is {max_bytes}"
            )

    classification = inventory["rust_sources"]["classification"]
    failures.extend(
        unexpected_ids(
            classification["generated_files"],
            baseline["generated_file_allowlist"],
            "generated Rust file classification",
        )
    )
    failures.extend(
        unexpected_ids(
            classification["vendored_files"],
            baseline["vendored_file_allowlist"],
            "vendored Rust file classification",
        )
    )

    failures.extend(
        unexpected_ids(
            edge_ids(inventory),
            baseline["workspace_dependency_edges"],
            "workspace dependency edge",
        )
    )
    failures.extend(
        unexpected_ids(
            native_ids(inventory),
            baseline["native_dependency_allowlist"],
            "native dependency",
        )
    )
    failures.extend(
        unexpected_ids(
            platform_ids(inventory),
            baseline["platform_dependency_allowlist"],
            "platform dependency",
        )
    )

    public_growth = tolerances["public_surface_growth"]
    for crate, counts in inventory["public_surfaces"].items():
        limits = baseline["public_surface_limits"][crate]
        for name, count in counts.items():
            limit = limits[name] + public_growth
            if count > limit:
                failures.append(
                    f"{crate} {name} is {count}; public surface limit is {limit}"
                )

    for inventory_key, baseline_key, label in (
        ("module_wide_allows", "module_wide_allowlist", "module-wide allow"),
        ("source_reparsing", "source_reparsing_allowlist", "source reparsing fallback"),
        (
            "pointer_integer_identity",
            "pointer_integer_identity_allowlist",
            "pointer integer identity",
        ),
        (
            "diagnostic_string_parsing",
            "diagnostic_string_parsing_allowlist",
            "diagnostic string parsing",
        ),
    ):
        failures.extend(
            unexpected_ids(
                [row["id"] for row in inventory[inventory_key]],
                [row["id"] for row in baseline[baseline_key]],
                label,
            )
        )
    return failures


def markdown_table(headers: list[str], rows: Iterable[Iterable[object]]) -> list[str]:
    lines = [
        "| " + " | ".join(headers) + " |",
        "| " + " | ".join("---" for _ in headers) + " |",
    ]
    lines.extend("| " + " | ".join(str(value) for value in row) + " |" for row in rows)
    return lines


def render_markdown(inventory: dict, failures: list[str]) -> str:
    production = inventory["rust_sources"]["production"]
    lines = [
        "# Architecture Inventory",
        "",
        f"Source revision: `{inventory['source_revision']}`",
        "",
        f"Status: `{'fail' if failures else 'pass'}`",
        "",
        "## Production Rust",
        "",
        (
            f"{production['file_count']} files, {production['total_lines']} lines, "
            f"{production['total_bytes']} bytes."
        ),
        "",
        "### Top 20 Production Files",
        "",
    ]
    lines.extend(
        markdown_table(
            ["Path", "Lines", "Bytes"],
            (
                (f"`{row['path']}`", row["lines"], row["bytes"])
                for row in production["top_20"]
            ),
        )
    )
    lines.extend(["", "## Workspace Dependency Edges", ""])
    lines.extend(
        markdown_table(
            ["From", "To", "Kind", "Optional"],
            (
                (row["from"], row["to"], row["kind"], row["optional"])
                for row in inventory["dependencies"]["workspace_edges"]
            ),
        )
    )
    lines.extend(["", "## Native Dependencies", ""])
    native_rows = []
    for crate, dependencies in inventory["dependencies"]["native_dependencies"].items():
        if dependencies:
            native_rows.append(
                (crate, ", ".join(f"{row['name']} ({row['links']})" for row in dependencies))
            )
    lines.extend(markdown_table(["Crate", "Native link packages"], native_rows))
    lines.extend(["", "## Public Surface", ""])
    lines.extend(
        markdown_table(
            ["Crate", "Public modules", "Root modules", "Re-exports", "Root re-exports"],
            (
                (
                    crate,
                    counts["public_module_declarations"],
                    counts["root_public_module_declarations"],
                    counts["public_reexport_statements"],
                    counts["root_reexport_statements"],
                )
                for crate, counts in inventory["public_surfaces"].items()
            ),
        )
    )
    for title, key in (
        ("Module-wide Allows", "module_wide_allows"),
        ("Source Reparsing Fallbacks", "source_reparsing"),
        ("Classified Raw Source Accesses", "raw_source_accesses"),
        ("Pointer Integer Identity", "pointer_integer_identity"),
        ("Diagnostic String Parsing", "diagnostic_string_parsing"),
    ):
        lines.extend(["", f"## {title}", ""])
        rows = inventory[key]
        if not rows:
            lines.append("None.")
            continue
        lines.extend(
            markdown_table(
                ["Path", "Line", "Category"],
                (
                    (
                        f"`{row['path']}`",
                        row["line"],
                        (
                            row.get("classification", row.get("category", row.get("allow", "")))
                            + (
                                f": {row['purpose']}"
                                if row.get("purpose")
                                else ""
                            )
                        ),
                    )
                    for row in rows
                ),
            )
        )
    lines.extend(["", "## Baseline Commands", ""])
    lines.extend(
        markdown_table(
            ["Category", "Command", "Measurement"],
            (
                (row["category"], f"`{row['command']}`", row["measurement"])
                for row in inventory["benchmark_commands"]
            ),
        )
    )
    if failures:
        lines.extend(["", "## Regressions", ""])
        lines.extend(f"- {failure}" for failure in failures)
    lines.append("")
    return "\n".join(lines)


def write_report(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")


def canonical_json(value: dict) -> str:
    return json.dumps(value, indent=2, sort_keys=True) + "\n"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path, default=DEFAULT_BASELINE)
    parser.add_argument(
        "--source-classification",
        type=Path,
        default=DEFAULT_SOURCE_CLASSIFICATION,
    )
    parser.add_argument("--json-out", type=Path, default=DEFAULT_REPORT_JSON)
    parser.add_argument("--summary-out", type=Path, default=DEFAULT_REPORT_MD)
    parser.add_argument("--check", action="store_true", help="enforce the checked baseline")
    parser.add_argument(
        "--write-baseline",
        action="store_true",
        help="replace the baseline with limits derived from the current tree",
    )
    parser.add_argument(
        "--verify-determinism",
        action="store_true",
        help="collect twice and fail unless canonical JSON is identical",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        inventory = collect_inventory()
        previous_baseline = load_baseline(args.baseline)
        classification = load_source_classification(args.source_classification)
        classification_failures = classify_raw_source_accesses(
            inventory, classification
        )
        if args.verify_determinism:
            repeated = collect_inventory()
            repeated_failures = classify_raw_source_accesses(repeated, classification)
            if repeated_failures != classification_failures:
                raise InventoryError(
                    "two consecutive source-text classifications produced different failures"
                )
            if canonical_json(inventory) != canonical_json(repeated):
                raise InventoryError("two consecutive inventory runs produced different JSON")
        if args.write_baseline:
            write_report(
                args.baseline,
                canonical_json(baseline_for(inventory, previous_baseline)),
            )
        failures = classification_failures
        if args.check:
            failures.extend(check_baseline(inventory, load_baseline(args.baseline)))
        write_report(args.json_out, canonical_json(inventory))
        write_report(args.summary_out, render_markdown(inventory, failures))
    except InventoryError as error:
        print(f"[fail] architecture inventory: {error}", file=sys.stderr)
        return 1
    if failures:
        print("[fail] architecture inventory regressions:", file=sys.stderr)
        for failure in failures:
            print(f"  - {failure}", file=sys.stderr)
        print(f"Report: {args.summary_out.relative_to(ROOT)}", file=sys.stderr)
        return 1
    print(
        "[ok] architecture inventory wrote "
        f"{args.json_out.relative_to(ROOT)} and {args.summary_out.relative_to(ROOT)}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
