#!/usr/bin/env python3
"""Bounded deterministic fuzz smoke for Cranelift-eligible int-only IR."""

from __future__ import annotations

import argparse
import json
import os
import random
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import jit_bench_matrix


ROOT = Path(__file__).resolve().parents[3]
DEFAULT_ENGINE = ROOT / "target/debug/php-vm"
DEFAULT_OUT = ROOT / "target/phase7/cranelift/fuzz-smoke.json"
DEFAULT_FIXTURE_DIR = ROOT / "target/phase7/cranelift/fuzz/fixtures"
DEFAULT_SEEDS = (0x070C0001, 0x070C0002)
CALLS = ((-3, -1), (-2, 5), (0, 0), (1, 4), (3, 2), (6, -2))


@dataclass(frozen=True)
class GeneratedCase:
    seed: int
    index: int
    name: str
    source: str
    grammar_features: list[str]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--engine", type=Path, default=Path(os.getenv("PHRUST_PHP_VM", DEFAULT_ENGINE)))
    parser.add_argument("--out", type=Path, default=DEFAULT_OUT)
    parser.add_argument("--fixture-dir", type=Path, default=DEFAULT_FIXTURE_DIR)
    parser.add_argument("--cases-per-seed", type=int, default=6)
    parser.add_argument("--timeout", type=float, default=5.0)
    parser.add_argument(
        "--seed",
        action="append",
        type=lambda value: int(value, 0),
        dest="seeds",
        help="Deterministic seed. May be passed more than once.",
    )
    return parser.parse_args()


def rel(path: Path) -> str:
    try:
        return path.resolve().relative_to(ROOT).as_posix()
    except ValueError:
        return path.as_posix()


def env_for(seed: int) -> dict[str, str]:
    env = jit_bench_matrix.normalized_env(ROOT / "target/phase7/cranelift/fuzz/tmp")
    env["PHRUST_RANDOM_SEED"] = f"phase7-cranelift-fuzz-{seed}"
    env["RUST_TEST_SEED"] = f"phase7-cranelift-fuzz-{seed}"
    return env


def atom(rng: random.Random) -> tuple[str, set[str]]:
    choice = rng.randrange(4)
    if choice == 0:
        return "$a", {"params"}
    if choice == 1:
        return "$b", {"params"}
    return str(rng.randint(0, 9)), {"constants"}


def expr(rng: random.Random, depth: int) -> tuple[str, set[str]]:
    if depth <= 0:
        return atom(rng)
    op = rng.choice(("+", "-", "*"))
    lhs, lhs_features = expr(rng, depth - 1)
    rhs, rhs_features = expr(rng, depth - 1)
    return f"({lhs} {op} {rhs})", lhs_features | rhs_features | {op_name(op)}


def op_name(op: str) -> str:
    return {"+": "add", "-": "sub", "*": "mul"}[op]


def comparison(rng: random.Random) -> tuple[str, set[str]]:
    lhs, lhs_features = expr(rng, 1)
    rhs, rhs_features = expr(rng, 1)
    op = rng.choice(("<", "<=", ">", ">=", "===", "!=="))
    return f"{lhs} {op} {rhs}", lhs_features | rhs_features | {"comparisons"}


def generate_case(seed: int, index: int) -> GeneratedCase:
    rng = random.Random((seed << 16) ^ index)
    name = f"phase7_cl_fuzz_{seed:x}_{index}"
    features: set[str] = set()
    body: list[str]
    if index % 2 == 0:
        condition, condition_features = comparison(rng)
        then_expr, then_features = expr(rng, 2)
        else_expr, else_features = expr(rng, 2)
        features |= condition_features | then_features | else_features | {"branches"}
        body = [
            f"    if ({condition}) {{",
            f"        return {then_expr};",
            "    }",
            f"    return {else_expr};",
        ]
    else:
        value, expr_features = expr(rng, 3)
        features |= expr_features
        body = [f"    return {value};"]

    lines = [
        "<?php",
        f"function {name}(int $a, int $b): int",
        "{",
        *body,
        "}",
        "",
    ]
    for a, b in CALLS:
        lines.append(f"echo {name}({a}, {b}), \"\\n\";")
    lines.append("")
    return GeneratedCase(
        seed=seed,
        index=index,
        name=name,
        source="\n".join(lines),
        grammar_features=sorted(features),
    )


