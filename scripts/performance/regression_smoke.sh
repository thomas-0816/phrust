#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

ENGINE="${PHRUST_PHP_VM:-${CARGO_TARGET_DIR:-target}/debug/php-vm}"
FIXTURES_DIR="tests/fixtures/performance/regressions"
OUT_DIR="target/performance/regression-smoke"

if [ ! -x "$ENGINE" ]; then
    printf '[fail] Rust VM is not executable: %s\n' "$ENGINE" >&2
    exit 1
fi
if [ ! -d "$FIXTURES_DIR" ]; then
    printf '[fail] missing performance regression fixture directory: %s\n' "$FIXTURES_DIR" >&2
    exit 1
fi

mkdir -p "$OUT_DIR"
rm -f "$OUT_DIR"/*

normalize() {
    tr -d '\r' < "$1"
}

fixture_count=0
run_count=0
for fixture in "$FIXTURES_DIR"/*.php; do
    name="$(basename "$fixture" .php)"
    expected="$fixture.out"
    if [ ! -f "$expected" ]; then
        printf '[fail] missing expected output for %s\n' "$fixture" >&2
        exit 1
    fi

    baseline_stderr=""
    for preset in baseline default; do
                label="${name}.${preset}"
                stdout="$OUT_DIR/$label.stdout"
                stderr="$OUT_DIR/$label.stderr"
                stdout_norm="$OUT_DIR/$label.stdout.norm"
                stderr_norm="$OUT_DIR/$label.stderr.norm"

                status=0
                "$ENGINE" run \
                    --engine-preset="$preset" \
                    "$fixture" \
                    > "$stdout" \
                    2> "$stderr" || status=$?

                if [ "$status" -ne 0 ]; then
                    printf '[fail] performance regression exited nonzero: %s (%s), status %s\n' "$fixture" "$label" "$status" >&2
                    printf '[fail] stdout: %s\n' "$stdout" >&2
                    printf '[fail] stderr: %s\n' "$stderr" >&2
                    exit "$status"
                fi

                normalize "$stdout" > "$stdout_norm"
                normalize "$stderr" > "$stderr_norm"

                if ! cmp -s "$expected" "$stdout_norm"; then
                    printf '[fail] performance regression stdout mismatch: %s (%s)\n' "$fixture" "$label" >&2
                    exit 1
                fi
                if [ -z "$baseline_stderr" ]; then
                    baseline_stderr="$stderr_norm"
                elif ! cmp -s "$baseline_stderr" "$stderr_norm"; then
                    printf '[fail] performance regression stderr diverged: %s (%s)\n' "$fixture" "$label" >&2
                    exit 1
                fi
                run_count=$((run_count + 1))
    done
    fixture_count=$((fixture_count + 1))
done

if [ "$fixture_count" -lt 8 ]; then
    printf '[fail] expected at least 8 performance regression fixtures, found %s\n' "$fixture_count" >&2
    exit 1
fi

printf '[pass] performance regression smoke compared %s fixture(s), %s run(s)\n' "$fixture_count" "$run_count"
