#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

OUT_DIR="target/performance/callgrind"
mkdir -p "$OUT_DIR"
rm -f "$OUT_DIR"/*

write_skip() {
    local reason="$1"
    printf '[skip] performance callgrind smoke: %s\n' "$reason"
    python3 - "$OUT_DIR/summary.json" "$reason" <<'PY'
import json
import sys
from pathlib import Path

out = Path(sys.argv[1])
reason = sys.argv[2]
out.write_text(
    json.dumps(
        {
            "status": "skipped",
            "reason": reason,
            "tool": "valgrind/callgrind",
            "measurements": [],
        },
        indent=2,
        sort_keys=True,
    )
    + "\n",
    encoding="utf-8",
)
PY
}

if [ "$(uname -s)" != "Linux" ]; then
    write_skip "Callgrind is only supported by this gate on Linux; host is $(uname -s)"
    exit 0
fi

if ! command -v valgrind >/dev/null 2>&1; then
    write_skip "valgrind is not available in PATH"
    exit 0
fi

ENGINE="${PHRUST_PHP_VM:-${CARGO_TARGET_DIR:-target}/debug/php-vm}"
if [ -z "${PHRUST_PHP_VM:-}" ]; then
    cargo build -p php_vm_cli --bin php-vm
fi
if [ ! -x "$ENGINE" ]; then
    printf '[fail] Rust VM is not executable: %s\n' "$ENGINE" >&2
    exit 1
fi

export TZ=UTC
export LC_ALL=C
export LANG=C

SCENARIOS=(
    "loops:tests/fixtures/performance/perf_smoke/loops.php"
    "function_calls:tests/fixtures/performance/perf_smoke/function_calls.php"
    "arrays_packed:tests/fixtures/performance/perf_smoke/arrays_packed.php"
)

summary_tsv="$OUT_DIR/summary.tsv"
printf 'scenario\tinstructions\tcallgrind_file\n' > "$summary_tsv"

for scenario in "${SCENARIOS[@]}"; do
    name="${scenario%%:*}"
    fixture="${scenario#*:}"
    expected="$fixture.out"
    callgrind_out="$OUT_DIR/$name.callgrind.out"
    stdout="$OUT_DIR/$name.stdout"
    stderr="$OUT_DIR/$name.stderr"

    if [ ! -f "$fixture" ]; then
        printf '[fail] missing callgrind fixture: %s\n' "$fixture" >&2
        exit 1
    fi
    if [ ! -f "$expected" ]; then
        printf '[fail] missing expected output for %s\n' "$fixture" >&2
        exit 1
    fi

    set +e
    valgrind \
        --tool=callgrind \
        --quiet \
        --callgrind-out-file="$callgrind_out" \
        "$ENGINE" run \
        --opt-level=1 \
        --quickening=on \
        --inline-caches=on \
        "$fixture" \
        > "$stdout" \
        2> "$stderr"
    valgrind_status=$?
    set -e

    if [ "$valgrind_status" -ne 0 ]; then
        if grep -Fq 'E_PHRUST_CLI_THREAD_SPAWN_FAILED' "$stderr" \
            && grep -Fq 'Bad address (os error 14)' "$stderr"; then
            write_skip "$(valgrind --version) cannot launch the php-vm runtime thread on this host: $(head -n 1 "$stderr")"
            exit 0
        fi
        printf '[fail] callgrind execution failed for %s with status %s\n' \
            "$fixture" "$valgrind_status" >&2
        sed -n '1,20p' "$stderr" >&2
        exit 1
    fi

    if ! cmp -s "$expected" "$stdout"; then
        printf '[fail] callgrind smoke stdout mismatch for %s\n' "$fixture" >&2
        exit 1
    fi
    if [ ! -s "$callgrind_out" ]; then
        printf '[fail] callgrind did not write output for %s\n' "$fixture" >&2
        exit 1
    fi

    instructions="$(awk '/^summary:/ { print $2; found=1; exit } END { if (!found) exit 1 }' "$callgrind_out")" || {
        printf '[fail] callgrind summary missing for %s\n' "$fixture" >&2
        exit 1
    }
    case "$instructions" in
        ''|*[!0-9]*)
            printf '[fail] callgrind summary is not a stable integer for %s: %s\n' "$fixture" "$instructions" >&2
            exit 1
            ;;
    esac
    printf '%s\t%s\t%s\n' "$name" "$instructions" "$callgrind_out" >> "$summary_tsv"
done

python3 - "$summary_tsv" "$OUT_DIR/summary.json" "$(valgrind --version)" <<'PY'
import csv
import json
import sys
from pathlib import Path

tsv = Path(sys.argv[1])
out = Path(sys.argv[2])
version = sys.argv[3]

rows = []
with tsv.open("r", encoding="utf-8", newline="") as handle:
    for row in csv.DictReader(handle, delimiter="\t"):
        rows.append(
            {
                "scenario": row["scenario"],
                "instructions": int(row["instructions"]),
                "callgrind_file": row["callgrind_file"],
            }
        )

out.write_text(
    json.dumps(
        {
            "status": "passed",
            "tool": "valgrind/callgrind",
            "tool_version": version,
            "threshold_policy": "none",
            "measurements": rows,
        },
        indent=2,
        sort_keys=True,
    )
    + "\n",
    encoding="utf-8",
)
PY

printf '[pass] performance callgrind smoke measured %s scenario(s)\n' "${#SCENARIOS[@]}"
