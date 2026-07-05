#!/usr/bin/env python3
"""Mine adjacent dense-bytecode opcode patterns for superinstruction work."""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from collections import Counter
from dataclasses import dataclass
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_ENGINE = ROOT / "target/debug/php-vm"
DEFAULT_OUT_DIR = ROOT / "target/performance/superinstructions"
DEFAULT_FIXTURES = (
    ROOT / "fixtures/runtime/valid/hello.php",
    ROOT / "fixtures/runtime/valid/scalars/echo.php",
    ROOT / "fixtures/runtime/valid/scalars/expressions.php",
    ROOT / "fixtures/runtime/valid/variables/assignment.php",
    ROOT / "fixtures/runtime/valid/functions/simple.php",
    ROOT / "fixtures/runtime/valid/functions/two-args.php",
    ROOT / "fixtures/bytecode/literals/valid/echo-int.php",
    ROOT / "fixtures/bytecode/literals/valid/echo-multiple.php",
    ROOT / "tests/fixtures/performance/perf_smoke/arrays_packed.php",
    ROOT / "tests/fixtures/performance/framework_smoke/packed_mixed_array_traversal.php",
)
CHOSEN_FUSIONS = {
    "load_const echo": "load_const_echo",
    "load_local echo": "load_local_echo",
    "binary_concat echo": "binary_concat_echo",
    "load_const fetch_dim": "load_const_fetch_dim",
    "load_local load_const": "load_local_load_const",
    "call_function discard": "call_function_discard",
    "load_const load_const": "load_const_load_const",
    "load_const array_insert": "load_const_array_insert",
}


@dataclass(frozen=True)
class FixtureReport:
    fixture: str
    functions: int
    blocks: int
    instructions: int
    pairs: dict[str, int]
    triples: dict[str, int]


def rel(path: Path) -> str:
    try:
        return path.resolve().relative_to(ROOT).as_posix()
    except ValueError:
        return path.as_posix()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--engine", type=Path, default=DEFAULT_ENGINE)
    parser.add_argument("--out-dir", type=Path, default=DEFAULT_OUT_DIR)
    parser.add_argument("--summary-doc", type=Path)
    parser.add_argument("--fixture", action="append", type=Path, default=[])
    parser.add_argument("--top", type=int, default=12)
    return parser.parse_args()


def resolved_fixtures(extra: list[Path]) -> list[Path]:
    fixtures = list(DEFAULT_FIXTURES)
    fixtures.extend(extra)
    resolved: list[Path] = []
    seen: set[Path] = set()
    for fixture in fixtures:
        path = fixture if fixture.is_absolute() else ROOT / fixture
        path = path.resolve()
        if path in seen:
            continue
        if not path.is_file():
            raise SystemExit(f"missing superinstruction pattern fixture: {path}")
        seen.add(path)
        resolved.append(path)
    return resolved


def normalized_env() -> dict[str, str]:
    env = dict(os.environ)
    env.update(
        {
            "TZ": "UTC",
            "LC_ALL": "C",
            "LANG": "C",
            "PHRUST_RANDOM_SEED": "superinstruction-patterns",
            "RUST_TEST_SEED": "superinstruction-patterns",
        }
    )
    return env


def require_int_map(path: str, key: str, value: Any) -> dict[str, int]:
    if not isinstance(value, dict):
        raise SystemExit(f"{path}: {key} must be an object")
    result: dict[str, int] = {}
    for item_key, item_value in value.items():
        if not isinstance(item_key, str):
            raise SystemExit(f"{path}: {key} contains a non-string key")
        if not isinstance(item_value, int) or item_value < 0:
            raise SystemExit(f"{path}: {key}.{item_key} must be a non-negative integer")
        result[item_key] = item_value
    return result


