#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT}"

ENGINE="${CARGO_TARGET_DIR:-target}/debug/php-vm"
OUT_DIR="target/performance/bytecode-exec-smoke"
mkdir -p "${OUT_DIR}"

if [[ ! -x "${ENGINE}" ]]; then
  printf '[error] missing VM engine: %s\n' "${ENGINE}" >&2
  printf '[hint] run: cargo build -p php_vm_cli\n' >&2
  exit 1
fi

supported_fixtures=(
  "fixtures/runtime/valid/hello.php"
  "fixtures/runtime/valid/scalars/echo.php"
  "fixtures/runtime/valid/scalars/expressions.php"
  "fixtures/runtime/valid/scalars/comparisons.php"
  "fixtures/runtime/valid/variables/assignment.php"
  "fixtures/runtime/valid/functions/simple.php"
  "fixtures/runtime/valid/functions/two-args.php"
  "fixtures/bytecode/lower/valid/echo.php"
  "fixtures/bytecode/literals/valid/echo-int.php"
  "fixtures/bytecode/literals/valid/echo-multiple.php"
  "fixtures/bytecode/fallback/property-read-unsupported.php"
  "fixtures/bytecode/fallback/property-write-unsupported.php"
  "fixtures/bytecode/fallback/method-call-unsupported.php"
  "fixtures/bytecode/fallback/static-method-unsupported.php"
  "fixtures/bytecode/fallback/static-method-auto.php"
  "tests/fixtures/performance/perf_smoke/arrays_packed.php"
  "tests/fixtures/performance/framework_smoke/packed_mixed_array_traversal.php"
)

# Must stay outside the dense subset in strict mode; includes gained dense
# support, so generator yields are the probe now.
strict_unsupported_fixtures=(
  "fixtures/runtime/valid/generators/yield.php:instruction_subset"
)

auto_fallback_fixtures=(
  "fixtures/bytecode/fallback/object-property-read-auto.php:object_instantiation"
  "fixtures/bytecode/fallback/object-property-write-auto.php:object_instantiation"
  "fixtures/bytecode/fallback/method-dispatch-auto.php:object_instantiation"
  "fixtures/bytecode/fallback/magic-get-auto.php:object_instantiation"
  "fixtures/bytecode/fallback/dynamic-property-auto.php:object_instantiation"
  "fixtures/bytecode/fallback/typed-property-auto.php:object_instantiation"
  "fixtures/bytecode/fallback/property-hook-auto.php:object_instantiation"
)

json_escape() {
  python3 -c 'import json,sys; print(json.dumps(sys.argv[1]))' "$1"
}

check_counter() {
  local path="$1"
  local key="$2"
  local expected="$3"
  python3 - "$path" "$key" "$expected" <<'PY'
import json
import sys

path, key, expected = sys.argv[1], sys.argv[2], int(sys.argv[3])
data = json.loads(open(path, encoding="utf-8").read())
actual = data.get(key)
if actual != expected:
    raise SystemExit(f"[error] {path}: expected {key}={expected}, got {actual}")
PY
}

check_counter_map_key() {
  local path="$1"
  local map_key="$2"
  local reason="$3"
  python3 - "$path" "$map_key" "$reason" <<'PY'
import json
import sys

path, map_key, reason = sys.argv[1], sys.argv[2], sys.argv[3]
data = json.loads(open(path, encoding="utf-8").read())
value = data.get(map_key, {}).get(reason, 0)
if value <= 0:
    raise SystemExit(f"[error] {path}: expected {map_key}[{reason!r}] > 0, got {value}")
PY
}

check_counter_at_least() {
  local path="$1"
  local key="$2"
  local minimum="$3"
  python3 - "$path" "$key" "$minimum" <<'PY'
import json
import sys

path, key, minimum = sys.argv[1], sys.argv[2], int(sys.argv[3])
data = json.loads(open(path, encoding="utf-8").read())
actual = data.get(key, 0)
if actual < minimum:
    raise SystemExit(f"[error] {path}: expected {key}>={minimum}, got {actual}")
PY
}

summary_rows=()
for fixture in "${supported_fixtures[@]}"; do
  stem="${fixture%.php}"
  stem="${stem//\//_}"
  ir_stdout="${OUT_DIR}/${stem}.ir.stdout"
  ir_stderr="${OUT_DIR}/${stem}.ir.stderr"
  bc_stdout="${OUT_DIR}/${stem}.bytecode.stdout"
  bc_stderr="${OUT_DIR}/${stem}.bytecode.stderr"
  bc_counters="${OUT_DIR}/${stem}.bytecode.counters.json"

  "${ENGINE}" run --exec-format=ir "${fixture}" >"${ir_stdout}" 2>"${ir_stderr}"
  "${ENGINE}" run --exec-format=bytecode --counters-json="${bc_counters}" "${fixture}" >"${bc_stdout}" 2>"${bc_stderr}"
  cmp "${ir_stdout}" "${bc_stdout}"
  cmp "${ir_stderr}" "${bc_stderr}"
  check_counter "${bc_counters}" bytecode_lower_attempts 1
  check_counter "${bc_counters}" bytecode_lower_successes 1
  check_counter "${bc_counters}" bytecode_unsupported_fallbacks 0
  check_counter_map_key "${bc_counters}" bytecode_lowered_by_family "control_flow"
  python3 - "$bc_counters" <<'PY'
