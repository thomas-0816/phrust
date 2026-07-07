#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT}"

ENGINE="${CARGO_TARGET_DIR:-target}/debug/php-vm"
OUT_DIR="target/performance/superinstruction-smoke"
RULE_DIR="target/performance/rules"
mkdir -p "${OUT_DIR}"
mkdir -p "${RULE_DIR}"

if [[ ! -x "${ENGINE}" ]]; then
  printf '[error] missing VM engine: %s\n' "${ENGINE}" >&2
  printf '[hint] run: cargo build -p php_vm_cli\n' >&2
  exit 1
fi

fixtures=(
  "fixtures/runtime/valid/hello.php"
  "fixtures/runtime/valid/scalars/echo.php"
  "fixtures/runtime/valid/scalars/expressions.php"
  "fixtures/runtime/valid/variables/assignment.php"
  "fixtures/runtime/valid/functions/simple.php"
  "fixtures/runtime/valid/functions/two-args.php"
  "fixtures/bytecode/literals/valid/echo-int.php"
  "fixtures/bytecode/literals/valid/echo-multiple.php"
  "tests/fixtures/performance/superinstructions/binary-concat-echo.php"
  "tests/fixtures/performance/superinstructions/store-discard.php"
  "tests/fixtures/performance/superinstructions/const-key-dim-fetch.php"
  "tests/fixtures/performance/superinstructions/load-chain-const.php"
  "tests/fixtures/performance/superinstructions/call-discard.php"
  "tests/fixtures/performance/superinstructions/const-pair-load.php"
  "tests/fixtures/performance/superinstructions/const-array-insert.php"
)

json_escape() {
  python3 -c 'import json,sys; print(json.dumps(sys.argv[1]))' "$1"
}

summary_rows=()
for fixture in "${fixtures[@]}"; do
  stem="${fixture%.php}"
  stem="${stem//\//_}"
  off_stdout="${OUT_DIR}/${stem}.off.stdout"
  off_stderr="${OUT_DIR}/${stem}.off.stderr"
  on_stdout="${OUT_DIR}/${stem}.on.stdout"
  on_stderr="${OUT_DIR}/${stem}.on.stderr"
  on_counters="${OUT_DIR}/${stem}.on.counters.json"
  rule_dump="${RULE_DIR}/${stem}.rules.txt"
  rule_json="${RULE_DIR}/${stem}.rules.json"

  "${ENGINE}" run --exec-format=bytecode --superinstructions=off "${fixture}" >"${off_stdout}" 2>"${off_stderr}"
  "${ENGINE}" run --exec-format=bytecode --superinstructions=on --counters-json="${on_counters}" "${fixture}" >"${on_stdout}" 2>"${on_stderr}"
  "${ENGINE}" dump-rule-selection "${fixture}" >"${rule_dump}"
  "${ENGINE}" dump-rule-selection --json "${fixture}" >"${rule_json}"
  cmp "${off_stdout}" "${on_stdout}"
  cmp "${off_stderr}" "${on_stderr}"
  python3 - "$on_counters" <<'PY'
import json
import sys

path = sys.argv[1]
data = json.loads(open(path, encoding="utf-8").read())
if data.get("bytecode_lower_attempts") != 1:
    raise SystemExit(f"[error] {path}: expected one dense bytecode lowering attempt")
if data.get("bytecode_lower_successes") != 1:
    raise SystemExit(f"[error] {path}: expected one dense bytecode lowering success")
if data.get("bytecode_unsupported_fallbacks") != 0:
    raise SystemExit(f"[error] {path}: expected no dense bytecode fallback")
if data.get("superinstruction_deopt_or_fallbacks") != 0:
    raise SystemExit(f"[error] {path}: expected no superinstruction deopt/fallback")
if data.get("superinstruction_deopt_or_fallback_by_reason") != {}:
    raise SystemExit(f"[error] {path}: expected no superinstruction fallback reasons")
PY
  if ! grep -q '^rule-selection$' "${rule_dump}"; then
    printf '[error] %s: missing rule-selection header\n' "${rule_dump}" >&2
    exit 1
  fi
  python3 - "$rule_json" <<'PY'