def run_engine(engine: Path, fixture: Path, mode: str, seed: int, timeout: float) -> subprocess.CompletedProcess[str]:
    command = [str(engine), "run", f"--jit={mode}"]
    if mode == "cranelift":
        command.extend(["--jit-eager", "--jit-stats=json"])
    command.append(rel(fixture))
    return subprocess.run(
        command,
        cwd=ROOT,
        env=env_for(seed),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
        check=False,
    )


def normalize_stderr(stderr: str) -> str:
    return jit_bench_matrix.stderr_without_jit_stats(jit_bench_matrix.normalize_text(stderr))


def stats_from(stderr: str) -> dict[str, Any] | None:
    return jit_bench_matrix.extract_jit_stats(stderr)


def case_report(
    *,
    case: GeneratedCase,
    fixture: Path,
    off: subprocess.CompletedProcess[str],
    cranelift: subprocess.CompletedProcess[str],
) -> dict[str, Any]:
    stats = stats_from(cranelift.stderr) or {}
    return {
        "seed": case.seed,
        "case_index": case.index,
        "function": case.name,
        "fixture": rel(fixture),
        "grammar_features": case.grammar_features,
        "interpreter_exit": off.returncode,
        "cranelift_exit": cranelift.returncode,
        "stdout_match": jit_bench_matrix.normalize_text(off.stdout)
        == jit_bench_matrix.normalize_text(cranelift.stdout),
        "stderr_match_without_jit_stats": normalize_stderr(off.stderr) == normalize_stderr(cranelift.stderr),
        "compiled": int(stats.get("compiled", 0)),
        "executed": int(stats.get("executed", 0)),
        "side_exits": int(stats.get("side_exits", 0)),
        "side_exit_reasons": stats.get("side_exit_reasons", {}),
        "compile_descriptors": stats.get("compile_descriptors", []),
    }


def main() -> int:
    args = parse_args()
    if not args.engine.is_file() or not os.access(args.engine, os.X_OK):
        raise SystemExit(f"Rust VM is not executable: {args.engine}")
    seeds = tuple(args.seeds) if args.seeds else DEFAULT_SEEDS
    args.fixture_dir.mkdir(parents=True, exist_ok=True)
    cases: list[dict[str, Any]] = []
    failures: list[str] = []

    for seed in seeds:
        for index in range(args.cases_per_seed):
            case = generate_case(seed, index)
            fixture = args.fixture_dir / f"{case.name}.php"
            fixture.write_text(case.source, encoding="utf-8")
            try:
                off = run_engine(args.engine, fixture, "off", seed, args.timeout)
                cranelift = run_engine(args.engine, fixture, "cranelift", seed, args.timeout)
            except subprocess.TimeoutExpired:
                failures.append(f"{case.name}: timed out after {args.timeout}s")
                continue
            report = case_report(case=case, fixture=fixture, off=off, cranelift=cranelift)
            cases.append(report)
            if report["interpreter_exit"] != report["cranelift_exit"]:
                failures.append(f"{case.name}: exit mismatch off={off.returncode} cranelift={cranelift.returncode}")
            if not report["stdout_match"]:
                failures.append(f"{case.name}: stdout mismatch")
            if not report["stderr_match_without_jit_stats"]:
                failures.append(f"{case.name}: stderr mismatch")
            if report["compiled"] <= 0 or report["executed"] <= 0:
                failures.append(f"{case.name}: expected Cranelift compile and execution")
            if report["side_exits"] != 0:
                failures.append(f"{case.name}: expected zero side exits, got {report['side_exits']}")

    grammar_coverage = sorted({feature for case in cases for feature in case["grammar_features"]})
    required_features = {"constants", "params", "add", "sub", "mul", "comparisons", "branches"}
    missing_features = sorted(required_features - set(grammar_coverage))
    for feature in missing_features:
        failures.append(f"missing generated grammar feature: {feature}")

    result = {
        "schema_version": 1,
        "status": "pass" if not failures else "fail",
        "seeds": list(seeds),
        "cases_per_seed": args.cases_per_seed,
        "case_count": len(cases),
        "grammar": {
            "allowed": sorted(required_features),
            "observed": grammar_coverage,
            "small_loops": "not generated; loops are optional for 07.CL.C",
        },
        "runtime_policy": "bounded deterministic smoke suitable for optional CI",
        "cases": cases,
        "failures": failures,
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    if failures:
        print(f"[fail] Cranelift eligible-IR fuzz smoke found {len(failures)} failure(s); wrote {rel(args.out)}", file=sys.stderr)
        return 1
    print(f"[pass] Cranelift eligible-IR fuzz smoke compared {len(cases)} generated case(s); wrote {rel(args.out)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