import json
import sys

data = json.loads(open(sys.argv[1], encoding="utf-8").read())
if data.get("bytecode_instructions_executed", 0) <= 0:
    raise SystemExit(f"[error] {sys.argv[1]}: expected bytecode instructions to execute")
if not data.get("bytecode_executed_by_family"):
    raise SystemExit(f"[error] {sys.argv[1]}: expected bytecode executed family counters")
PY
  summary_rows+=("$(json_escape "${fixture}")")
done

# Must stay a mixed-plan fixture: the unit lowers but main is rich-planned.
# casts.php stopped qualifying when `cast` joined the dense subset.
fallback_fixture="fixtures/runtime/valid/exceptions/catch-exception.php"
fallback_ir_stdout="${OUT_DIR}/fallback.ir.stdout"
fallback_ir_stderr="${OUT_DIR}/fallback.ir.stderr"
fallback_auto_stdout="${OUT_DIR}/fallback.auto.stdout"
fallback_auto_stderr="${OUT_DIR}/fallback.auto.stderr"
fallback_counters="${OUT_DIR}/fallback.auto.counters.json"
strict_stdout="${OUT_DIR}/fallback.strict.stdout"
strict_stderr="${OUT_DIR}/fallback.strict.stderr"

"${ENGINE}" run --exec-format=ir "${fallback_fixture}" >"${fallback_ir_stdout}" 2>"${fallback_ir_stderr}"
"${ENGINE}" run --exec-format=auto --counters-json="${fallback_counters}" "${fallback_fixture}" >"${fallback_auto_stdout}" 2>"${fallback_auto_stderr}"
cmp "${fallback_ir_stdout}" "${fallback_auto_stdout}"
cmp "${fallback_ir_stderr}" "${fallback_auto_stderr}"
check_counter "${fallback_counters}" bytecode_lower_attempts 1
check_counter "${fallback_counters}" bytecode_lower_successes 1
check_counter "${fallback_counters}" bytecode_unsupported_fallbacks 0
check_counter "${fallback_counters}" bytecode_instructions_executed 0
check_counter_at_least "${fallback_counters}" rich_fallback_functions_executed 1

set +e
"${ENGINE}" run --exec-format=bytecode "${fallback_fixture}" >"${strict_stdout}" 2>"${strict_stderr}"
strict_status=$?
set -e
if [[ "${strict_status}" -ne 4 ]]; then
  printf '[error] strict bytecode unsupported fixture returned %s, expected 4\n' "${strict_status}" >&2
  exit 1
fi
grep -q 'E_PHP_VM_DENSE_BYTECODE_UNSUPPORTED' "${strict_stderr}"

strict_unsupported_rows=()
for entry in "${strict_unsupported_fixtures[@]}"; do
  fixture="${entry%%:*}"
  reason="${entry#*:}"
  stem="${fixture%.php}"
  stem="${stem//\//_}"
  strict_stdout="${OUT_DIR}/${stem}.strict.stdout"
  strict_stderr="${OUT_DIR}/${stem}.strict.stderr"
  strict_counters="${OUT_DIR}/${stem}.strict.counters.json"

  set +e
  "${ENGINE}" run --exec-format=bytecode --counters-json="${strict_counters}" "${fixture}" >"${strict_stdout}" 2>"${strict_stderr}"
  strict_status=$?
  set -e
  if [[ "${strict_status}" -ne 4 ]]; then
    printf '[error] strict bytecode unsupported fixture %s returned %s, expected 4\n' "${fixture}" "${strict_status}" >&2
    exit 1
  fi
  grep -q 'E_PHP_VM_DENSE_BYTECODE_UNSUPPORTED' "${strict_stderr}"
  check_counter "${strict_counters}" bytecode_lower_attempts 1
  check_counter "${strict_counters}" bytecode_lower_successes 0
  check_counter_map_key "${strict_counters}" bytecode_unsupported_reasons "${reason}"
  strict_unsupported_rows+=("$(json_escape "${fixture}")")
done

