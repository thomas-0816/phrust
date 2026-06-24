#!/usr/bin/env python3
"""Offline framework-like Cranelift smokes for Phase 7."""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

import jit_bench_matrix


ROOT = Path(__file__).resolve().parents[3]
DEFAULT_ENGINE = ROOT / "target/debug/php-vm"
DEFAULT_OUT = ROOT / "target/phase7/cranelift/framework-smoke.json"
DEFAULT_FIXTURE_DIR = ROOT / "target/phase7/cranelift/framework-smoke/fixtures"

PATH_COUNTERS: tuple[tuple[str, str], ...] = (
    ("packed_array_fetch", "packed_fetch_fast_hits"),
    ("packed_foreach_sum", "packed_foreach_sum_fast_hits"),
    ("known_call", "known_call_fast_hits"),
    ("method_direct_call", "direct_call_hits"),
    ("property_load", "property_load_fast_hits"),
    ("string_concat", "string_concat_fast_path_hits"),
)


FIXTURES: tuple[dict[str, Any], ...] = (
    {
        "scenario": "router_dispatch",
        "kind": "router dispatch",
        "expected_paths": ["method_direct_call"],
        "source": """<?php
class Phase7FrameworkController {
    public function show(int $id): int {
        return $id + 10;
    }

    public function listing(int $id): int {
        return $id + 20;
    }
}

function phase7_framework_router_dispatch(string $path, int $id): int {
    $controller = new Phase7FrameworkController();
    if ($path === "/show") {
        return $controller->show($id);
    }
    return $controller->listing($id);
}

$total = 0;
for ($i = 0; $i < 16; $i++) {
    $total += phase7_framework_router_dispatch("/show", $i);
}
echo $total, "\\n";
""",
    },
    {
        "scenario": "dto_hydration",
        "kind": "DTO hydration",
        "expected_paths": ["property_load"],
        "source": """<?php
class Phase7FrameworkDto {
    public int $id;
    public int $score;

    public function __construct(int $id, int $score) {
        $this->id = $id;
        $this->score = $score;
    }
}

function phase7_framework_dto_score(Phase7FrameworkDto $dto): int {
    return $dto->score;
}

$total = 0;
for ($i = 0; $i < 16; $i++) {
    $dto = new Phase7FrameworkDto($i, $i + 3);
    $total += phase7_framework_dto_score($dto);
}
echo $total, "\\n";
""",
    },
    {
        "scenario": "service_method_loop",
        "kind": "service method loop",
        "expected_paths": ["method_direct_call"],
        "source": """<?php
class Phase7FrameworkServiceDto {
    public int $base;

    public function __construct(int $base) {
        $this->base = $base;
    }
}

class Phase7FrameworkService {
    public function score(Phase7FrameworkServiceDto $dto, int $x): int {
        return $dto->base + $x + 1;
    }
}

$service = new Phase7FrameworkService();
$dto = new Phase7FrameworkServiceDto(7);
$total = 0;
for ($i = 0; $i < 24; $i++) {
    $total += $service->score($dto, $i);
}
echo $total, "\\n";
""",
    },
    {
        "scenario": "template_string_concat",
        "kind": "template-like string concat",
        "expected_paths": ["string_concat"],
        "source": """<?php
function phase7_framework_template_piece(string $lhs, string $rhs): string {
    return $lhs . $rhs;
}

$html = "";
for ($i = 0; $i < 24; $i++) {
    $html = phase7_framework_template_piece($html, "<li>x</li>");
}
echo strlen($html), "\\n";
""",
    },
    {
        "scenario": "config_array_reads",
        "kind": "config array reads",
        "expected_paths": ["packed_array_fetch"],
        "source": """<?php
function phase7_framework_config_read(array $config, int $index): int {
    return $config[$index];
}

$config = [3, 5, 8, 13];
$total = 0;
for ($i = 0; $i < 20; $i++) {
    $total += phase7_framework_config_read($config, $i % 4);
}
echo $total, "\\n";
""",
    },
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--engine", type=Path, default=Path(os.getenv("PHRUST_PHP_VM", DEFAULT_ENGINE)))
    parser.add_argument("--out", type=Path, default=DEFAULT_OUT)
    parser.add_argument("--fixture-dir", type=Path, default=DEFAULT_FIXTURE_DIR)
    parser.add_argument("--timeout", type=float, default=5.0)
    return parser.parse_args()


def rel(path: Path) -> str:
    try:
        return path.resolve().relative_to(ROOT).as_posix()
    except ValueError:
        return path.as_posix()


def normalized_env() -> dict[str, str]:
    env = jit_bench_matrix.normalized_env(ROOT / "target/phase7/cranelift/framework-smoke/tmp")
    env["PHRUST_RANDOM_SEED"] = "phase7-cranelift-framework-smoke"
    env["RUST_TEST_SEED"] = "phase7-cranelift-framework-smoke"
    return env


def write_fixtures(fixture_dir: Path) -> dict[str, Path]:
    fixture_dir.mkdir(parents=True, exist_ok=True)
    paths: dict[str, Path] = {}
    for fixture in FIXTURES:
        path = fixture_dir / f"{fixture['scenario']}.php"
        path.write_text(str(fixture["source"]), encoding="utf-8")
        paths[str(fixture["scenario"])] = path
    return paths


def run_vm(
    *,
    engine: Path,
    fixture: Path,
    mode: str,
    timeout: float,
) -> tuple[subprocess.CompletedProcess[str], float]:
    command = [str(engine), "run", "--inline-caches=on", f"--jit={mode}"]
    if mode == "cranelift":
        command.extend(["--jit-eager", "--jit-stats=json"])
    command.append(rel(fixture))
    start = time.perf_counter()
    completed = subprocess.run(
        command,
        cwd=ROOT,
        env=normalized_env(),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
        check=False,
    )
    return completed, time.perf_counter() - start


def int_stat(stats: dict[str, Any], key: str) -> int:
    value = stats.get(key, 0)
    return value if isinstance(value, int) else 0


def triggered_paths(stats: dict[str, Any]) -> list[dict[str, Any]]:
    paths: list[dict[str, Any]] = []
    for path, counter in PATH_COUNTERS:
        hits = int_stat(stats, counter)
        if hits > 0:
            paths.append({"path": path, "counter": counter, "hits": hits})
    return paths


def path_names(paths: list[dict[str, Any]]) -> set[str]:
    return {str(path["path"]) for path in paths}


def main() -> int:
    args = parse_args()
    if not args.engine.is_file() or not os.access(args.engine, os.X_OK):
        raise SystemExit(f"Rust VM is not executable: {args.engine}")

    fixture_paths = write_fixtures(args.fixture_dir)
    rows: list[dict[str, Any]] = []
    failures: list[str] = []

    for fixture in FIXTURES:
        scenario = str(fixture["scenario"])
        fixture_path = fixture_paths[scenario]
        off, off_seconds = run_vm(
            engine=args.engine,
            fixture=fixture_path,
            mode="off",
            timeout=args.timeout,
        )
        cranelift, cranelift_seconds = run_vm(
            engine=args.engine,
            fixture=fixture_path,
            mode="cranelift",
            timeout=args.timeout,
        )
        stats = jit_bench_matrix.extract_jit_stats(cranelift.stderr)
        stripped_stderr = jit_bench_matrix.stderr_without_jit_stats(cranelift.stderr)
        stdout_match = jit_bench_matrix.normalize_text(off.stdout) == jit_bench_matrix.normalize_text(
            cranelift.stdout
        )
        stderr_match = jit_bench_matrix.normalize_text(off.stderr) == jit_bench_matrix.normalize_text(
            stripped_stderr
        )
        if stats is None:
            failures.append(f"{scenario}: missing Cranelift stats JSON")
            stats = {}

        paths = triggered_paths(stats)
        missing_paths = sorted(set(fixture["expected_paths"]) - path_names(paths))
        if off.returncode != cranelift.returncode:
            failures.append(f"{scenario}: exit mismatch off={off.returncode} cranelift={cranelift.returncode}")
        if not stdout_match:
            failures.append(f"{scenario}: stdout mismatch")
        if not stderr_match:
            failures.append(f"{scenario}: stderr mismatch")
        if missing_paths:
            failures.append(f"{scenario}: missing expected Big-Win path(s): {', '.join(missing_paths)}")

        rows.append(
            {
                "scenario": scenario,
                "kind": fixture["kind"],
                "fixture": rel(fixture_path),
                "expected_paths": list(fixture["expected_paths"]),
                "triggered_paths": paths,
                "missing_expected_paths": missing_paths,
                "off_exit": off.returncode,
                "cranelift_exit": cranelift.returncode,
                "stdout_match": stdout_match,
                "stderr_match": stderr_match,
                "output_match": off.returncode == cranelift.returncode and stdout_match and stderr_match,
                "off_seconds": off_seconds,
                "cranelift_seconds": cranelift_seconds,
                "jit": {
                    "compiled": int_stat(stats, "compiled"),
                    "executed": int_stat(stats, "executed"),
                    "fast_path_hits": int_stat(stats, "fast_path_hits"),
                    "helper_calls": int_stat(stats, "helper_calls"),
                    "side_exits": int_stat(stats, "side_exits"),
                    "guard_failures": int_stat(stats, "guard_failures"),
                    "code_bytes": int_stat(stats, "code_bytes"),
                    "compile_time_nanos": int_stat(stats, "compile_time_nanos"),
                },
            }
        )

    all_triggered = sorted({item["path"] for row in rows for item in row["triggered_paths"]})
    if not all_triggered:
        failures.append("no framework-like fixture triggered any Big-Win path")

    result = {
        "schema_version": 1,
        "status": "pass" if not failures else "fail",
        "runtime_policy": "offline local smoke; generated fixtures only; no vendored framework dependency",
        "fixture_count": len(FIXTURES),
        "required_fixture_kinds": [str(fixture["kind"]) for fixture in FIXTURES],
        "all_triggered_paths": all_triggered,
        "rows": rows,
        "failures": failures,
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")

    if failures:
        print(
            f"[fail] Cranelift framework smoke found {len(failures)} failure(s); wrote {rel(args.out)}",
            file=sys.stderr,
        )
        return 1
    print(f"[pass] Cranelift framework smoke compared {len(FIXTURES)} fixture(s); wrote {rel(args.out)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