def run_fixture(engine: Path, fixture: Path, out_dir: Path) -> FixtureReport:
    stem = rel(fixture).replace("/", "__")
    stdout_path = out_dir / f"{stem}.patterns.json"
    stderr_path = out_dir / f"{stem}.patterns.stderr"
    completed = subprocess.run(
        [str(engine), "dump-bytecode-patterns", rel(fixture), "--json"],
        cwd=ROOT,
        env=normalized_env(),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    stdout_path.write_text(completed.stdout, encoding="utf-8")
    stderr_path.write_text(completed.stderr, encoding="utf-8")
    if completed.returncode != 0:
        raise SystemExit(
            f"[error] strict dense pattern mining failed for {rel(fixture)}: "
            f"exit={completed.returncode}; see {rel(stderr_path)}"
        )
    data = json.loads(completed.stdout)
    if not isinstance(data, dict) or data.get("ok") is not True:
        raise SystemExit(f"{rel(stdout_path)}: expected ok JSON object")
    for key in ("functions", "blocks", "instructions"):
        if not isinstance(data.get(key), int) or data[key] < 0:
            raise SystemExit(f"{rel(stdout_path)}: {key} must be a non-negative integer")
    return FixtureReport(
        fixture=rel(fixture),
        functions=data["functions"],
        blocks=data["blocks"],
        instructions=data["instructions"],
        pairs=require_int_map(rel(stdout_path), "pairs", data.get("pairs")),
        triples=require_int_map(rel(stdout_path), "triples", data.get("triples")),
    )


def top_items(counter: Counter[str], limit: int) -> list[dict[str, Any]]:
    return [
        {"pattern": pattern, "count": count}
        for pattern, count in sorted(counter.items(), key=lambda item: (-item[1], item[0]))[:limit]
    ]


def aggregate(reports: list[FixtureReport], top: int) -> dict[str, Any]:
    pair_counts: Counter[str] = Counter()
    triple_counts: Counter[str] = Counter()
    for report in reports:
        pair_counts.update(report.pairs)
        triple_counts.update(report.triples)
    chosen = [
        {
            "pattern": pattern,
            "superinstruction": opcode,
            "observed_count": pair_counts.get(pattern, 0),
            "reason": "single-block producer immediately consumed by the fused successor, executed through the unfused arm's helper sequence",
        }
        for pattern, opcode in CHOSEN_FUSIONS.items()
    ]
    deferred = [
        {
            "family": "compare_plus_conditional_jump",
            "reason": "current dense blocks keep conditional jumps as terminators; terminator fusion needs separate source-map and branch accounting",
        },
        {
            "family": "binary_plus_store_chain",
            "reason": "store_local is almost always followed by discard, which already fuses; a binary/store pair fusion would only trade one fusion for another on the dominant triple",
        },
        {
            "family": "call_or_builtin_plus_branch_or_echo",
            "reason": "call-plus-branch/echo helpers preserve named-argument, by-reference, diagnostic, and fallback behavior and need dedicated fused helper proof; call-plus-discard is fused via the shared call arm",
        },
        {
            "family": "array_or_foreach_loop_skeleton",
            "reason": "array/foreach state carries COW, reference, mutation, and loop-control semantics that should remain unfused until guarded fixtures exist",
        },
        {
            "family": "property_fetch_plus_echo",
            "reason": "property fetch dense arms carry inline-cache observation and guard state whose fused re-entry accounting is not yet factored",
        },
    ]
    return {
        "status": "pass",
        "gate": "superinstruction-patterns",
        "fixture_count": len(reports),
        "fixtures": [report.fixture for report in reports],
        "blocks": sum(report.blocks for report in reports),
        "instructions": sum(report.instructions for report in reports),
        "top_pairs": top_items(pair_counts, top),
        "top_triples": top_items(triple_counts, top),
        "chosen_fusions": chosen,
        "deferred_families": deferred,
    }


def render_markdown(summary: dict[str, Any]) -> str:
    lines = [
        "# Superinstruction Pattern Report",
        "",
        "Generated by `nix develop -c just superinstruction-patterns`.",
        "Raw per-fixture JSON and stderr artifacts stay local under",
        "`target/performance/superinstructions/`.",
        "",
        "Pattern mining uses strict dense-bytecode lowering before superinstruction",
        "selection and counts adjacent opcode pairs/triples only within a dense basic",
        "block, so patterns never cross branch or return boundaries.",
        "",
        "## Summary",
        "",
        "| Field | Value |",
        "| --- | ---: |",
        f"| Fixtures | {summary['fixture_count']} |",
        f"| Dense blocks | {summary['blocks']} |",
        f"| Dense instructions | {summary['instructions']} |",
        "",
        "## Top Adjacent Pairs",
        "",
        "| Rank | Pair | Count |",
        "| ---: | --- | ---: |",
    ]
    for index, item in enumerate(summary["top_pairs"], start=1):
        lines.append(f"| {index} | `{item['pattern']}` | {item['count']} |")
    lines.extend(
        [
            "",
            "## Top Adjacent Triples",
            "",
            "| Rank | Triple | Count |",
            "| ---: | --- | ---: |",
        ]
    )
    for index, item in enumerate(summary["top_triples"], start=1):
        lines.append(f"| {index} | `{item['pattern']}` | {item['count']} |")
    lines.extend(
        [
            "",
            "## Chosen Fusion Set",
            "",
            "| Pattern | Superinstruction | Observed count | Reason |",
            "| --- | --- | ---: | --- |",
        ]
    )
    for item in summary["chosen_fusions"]:
        lines.append(
            f"| `{item['pattern']}` | `{item['superinstruction']}` | "
            f"{item['observed_count']} | {item['reason']} |"
        )
    lines.extend(
        [
            "",
            "## Deferred Families",
            "",
            "| Family | Reason |",
            "| --- | --- |",
        ]
    )
    for item in summary["deferred_families"]:
        lines.append(f"| `{item['family']}` | {item['reason']} |")
    lines.extend(
        [
            "",
            "## Correctness Policy",
            "",
            "`--superinstructions=off` remains the generic dense-bytecode baseline.",
            "`--superinstructions=on` is the managed fast runtime default and must match stdout,",
            "stderr/runtime diagnostics, and exit status for every smoke fixture.",
            "",
        ]
    )
    return "\n".join(lines)


def main() -> int:
    args = parse_args()
    engine = args.engine if args.engine.is_absolute() else ROOT / args.engine
    if not engine.is_file() or not os.access(engine, os.X_OK):
        raise SystemExit(f"engine is not executable: {engine}")
    out_dir = args.out_dir if args.out_dir.is_absolute() else ROOT / args.out_dir
    out_dir.mkdir(parents=True, exist_ok=True)
    reports = [run_fixture(engine, fixture, out_dir) for fixture in resolved_fixtures(args.fixture)]
    summary = aggregate(reports, max(args.top, 1))
    json_path = out_dir / "patterns.json"
    markdown_path = out_dir / "patterns.md"
    json_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    markdown = render_markdown(summary)
    markdown_path.write_text(markdown, encoding="utf-8")
    if args.summary_doc is not None:
        summary_doc = args.summary_doc if args.summary_doc.is_absolute() else ROOT / args.summary_doc
        summary_doc.write_text(markdown, encoding="utf-8")
    print(
        "[pass] superinstruction pattern mining analyzed "
        f"{summary['fixture_count']} fixture(s), {summary['blocks']} dense block(s), "
        f"and wrote {rel(json_path)}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