auto_fallback_rows=()
for entry in "${auto_fallback_fixtures[@]}"; do
  fixture="${entry%%:*}"
  reason="${entry#*:}"
  stem="${fixture%.php}"
  stem="${stem//\//_}"
  ir_stdout="${OUT_DIR}/${stem}.ir.stdout"
  ir_stderr="${OUT_DIR}/${stem}.ir.stderr"
  auto_stdout="${OUT_DIR}/${stem}.auto.stdout"
  auto_stderr="${OUT_DIR}/${stem}.auto.stderr"
  auto_counters="${OUT_DIR}/${stem}.auto.counters.json"

  "${ENGINE}" run --exec-format=ir "${fixture}" >"${ir_stdout}" 2>"${ir_stderr}"
  "${ENGINE}" run --exec-format=auto --counters-json="${auto_counters}" "${fixture}" >"${auto_stdout}" 2>"${auto_stderr}"
  cmp "${ir_stdout}" "${auto_stdout}"
  cmp "${ir_stderr}" "${auto_stderr}"
  check_counter "${auto_counters}" bytecode_lower_attempts 1
  check_counter "${auto_counters}" bytecode_lower_successes 1
  check_counter "${auto_counters}" bytecode_unsupported_fallbacks 0
  check_counter_at_least "${auto_counters}" rich_fallback_functions_executed 1
  check_counter_map_key "${auto_counters}" dense_function_fallback_by_reason "${reason}"
  auto_fallback_rows+=("$(json_escape "${fixture}")")
done

mixed_fixture="tests/fixtures/performance/bytecode_exec/mixed_dense_rich.php"
mixed_ir_stdout="${OUT_DIR}/mixed.ir.stdout"
mixed_ir_stderr="${OUT_DIR}/mixed.ir.stderr"
mixed_auto_stdout="${OUT_DIR}/mixed.auto.stdout"
mixed_auto_stderr="${OUT_DIR}/mixed.auto.stderr"
mixed_counters="${OUT_DIR}/mixed.auto.counters.json"

"${ENGINE}" run --exec-format=ir "${mixed_fixture}" >"${mixed_ir_stdout}" 2>"${mixed_ir_stderr}"
"${ENGINE}" run --exec-format=auto --counters-json="${mixed_counters}" "${mixed_fixture}" >"${mixed_auto_stdout}" 2>"${mixed_auto_stderr}"
cmp "${mixed_ir_stdout}" "${mixed_auto_stdout}"
cmp "${mixed_ir_stderr}" "${mixed_auto_stderr}"
check_counter "${mixed_counters}" bytecode_lower_attempts 1
check_counter "${mixed_counters}" bytecode_lower_successes 1
check_counter "${mixed_counters}" bytecode_unsupported_fallbacks 0
check_counter_at_least "${mixed_counters}" dense_functions_planned 2
check_counter_at_least "${mixed_counters}" dense_functions_executed 2
check_counter_at_least "${mixed_counters}" rich_fallback_functions_planned 1
check_counter_at_least "${mixed_counters}" rich_fallback_functions_executed 1
check_counter_map_key "${mixed_counters}" dense_function_fallback_by_reason "object_instantiation"
check_counter_map_key "${mixed_counters}" dense_instruction_families_executed "function_calls"

summary="${OUT_DIR}/summary.json"
{
  printf '{\n'
  printf '  "status": "pass",\n'
  printf '  "engine": %s,\n' "$(json_escape "${ENGINE}")"
  printf '  "default_exec_format": "ir",\n'
  printf '  "supported_fixture_count": %s,\n' "${#supported_fixtures[@]}"
  printf '  "supported_fixtures": ['
  for index in "${!summary_rows[@]}"; do
    if [[ "${index}" -gt 0 ]]; then
      printf ', '
    fi
    printf '%s' "${summary_rows[$index]}"
  done
  printf '],\n'
  printf '  "fallback_fixture": %s,\n' "$(json_escape "${fallback_fixture}")"
  printf '  "strict_unsupported_status": %s,\n' "${strict_status}"
  printf '  "strict_unsupported_fixture_count": %s,\n' "${#strict_unsupported_fixtures[@]}"
  printf '  "strict_unsupported_fixtures": ['
  for index in "${!strict_unsupported_rows[@]}"; do
    if [[ "${index}" -gt 0 ]]; then
      printf ', '
    fi
    printf '%s' "${strict_unsupported_rows[$index]}"
  done
  printf '],\n'
  printf '  "auto_fallback_fixture_count": %s,\n' "${#auto_fallback_fixtures[@]}"
  printf '  "auto_fallback_fixtures": ['
  for index in "${!auto_fallback_rows[@]}"; do
    if [[ "${index}" -gt 0 ]]; then
      printf ', '
    fi
    printf '%s' "${auto_fallback_rows[$index]}"
  done
  printf '],\n'
  printf '  "mixed_mode_fixture": %s\n' "$(json_escape "${mixed_fixture}")"
  printf '}\n'
} >"${summary}"

printf '[pass] bytecode exec smoke compared %s supported fixture(s), verified %s strict unsupported fixture(s), %s auto fallback fixture(s), verified mixed dense/rich execution, and wrote %s\n' "${#supported_fixtures[@]}" "${#strict_unsupported_fixtures[@]}" "${#auto_fallback_fixtures[@]}" "${summary}"