import json
import sys

path = sys.argv[1]
data = json.loads(open(path, encoding="utf-8").read())
if data.get("rule_selection_candidates", 0) <= 0:
    raise SystemExit(f"[error] {path}: expected rule selection candidates")
if data.get("rule_selection_selected", 0) <= 0:
    raise SystemExit(f"[error] {path}: expected selected rules")
if not isinstance(data.get("rule_selection_by_kind"), dict):
    raise SystemExit(f"[error] {path}: rule_selection_by_kind must be an object")
PY
  summary_rows+=("$(json_escape "${fixture}")")
done

python3 - "${OUT_DIR}"/*.on.counters.json <<'PY'
import json
import sys

total_candidates = 0
total_emitted = 0
total_executed = 0
kinds = set()
candidate_kinds = set()
emitted_kinds = set()
skipped_reasons = set()
for path in sys.argv[1:]:
    data = json.loads(open(path, encoding="utf-8").read())
    total_candidates += data.get("superinstruction_candidates", 0)
    total_emitted += data.get("superinstructions_emitted", 0)
    candidate_map = data.get("superinstruction_candidates_by_kind", {})
    emitted_map = data.get("superinstructions_emitted_by_kind", {})
    skipped_map = data.get("superinstruction_skipped_by_reason", {})
    if not isinstance(candidate_map, dict):
        raise SystemExit(f"[error] {path}: superinstruction_candidates_by_kind must be an object")
    if not isinstance(emitted_map, dict):
        raise SystemExit(f"[error] {path}: superinstructions_emitted_by_kind must be an object")
    if not isinstance(skipped_map, dict):
        raise SystemExit(f"[error] {path}: superinstruction_skipped_by_reason must be an object")
    candidate_kinds.update(candidate_map)
    emitted_kinds.update(emitted_map)
    skipped_reasons.update(skipped_map)
    executed = data.get("superinstructions_executed", {})
    total_executed += sum(executed.values())
    kinds.update(executed)
if total_candidates <= 0:
    raise SystemExit("[error] expected at least one superinstruction candidate")
if total_emitted <= 0:
    raise SystemExit("[error] expected at least one emitted superinstruction")
if total_executed <= 0:
    raise SystemExit("[error] expected at least one executed superinstruction")
required = {"load_const_echo", "load_local_echo", "binary_concat_echo", "store_local_discard"}
missing = sorted(required - kinds)
if missing:
    raise SystemExit(f"[error] missing executed superinstruction kinds: {', '.join(missing)}")
missing_candidates = sorted(required - candidate_kinds)
if missing_candidates:
    raise SystemExit(f"[error] missing candidate superinstruction kinds: {', '.join(missing_candidates)}")
missing_emitted = sorted(required - emitted_kinds)
if missing_emitted:
    raise SystemExit(f"[error] missing emitted superinstruction kinds: {', '.join(missing_emitted)}")
if "unsupported_producer_echo_pair" not in skipped_reasons:
    raise SystemExit("[error] expected unsupported producer+echo skip reason")
print(f"{total_candidates} {total_emitted} {total_executed}")
PY

summary="${OUT_DIR}/summary.json"
{
  printf '{\n'
  printf '  "status": "pass",\n'
  printf '  "engine": %s,\n' "$(json_escape "${ENGINE}")"
  printf '  "fixture_count": %s,\n' "${#fixtures[@]}"
  printf '  "fixtures": ['
  for index in "${!summary_rows[@]}"; do
    if [[ "${index}" -gt 0 ]]; then
      printf ', '
    fi
    printf '%s' "${summary_rows[$index]}"
  done
  printf ']\n'
  printf '}\n'
} >"${summary}"

scripts/performance/superinstruction_patterns.py \
  --engine "${ENGINE}" \
  --summary-doc target/performance/superinstructions/summary.md

printf '[pass] superinstruction smoke compared %s fixture(s) and wrote %s\n' "${#fixtures[@]}" "${summary}"
