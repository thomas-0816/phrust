#!/usr/bin/env python3
"""Classify external ARM64 samples into exclusive CPU mechanisms."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import random
import re
import statistics
import sys
import tomllib
from collections import Counter, defaultdict
from dataclasses import dataclass
from pathlib import Path
from typing import Any


SCRIPT_DIR = Path(__file__).resolve().parent
DEFAULT_TAXONOMY = SCRIPT_DIR / "arm64_stack_taxonomy.toml"
DEFAULT_RUN_PARENT = Path("target/performance/arm64-work-accounting")
FIXTURE_PATH = SCRIPT_DIR / "fixtures/arm64_sample/classifier_cases.json"
UNKNOWN_CATEGORIES = {"unclassified_phrust", "unresolved"}
STATE_RULES = {"idle", "unresolved", "fallback"}
TOP_LIMIT = 10
BOOTSTRAP_REPLICATES = 10_000


def read_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def write_json(value: Any, path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def compile_optional(value: str | None) -> re.Pattern[str] | None:
    return re.compile(value) if value is not None else None


@dataclass(frozen=True)
class Rule:
    id: str
    category: str
    target: str
    state: str | None
    module_regex: re.Pattern[str] | None
    symbol_regex: re.Pattern[str] | None
    source_regex: re.Pattern[str] | None

    @classmethod
    def from_dict(cls, value: dict[str, Any]) -> "Rule":
        return cls(
            id=value["id"],
            category=value["category"],
            target=value["target"],
            state=value.get("state"),
            module_regex=compile_optional(value.get("module_regex")),
            symbol_regex=compile_optional(value.get("symbol_regex")),
            source_regex=compile_optional(value.get("source_regex")),
        )

    def matches_frame(self, frame: dict[str, Any]) -> bool:
        fields = (
            (self.module_regex, frame.get("module") or ""),
            (self.symbol_regex, frame.get("symbol") or ""),
            (self.source_regex, frame.get("source") or ""),
        )
        configured = False
        for pattern, value in fields:
            if pattern is None:
                continue
            configured = True
            if pattern.search(value) is None:
                return False
        return configured


@dataclass(frozen=True)
class OriginRule:
    id: str
    module_regex: re.Pattern[str] | None
    symbol_regex: re.Pattern[str] | None
    source_regex: re.Pattern[str] | None

    @classmethod
    def from_dict(cls, value: dict[str, Any]) -> "OriginRule":
        return cls(
            id=value["id"],
            module_regex=compile_optional(value.get("module_regex")),
            symbol_regex=compile_optional(value.get("symbol_regex")),
            source_regex=compile_optional(value.get("source_regex")),
        )

    def matches_frame(self, frame: dict[str, Any]) -> bool:
        fields = (
            (self.module_regex, frame.get("module") or ""),
            (self.symbol_regex, frame.get("symbol") or ""),
            (self.source_regex, frame.get("source") or ""),
        )
        configured = False
        for pattern, value in fields:
            if pattern is None:
                continue
            configured = True
            if pattern.search(value) is None:
                return False
        return configured


@dataclass(frozen=True)
class Taxonomy:
    path: Path
    schema_version: int
    worker_thread: str
    categories: dict[str, dict[str, Any]]
    rules: tuple[Rule, ...]
    origins: tuple[OriginRule, ...]
    phrust_module_regex: re.Pattern[str]
    phrust_owner_symbol_regex: re.Pattern[str]
    generic_rust_symbol_regex: re.Pattern[str]
    idle_symbol_regex: re.Pattern[str]

    @classmethod
    def load(cls, path: Path) -> "Taxonomy":
        with path.open("rb") as handle:
            raw = tomllib.load(handle)
        taxonomy = cls(
            path=path.resolve(),
            schema_version=int(raw["schema_version"]),
            worker_thread=raw["worker_thread"],
            categories=raw.get("categories", {}),
            rules=tuple(Rule.from_dict(value) for value in raw.get("rules", [])),
            origins=tuple(
                OriginRule.from_dict(value) for value in raw.get("origins", [])
            ),
            phrust_module_regex=re.compile(raw["phrust_module_regex"]),
            phrust_owner_symbol_regex=re.compile(raw["phrust_owner_symbol_regex"]),
            generic_rust_symbol_regex=re.compile(raw["generic_rust_symbol_regex"]),
            idle_symbol_regex=re.compile(raw["idle_symbol_regex"]),
        )
        taxonomy.validate()
        return taxonomy

    def validate(self) -> None:
        if not self.categories:
            raise ValueError("taxonomy has no categories")
        rule_ids = [rule.id for rule in self.rules]
        if len(rule_ids) != len(set(rule_ids)):
            raise ValueError("taxonomy rule ids must be unique")
        unknown = sorted({rule.category for rule in self.rules} - set(self.categories))
        if unknown:
            raise ValueError(
                f"rules reference unknown categories: {', '.join(unknown)}"
            )
        covered = {rule.category for rule in self.rules}
        missing = sorted(set(self.categories) - covered)
        if missing:
            raise ValueError(f"categories without rule ids: {', '.join(missing)}")
        for rule in self.rules:
            if rule.target not in {"state", "leaf", "owner", "stack"}:
                raise ValueError(f"rule {rule.id} has unsupported target {rule.target}")
            if rule.target == "state" and rule.state not in STATE_RULES:
                raise ValueError(f"rule {rule.id} has unsupported state {rule.state}")


def frame_signature(frame: dict[str, Any]) -> str:
    module = frame.get("module") or "unknown"
    symbol = frame.get("symbol") or "???"
    return f"{module}`{symbol}"


def stack_is_idle(stack: dict[str, Any], taxonomy: Taxonomy) -> bool:
    return any(
        taxonomy.idle_symbol_regex.search(frame.get("symbol") or "") is not None
        for frame in stack["frames"]
    )


def stack_is_unresolved(stack: dict[str, Any]) -> bool:
    leaf = stack["frames"][-1]
    symbol = leaf.get("symbol") or ""
    return (
        leaf.get("module") is None or symbol in {"", "???"} or symbol.startswith("0x")
    )


def closest_phrust_owner(
    stack: dict[str, Any], taxonomy: Taxonomy
) -> dict[str, Any] | None:
    frames = stack["frames"]
    leaf_symbol = frames[-1].get("symbol") or ""
    skip_leaf = taxonomy.generic_rust_symbol_regex.search(leaf_symbol) is not None
    owner_frames = frames[:-1] if skip_leaf else frames
    for frame in reversed(owner_frames):
        module = frame.get("module") or ""
        symbol = frame.get("symbol") or ""
        if (
            taxonomy.phrust_module_regex.search(module) is not None
            and taxonomy.phrust_owner_symbol_regex.search(symbol) is not None
        ):
            return frame
    return None


def matching_origins(stack: dict[str, Any], taxonomy: Taxonomy) -> list[str]:
    return [
        origin.id
        for origin in taxonomy.origins
        if any(origin.matches_frame(frame) for frame in stack["frames"])
    ]


def classify_stack(stack: dict[str, Any], taxonomy: Taxonomy) -> dict[str, Any]:
    if not stack.get("frames"):
        raise ValueError("stack has no frames")
    idle = stack_is_idle(stack, taxonomy)
    unresolved = stack_is_unresolved(stack)
    leaf = stack["frames"][-1]
    owner = closest_phrust_owner(stack, taxonomy)
    for rule in taxonomy.rules:
        matched = False
        if rule.target == "state":
            matched = (
                (rule.state == "idle" and idle)
                or (rule.state == "unresolved" and not idle and unresolved)
                or (rule.state == "fallback" and not idle and not unresolved)
            )
        elif not idle and not unresolved and rule.target == "leaf":
            matched = rule.matches_frame(leaf)
        elif (
            not idle and not unresolved and rule.target == "owner" and owner is not None
        ):
            matched = rule.matches_frame(owner)
        elif not idle and not unresolved and rule.target == "stack":
            matched = any(rule.matches_frame(frame) for frame in stack["frames"])
        if matched:
            return {
                "category": rule.category,
                "rule_id": rule.id,
                "leaf": leaf,
                "owner": owner,
                "origins": matching_origins(stack, taxonomy),
                "idle": idle,
            }
    raise ValueError(f"taxonomy has no fallback rule for {frame_signature(leaf)}")


def allocate_basis_points(counts: dict[str, int], total: int) -> dict[str, int]:
    if total == 0:
        return {category: 0 for category in counts}
    floors: dict[str, int] = {}
    remainders: list[tuple[int, str]] = []
    for category, count in counts.items():
        numerator = count * 10_000
        floors[category] = numerator // total
        remainders.append((numerator % total, category))
    missing = 10_000 - sum(floors.values())
    for _, category in sorted(remainders, key=lambda item: (-item[0], item[1]))[
        :missing
    ]:
        floors[category] += 1
    return floors


def top_counter(counter: Counter[str], limit: int = TOP_LIMIT) -> list[dict[str, Any]]:
    return [
        {"name": name, "samples": count}
        for name, count in sorted(
            counter.items(), key=lambda item: (-item[1], item[0])
        )[:limit]
    ]


def signature_entry(
    stack: dict[str, Any], classification: dict[str, Any]
) -> dict[str, Any]:
    return {
        "category": classification["category"],
        "folded": stack["folded"],
        "leaf": classification["leaf"],
        "owner": classification["owner"],
        "origins": classification["origins"],
        "rule_id": classification["rule_id"],
        "weight": int(stack["weight"]),
    }


def classify_window(
    folded: dict[str, Any], taxonomy: Taxonomy, window_number: int
) -> tuple[dict[str, Any], list[dict[str, Any]], dict[str, Any]]:
    category_counts = Counter({category: 0 for category in taxonomy.categories})
    category_rules: dict[str, Counter[str]] = defaultdict(Counter)
    category_leaves: dict[str, Counter[str]] = defaultdict(Counter)
    category_origins: dict[str, Counter[str]] = defaultdict(Counter)
    category_stacks: dict[str, Counter[str]] = defaultdict(Counter)
    global_leaves: Counter[str] = Counter()
    global_origins: Counter[str] = Counter()
    unexplained: list[dict[str, Any]] = []
    classified_stacks: list[dict[str, Any]] = []
    idle_samples = 0
    active_samples = 0

    for stack_index, stack in enumerate(folded["stacks"]):
        if stack.get("thread_name") != taxonomy.worker_thread:
            continue
        classification = classify_stack(stack, taxonomy)
        weight = int(stack["weight"])
        category = classification["category"]
        classified_stacks.append(
            {
                "category": category,
                "folded_stack_index": stack_index,
                "idle": classification["idle"],
                "leaf": frame_signature(classification["leaf"]),
                "origins": classification["origins"],
                "owner": (
                    frame_signature(classification["owner"])
                    if classification["owner"] is not None
                    else None
                ),
                "rule_id": classification["rule_id"],
                "weight": weight,
            }
        )
        if classification["idle"]:
            idle_samples += weight
            category_counts[category] += weight
            category_rules[category][classification["rule_id"]] += weight
            continue
        active_samples += weight
        category_counts[category] += weight
        category_rules[category][classification["rule_id"]] += weight
        leaf_name = frame_signature(classification["leaf"])
        category_leaves[category][leaf_name] += weight
        category_stacks[category][stack["folded"]] += weight
        global_leaves[leaf_name] += weight
        for origin in classification["origins"]:
            category_origins[category][origin] += weight
            global_origins[origin] += weight
        if category in UNKNOWN_CATEGORIES:
            entry = signature_entry(stack, classification)
            entry["window"] = window_number
            unexplained.append(entry)

    active_counts = {
        category: count
        for category, count in category_counts.items()
        if category != "idle"
    }
    basis_points = allocate_basis_points(active_counts, active_samples)
    categories = {}
    for category in taxonomy.categories:
        count = active_counts.get(category, 0)
        categories[category] = {
            "samples": count,
            "share_basis_points": basis_points.get(category, 0),
            "share_percent": basis_points.get(category, 0) / 100.0,
            "rule_ids": sorted(category_rules[category]),
        }
    result = {
        "window": window_number,
        "active_worker_samples": active_samples,
        "idle_worker_samples": idle_samples,
        "primary_categories": categories,
        "top_leaf_symbols": top_counter(global_leaves),
        "top_caller_origins": top_counter(global_origins),
        "unclassified_or_unresolved_signatures": sorted(
            unexplained, key=lambda item: (-item["weight"], item["folded"])
        )[:TOP_LIMIT],
    }
    internals = {
        "classified_stacks": classified_stacks,
        "rules": category_rules,
        "leaves": category_leaves,
        "origins": category_origins,
        "stacks": category_stacks,
    }
    return result, unexplained, internals


def percentile(values: list[float], quantile: float) -> float:
    if not values:
        return 0.0
    position = (len(values) - 1) * quantile
    lower = math.floor(position)
    upper = math.ceil(position)
    if lower == upper:
        return values[lower]
    fraction = position - lower
    return values[lower] * (1.0 - fraction) + values[upper] * fraction


def bootstrap_interval(
    category: str, windows: list[dict[str, Any]], seed: int
) -> dict[str, float | int | str]:
    rng = random.Random(seed)
    shares = []
    for _ in range(BOOTSTRAP_REPLICATES):
        selected = [rng.randrange(len(windows)) for _ in windows]
        samples = sum(
            windows[index]["primary_categories"][category]["samples"]
            for index in selected
        )
        total = sum(windows[index]["active_worker_samples"] for index in selected)
        shares.append(samples * 100.0 / total if total else 0.0)
    shares.sort()
    return {
        "confidence": 0.95,
        "lower_percent": percentile(shares, 0.025),
        "upper_percent": percentile(shares, 0.975),
        "replicates": BOOTSTRAP_REPLICATES,
        "resampling_unit": "sample_window",
    }


def merge_windows(
    windows: list[dict[str, Any]], internals: list[dict[str, Any]], taxonomy: Taxonomy
) -> dict[str, Any]:
    total = sum(window["active_worker_samples"] for window in windows)
    counts = {
        category: sum(
            window["primary_categories"][category]["samples"] for window in windows
        )
        for category in taxonomy.categories
        if category != "idle"
    }
    basis_points = allocate_basis_points(counts, total)
    categories = {}
    for category in taxonomy.categories:
        configured_rules = sorted(
            rule.id for rule in taxonomy.rules if rule.category == category
        )
        if category == "idle":
            categories[category] = {
                "samples": sum(window["idle_worker_samples"] for window in windows),
                "share_basis_points": 0,
                "share_percent": None,
                "window_statistics": None,
                "bootstrap_interval": None,
                "rule_ids": configured_rules,
            }
            continue
        shares = [
            window["primary_categories"][category]["share_percent"]
            for window in windows
        ]
        categories[category] = {
            "samples": counts[category],
            "share_basis_points": basis_points[category],
            "share_percent": basis_points[category] / 100.0,
            "window_statistics": {
                "minimum_percent": min(shares),
                "maximum_percent": max(shares),
                "mean_percent": statistics.fmean(shares),
                "standard_deviation_percent": statistics.pstdev(shares),
            },
            "bootstrap_interval": bootstrap_interval(
                category,
                windows,
                seed=int(hashlib.sha256(category.encode()).hexdigest()[:8], 16),
            ),
            "rule_ids": configured_rules,
        }

    evidence = {}
    for category in taxonomy.categories:
        leaves: Counter[str] = Counter()
        origins: Counter[str] = Counter()
        stacks: Counter[str] = Counter()
        for internal in internals:
            leaves.update(internal["leaves"][category])
            origins.update(internal["origins"][category])
            stacks.update(internal["stacks"][category])
        evidence[category] = {
            "top_leaf_symbols": top_counter(leaves),
            "top_caller_origins": top_counter(origins),
            "top_stacks": top_counter(stacks),
        }
    return {
        "active_worker_samples": total,
        "idle_worker_samples": sum(window["idle_worker_samples"] for window in windows),
        "primary_categories": categories,
        "evidence": evidence,
    }


def gate_check(name: str, passed: bool, detail: str) -> dict[str, Any]:
    return {"name": name, "passed": bool(passed), "detail": detail}


def actionable_coverage_passes(
    category_counts: dict[str, int], active_samples: int
) -> bool:
    unknown = sum(category_counts.get(category, 0) for category in UNKNOWN_CATEGORIES)
    return (
        active_samples > 0 and (active_samples - unknown) * 100 >= active_samples * 90
    )


def identity_check(
    summary: dict[str, Any], folded_reports: list[dict[str, Any]], run_root: Path
) -> tuple[bool, str]:
    identity = summary.get("identity", {})
    commit = identity.get("git", {}).get("commit")
    binary_hash = identity.get("phrust", {}).get("binary_sha256")
    sampler = summary.get("sampler", {})
    binary = Path(sampler.get("binary", "")).resolve()
    windows = sampler.get("windows", [])
    if not commit or not binary_hash or len(windows) != 3 or len(folded_reports) != 3:
        return False, "missing source, binary, or three-window identity"
    for window, folded in zip(windows, folded_reports):
        raw = Path(window.get("raw", "")).resolve()
        folded_path = Path(window.get("folded", "")).resolve()
        if not raw.is_file() or not folded_path.is_file():
            return (
                False,
                f"missing raw/folded evidence for window {window.get('window')}",
            )
        if Path(folded.get("source", "")).resolve() != raw:
            return False, f"folded source mismatch for window {window.get('window')}"
        symbol_binary = folded.get("symbolization", {}).get("binary")
        if symbol_binary is not None and Path(symbol_binary).resolve() != binary:
            return (
                False,
                f"symbolization binary mismatch for window {window.get('window')}",
            )
        if (
            run_root.resolve() not in raw.parents
            or run_root.resolve() not in folded_path.parents
        ):
            return False, f"window {window.get('window')} escapes run root"
    return True, f"commit {commit[:12]}, binary {binary_hash[:12]}, three windows"


def evaluate_gate(
    accounting: dict[str, Any],
    summary: dict[str, Any],
    taxonomy: Taxonomy,
    folded_reports: list[dict[str, Any]],
    run_root: Path,
) -> list[dict[str, Any]]:
    pooled = accounting["pooled"]
    categories = pooled["primary_categories"]
    active = pooled["active_worker_samples"]
    primary_sum = sum(
        value["samples"] for category, value in categories.items() if category != "idle"
    )
    basis_sum = sum(
        value["share_basis_points"]
        for category, value in categories.items()
        if category != "idle"
    )
    unknown_samples = sum(categories[name]["samples"] for name in UNKNOWN_CATEGORIES)
    unknown_basis = sum(
        categories[name]["share_basis_points"] for name in UNKNOWN_CATEGORIES
    )
    actionable = active - unknown_samples
    evidence_missing = [
        category
        for category, value in categories.items()
        if category != "idle"
        and value["samples"] > 0
        and not pooled["evidence"][category]["top_stacks"]
    ]
    expected_commit = summary.get("identity", {}).get("git", {}).get("commit")
    expected_binary_hash = (
        summary.get("identity", {}).get("phrust", {}).get("binary_sha256")
    )
    expected_taxonomy_hash = accounting.get("taxonomy", {}).get("sha256")
    expected_folded_paths = {
        int(window["window"]): Path(window["folded"]).resolve()
        for window in summary.get("sampler", {}).get("windows", [])
    }
    ledger_failures = []
    for window in accounting["windows"]:
        metadata = window.get("classified_stacks", {})
        path = Path(metadata.get("path", ""))
        if not path.is_file() or sha256(path) != metadata.get("sha256"):
            ledger_failures.append(
                f"window {window['window']} ledger missing or changed"
            )
            continue
        ledger = read_json(path)
        window_number = int(window["window"])
        if ledger.get("window") != window_number:
            ledger_failures.append(f"window {window_number} ledger number mismatch")
        if ledger.get("source_commit") != expected_commit:
            ledger_failures.append(f"window {window_number} commit mismatch")
        if ledger.get("binary_sha256") != expected_binary_hash:
            ledger_failures.append(f"window {window_number} binary mismatch")
        if ledger.get("taxonomy_sha256") != expected_taxonomy_hash:
            ledger_failures.append(f"window {window_number} taxonomy mismatch")
        source_folded = Path(ledger.get("source_folded", "")).resolve()
        if source_folded != expected_folded_paths.get(window_number):
            ledger_failures.append(f"window {window_number} folded source mismatch")
        if len(ledger.get("stacks", [])) != metadata.get("stack_record_count"):
            ledger_failures.append(f"window {window_number} stack count mismatch")
        active_weight = sum(
            int(stack["weight"]) for stack in ledger["stacks"] if not stack["idle"]
        )
        idle_weight = sum(
            int(stack["weight"]) for stack in ledger["stacks"] if stack["idle"]
        )
        if active_weight != window["active_worker_samples"]:
            ledger_failures.append(f"window {window['window']} active ledger mismatch")
        if idle_weight != window["idle_worker_samples"]:
            ledger_failures.append(f"window {window['window']} idle ledger mismatch")
        if any(not stack.get("rule_id") for stack in ledger["stacks"]):
            ledger_failures.append(f"window {window['window']} stack without rule id")
    identity_ok, identity_detail = identity_check(summary, folded_reports, run_root)
    return [
        gate_check(
            "exclusive-sample-sum",
            primary_sum == active,
            f"primary {primary_sum} of active {active}",
        ),
        gate_check(
            "serialized-share-sum",
            basis_sum == 10_000,
            f"serialized primary share {basis_sum / 100:.2f}%",
        ),
        gate_check(
            "named-actionable-coverage",
            actionable_coverage_passes(
                {category: value["samples"] for category, value in categories.items()},
                active,
            ),
            f"named actionable {actionable * 100.0 / active if active else 0.0:.1f}%",
        ),
        gate_check(
            "unknown-share-limit",
            unknown_basis <= 1_000,
            f"unclassified plus unresolved {unknown_basis / 100:.2f}%",
        ),
        gate_check(
            "category-rule-coverage",
            all(
                any(rule.category == category for rule in taxonomy.rules)
                for category in taxonomy.categories
            ),
            "every category has at least one configured rule id",
        ),
        gate_check(
            "raw-stack-evidence",
            not evidence_missing,
            "all nonzero categories have top-stack evidence"
            if not evidence_missing
            else "missing: " + ", ".join(evidence_missing),
        ),
        gate_check(
            "per-stack-rule-ledger",
            not ledger_failures,
            "every weighted worker stack has one rule id"
            if not ledger_failures
            else "; ".join(ledger_failures),
        ),
        gate_check("three-window-identity", identity_ok, identity_detail),
        gate_check(
            "host-uncontaminated",
            not summary.get("host_failures"),
            "no host blockers"
            if not summary.get("host_failures")
            else "; ".join(summary["host_failures"]),
        ),
        gate_check(
            "sampler-accepted",
            summary.get("status") == "pass"
            and summary.get("sampler", {}).get("stable_sample_target_met") is True,
            f"sampler status {summary.get('status')}",
        ),
    ]


def render_markdown(summary: dict[str, Any], accounting: dict[str, Any]) -> str:
    pooled = accounting["pooled"]
    lines = [
        "# ARM64 Exclusive CPU Accounting",
        "",
        f"Status: `{accounting['gate']['status']}`",
        "",
        f"- source commit: `{summary['identity']['git']['commit']}`",
        f"- binary SHA-256: `{summary['identity']['phrust']['binary_sha256']}`",
        f"- active `php-worker-0` samples: `{pooled['active_worker_samples']}`",
        f"- idle worker samples excluded: `{pooled['idle_worker_samples']}`",
        "- primary view: exclusive leaf ownership; sums to exactly 100.00%",
        "- secondary caller origins: overlapping diagnostic labels; do not sum",
        "",
        "| category | samples | share | window range | top leaf symbols | top caller origins |",
        "| --- | ---: | ---: | ---: | --- | --- |",
    ]
    for category, value in pooled["primary_categories"].items():
        if category == "idle":
            continue
        stats = value["window_statistics"]
        evidence = pooled["evidence"][category]
        leaves = (
            ", ".join(item["name"] for item in evidence["top_leaf_symbols"][:3]) or "-"
        )
        origins = (
            ", ".join(item["name"] for item in evidence["top_caller_origins"][:3])
            or "-"
        )
        lines.append(
            f"| `{category}` | {value['samples']} | {value['share_percent']:.1f}% | "
            f"{stats['minimum_percent']:.1f}-{stats['maximum_percent']:.1f}% | {leaves} | {origins} |"
        )
    lines.extend(["", "## Windows", ""])
    for window in accounting["windows"]:
        top_leaves = ", ".join(item["name"] for item in window["top_leaf_symbols"][:3])
        top_origins = ", ".join(
            item["name"] for item in window["top_caller_origins"][:3]
        )
        lines.append(
            f"- window {window['window']}: {window['active_worker_samples']} active samples; "
            f"top leaves: {top_leaves or '-'}; caller origins: {top_origins or '-'}"
        )
    lines.extend(["", "## Gate", ""])
    for check in accounting["gate"]["checks"]:
        marker = "pass" if check["passed"] else "fail"
        lines.append(f"- `{marker}` `{check['name']}`: {check['detail']}")
    return "\n".join(lines) + "\n"


def run(run_root: Path, taxonomy: Taxonomy) -> dict[str, Any]:
    summary_path = run_root / "sampler/sampler-summary.json"
    summary = read_json(summary_path)
    folded_paths = [Path(window["folded"]) for window in summary["sampler"]["windows"]]
    raw_hashes_before = {
        Path(window["raw"]).resolve(): sha256(Path(window["raw"]))
        for window in summary["sampler"]["windows"]
    }
    folded_reports = [read_json(path) for path in folded_paths]
    windows = []
    unexplained = []
    internals = []
    for index, folded in enumerate(folded_reports, start=1):
        window, window_unexplained, internal = classify_window(folded, taxonomy, index)
        windows.append(window)
        unexplained.extend(window_unexplained)
        internals.append(internal)
    taxonomy_hash = sha256(taxonomy.path)
    for window, internal, folded_path in zip(windows, internals, folded_paths):
        classified_path = (
            run_root / "sampler" / f"window-{window['window']:02d}.classified.json"
        )
        classified_report = {
            "schema_version": 1,
            "binary_sha256": summary["identity"]["phrust"]["binary_sha256"],
            "source_commit": summary["identity"]["git"]["commit"],
            "source_folded": str(folded_path.resolve()),
            "taxonomy_sha256": taxonomy_hash,
            "window": window["window"],
            "stacks": internal["classified_stacks"],
        }
        write_json(classified_report, classified_path)
        window["classified_stacks"] = {
            "path": str(classified_path.resolve()),
            "sha256": sha256(classified_path),
            "stack_record_count": len(internal["classified_stacks"]),
        }
    pooled = merge_windows(windows, internals, taxonomy)
    accounting = {
        "schema_version": 1,
        "view": "exclusive-leaf-primary-with-overlapping-secondary-origins",
        "taxonomy": {
            "path": str(taxonomy.path),
            "sha256": taxonomy_hash,
            "schema_version": taxonomy.schema_version,
        },
        "windows": windows,
        "pooled": pooled,
    }
    checks = evaluate_gate(accounting, summary, taxonomy, folded_reports, run_root)
    accounting["gate"] = {
        "status": "pass" if all(check["passed"] for check in checks) else "fail",
        "checks": checks,
    }
    unexplained_report = {
        "schema_version": 1,
        "active_worker_samples": pooled["active_worker_samples"],
        "unclassified_or_unresolved_samples": sum(
            pooled["primary_categories"][category]["samples"]
            for category in UNKNOWN_CATEGORIES
        ),
        "stacks": sorted(
            unexplained,
            key=lambda item: (-item["weight"], item["window"], item["folded"]),
        ),
    }
    raw_hashes_after = {path: sha256(path) for path in raw_hashes_before}
    if raw_hashes_before != raw_hashes_after:
        raise RuntimeError("raw sampler evidence changed during classification")
    summary["accounting"] = accounting
    write_json(summary, summary_path)
    write_json(unexplained_report, run_root / "unclassified-stacks.json")
    (run_root / "sampler/sampler-summary.md").write_text(
        render_markdown(summary, accounting), encoding="utf-8"
    )
    return accounting


def discover_run_root() -> Path:
    configured = os.environ.get("PHRUST_ARM64_ACCOUNTING_ROOT")
    if configured:
        return Path(configured)
    candidates = [
        path.parent.parent
        for path in DEFAULT_RUN_PARENT.glob("*/sampler/sampler-summary.json")
        if path.is_file()
    ]
    if not candidates:
        raise FileNotFoundError(
            "no ARM64 sampler run found; pass --run-root or set PHRUST_ARM64_ACCOUNTING_ROOT"
        )
    return max(candidates, key=lambda path: path.stat().st_mtime_ns)


def fixture_stack(case: dict[str, Any]) -> dict[str, Any]:
    frames = [
        {
            "module": frame.get("module"),
            "symbol": frame.get("symbol"),
            "source": frame.get("source"),
            "line": None,
            "address": None,
        }
        for frame in case["frames"]
    ]
    return {
        "thread_id": "fixture",
        "thread_name": "php-worker-0",
        "weight": 1,
        "folded": ";".join(frame_signature(frame) for frame in frames),
        "frames": frames,
        "unresolved": False,
    }


def self_test(taxonomy_path: Path) -> int:
    taxonomy = Taxonomy.load(taxonomy_path)
    fixtures = read_json(FIXTURE_PATH)["cases"]
    active_counts = Counter(
        {category: 0 for category in taxonomy.categories if category != "idle"}
    )
    active = 0
    ambiguous_case = None
    for case in fixtures:
        stack = fixture_stack(case)
        classification = classify_stack(stack, taxonomy)
        assert classification["category"] == case["expected"], (
            case["name"],
            classification,
        )
        if classification["category"] != "idle":
            active += 1
            active_counts[classification["category"]] += 1
        if case.get("gate_must_fail"):
            ambiguous_case = classification
    assert sum(active_counts.values()) == active
    assert sum(allocate_basis_points(dict(active_counts), active).values()) == 10_000
    assert ambiguous_case is not None
    ambiguous_counts = {category: 0 for category in active_counts}
    ambiguous_counts[ambiguous_case["category"]] = 1
    assert not actionable_coverage_passes(ambiguous_counts, 1)

    replacement = Rule(
        id="fixture-mystery-owner",
        category="dense_semantic_helpers",
        target="leaf",
        state=None,
        module_regex=None,
        symbol_regex=re.compile(r"^mystery::opaque_operation$"),
        source_regex=None,
    )
    replaced_taxonomy = Taxonomy(
        path=taxonomy.path,
        schema_version=taxonomy.schema_version,
        worker_thread=taxonomy.worker_thread,
        categories=taxonomy.categories,
        rules=(*taxonomy.rules[:-1], replacement, taxonomy.rules[-1]),
        origins=taxonomy.origins,
        phrust_module_regex=taxonomy.phrust_module_regex,
        phrust_owner_symbol_regex=taxonomy.phrust_owner_symbol_regex,
        generic_rust_symbol_regex=taxonomy.generic_rust_symbol_regex,
        idle_symbol_regex=taxonomy.idle_symbol_regex,
    )
    mystery = next(case for case in fixtures if case.get("gate_must_fail"))
    assert (
        classify_stack(fixture_stack(mystery), replaced_taxonomy)["category"]
        == "dense_semantic_helpers"
    )
    print("arm64 stack classifier self-test: ok")
    return 0


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run-root", type=Path)
    parser.add_argument("--taxonomy", type=Path, default=DEFAULT_TAXONOMY)
    parser.add_argument("--gate", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    if args.self_test:
        return self_test(args.taxonomy)
    run_root = (args.run_root or discover_run_root()).resolve()
    taxonomy = Taxonomy.load(args.taxonomy)
    accounting = run(run_root, taxonomy)
    status = accounting["gate"]["status"]
    print(f"[{status}] wrote {run_root / 'sampler/sampler-summary.md'}")
    return 2 if args.gate and status != "pass" else 0


if __name__ == "__main__":
    sys.exit(main())
