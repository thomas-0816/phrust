#!/usr/bin/env bash
set -euo pipefail

mode="${1:-all}"
case "$mode" in
  all|diagnostics|debug) ;;
  *)
    printf 'usage: %s [all|diagnostics|debug]\n' "$0" >&2
    exit 2
    ;;
esac

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
target_dir="${CARGO_TARGET_DIR:-target}"
if [[ "$target_dir" = /* ]]; then
  bin_dir="$target_dir/debug"
else
  bin_dir="$repo_root/$target_dir/debug"
fi
work_dir="$repo_root/target/diagnostics-smoke"
mkdir -p "$work_dir"

# The smoke asserts full-pipeline debug events (frontend analyze, optimizer),
# so keep every run cold: isolate the default bytecode cache into this run's
# work dir and clear it up front.
export PHRUST_BYTECODE_CACHE_DIR="$work_dir/bytecode-cache"
rm -rf "$PHRUST_BYTECODE_CACHE_DIR"

cd "$repo_root"
cargo build -p php_vm_cli -p php_server >/dev/null

json_required_fields='kind schema_version code layer phase message'

assert_json_lines() {
  local path="$1"
  python3 - "$path" $json_required_fields <<'PY'
import json
import sys

path = sys.argv[1]
required = sys.argv[2:]
count = 0
with open(path, "r", encoding="utf-8") as handle:
    for line_no, line in enumerate(handle, 1):
        line = line.strip()
        if not line:
            continue
        try:
            payload = json.loads(line)
        except json.JSONDecodeError as error:
            raise SystemExit(f"{path}:{line_no}: invalid JSON: {error}: {line}") from error
        missing = [field for field in required if field not in payload]
        if missing:
            raise SystemExit(f"{path}:{line_no}: missing fields {missing}")
        count += 1
if count == 0:
    raise SystemExit(f"{path}: no JSON lines found")
PY
}

assert_first_json_line() {
  local path="$1"
  local first="$work_dir/first-json-line.jsonl"
  sed -n '1p' "$path" > "$first"
  assert_json_lines "$first"
}

assert_no_vague_text() {
  local path="$1"
  if grep -Eq 'called `Result::unwrap\\(\\)`|called `Option::unwrap\\(\\)`|PhpExecutionError::|panic at' "$path"; then
    printf '[fail] vague diagnostic text in %s\n' "$path" >&2
    cat "$path" >&2
    exit 1
  fi
}

expect_failure() {
  set +e
  "$@"
  local code=$?
  set -e
  if [ "$code" -eq 0 ]; then
    printf '[fail] expected command to fail: %q\n' "$*" >&2
    exit 1
  fi
}

http_get() {
  local address="$1"
  local path="$2"
  python3 - "$address" "$path" <<'PY'
import socket
import sys

address, path = sys.argv[1], sys.argv[2]
host, port = address.rsplit(":", 1)
with socket.create_connection((host, int(port)), timeout=5) as sock:
    request = f"GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    sock.sendall(request.encode("ascii"))
    chunks = []
    while True:
        data = sock.recv(65536)
        if not data:
            break
        chunks.append(data)
sys.stdout.buffer.write(b"".join(chunks))
PY
}

run_diagnostics_smoke() {
  local tmp="$work_dir/diagnostics"
  rm -rf "$tmp"
  mkdir -p "$tmp"

  expect_failure env PHRUST_ERROR_FORMAT=json "$bin_dir/php-vm" wat >"$tmp/usage.out" 2>"$tmp/usage.err"
  assert_json_lines "$tmp/usage.err"
  grep -q '"code":"E_PHRUST_CLI_USAGE"' "$tmp/usage.err"

  expect_failure env PHRUST_ERROR_FORMAT=json "$bin_dir/php-vm" run "$tmp/missing.php" >"$tmp/read.out" 2>"$tmp/read.err"
  assert_json_lines "$tmp/read.err"
  grep -q 'read source' "$tmp/read.err"
  grep -q 'suggestion' "$tmp/read.err"

  printf "<?php 'unterminated\n" > "$tmp/lexer.php"
  expect_failure env PHRUST_ERROR_FORMAT=json "$bin_dir/php-vm" run "$tmp/lexer.php" >"$tmp/lexer.out" 2>"$tmp/lexer.err"
  assert_json_lines "$tmp/lexer.err"
  grep -q '"code":"E_PHP_PARSE_LEXER_DIAGNOSTIC"' "$tmp/lexer.err"
  grep -q 'unterminated string literal' "$tmp/lexer.err"
  assert_no_vague_text "$tmp/lexer.err"
  cargo test -p php_lexer unterminated_string_diagnostic_has_stable_code --quiet >/dev/null

  printf '<?php $x = ;\n' > "$tmp/parser.php"
  expect_failure "$bin_dir/php-vm" run "$tmp/parser.php" >"$tmp/parser.out" 2>"$tmp/parser.err"
  grep -q 'expected_expression' "$tmp/parser.err"
  assert_no_vague_text "$tmp/parser.err"

  printf '<?php function f($x, $x) {}\n' > "$tmp/semantic.php"
  expect_failure "$bin_dir/php-vm" run "$tmp/semantic.php" >"$tmp/semantic.out" 2>"$tmp/semantic.err"
  grep -q 'duplicate' "$tmp/semantic.err"
  assert_no_vague_text "$tmp/semantic.err"

  printf '<?php $x = ;\n' > "$tmp/ir-unsupported.php"
  expect_failure env PHRUST_ERROR_FORMAT=json "$bin_dir/php-vm" run "$tmp/ir-unsupported.php" >"$tmp/ir-unsupported.out" 2>"$tmp/ir-unsupported.err"
  assert_json_lines "$tmp/ir-unsupported.err"
  grep -q '"code":"E_PHP_IR_UNSUPPORTED_HIR_STATEMENT"' "$tmp/ir-unsupported.err"
  assert_no_vague_text "$tmp/ir-unsupported.err"
  cargo test -p php_ir unsupported_feature_diagnostic_has_shared_envelope --quiet >/dev/null

  printf '<?php missing_func();\n' > "$tmp/runtime.php"
  expect_failure "$bin_dir/php-vm" run "$tmp/runtime.php" >"$tmp/runtime.out" 2>"$tmp/runtime.err"
  grep -q 'E_PHP_RUNTIME_UNDEFINED_FUNCTION' "$tmp/runtime.err"
  assert_no_vague_text "$tmp/runtime.err"

  printf '<?php require "missing.php";\n' > "$tmp/include.php"
  expect_failure "$bin_dir/php-vm" run "$tmp/include.php" >"$tmp/include.out" 2>"$tmp/include.err"
  # Missing requires render php-src-accurate Warning + Fatal error output on
  # stdout (matching the reference oracle) instead of an internal envelope.
  grep -q "Failed opening required 'missing.php'" "$tmp/include.out"
  assert_no_vague_text "$tmp/include.out"
  assert_no_vague_text "$tmp/include.err"

  cargo test -p php_vm vm_step_limit_has_shared_envelope_context --quiet >/dev/null

  expect_failure env PHRUST_SERVER_ERROR_FORMAT=json "$bin_dir/phrust-server" --wat >"$tmp/server-config.out" 2>"$tmp/server-config.err"
  assert_first_json_line "$tmp/server-config.err"
  grep -q '"code":"E_PHRUST_SERVER_CONFIG"' "$tmp/server-config.err"

  mkdir -p "$tmp/docroot"
  printf '<?php missing_func();\n' > "$tmp/docroot/runtime-error.php"
  "$bin_dir/phrust-server" --listen 127.0.0.1:0 --docroot "$tmp/docroot" --debug --error-format json --debug-log "$tmp/server-runtime.jsonl" >"$tmp/server-runtime.out" 2>"$tmp/server-runtime.err" &
  local server_pid=$!
  local address
  address=""
  for _ in {1..100}; do
    address="$(sed -n 's/^listening http:\/\///p' "$tmp/server-runtime.out" | head -n 1)"
    if [ -n "$address" ]; then
      break
    fi
    if ! kill -0 "$server_pid" 2>/dev/null; then
      break
    fi
    sleep 0.05
  done
  if [ -z "$address" ]; then
    printf '[fail] server did not report listening address for runtime-error smoke\n' >&2
    cat "$tmp/server-runtime.out" >&2 || true
    cat "$tmp/server-runtime.err" >&2 || true
    kill "$server_pid" 2>/dev/null || true
    exit 1
  fi
  http_get "$address" "/runtime-error.php" > "$tmp/server-runtime-response.txt"
  kill "$server_pid" 2>/dev/null || true
  wait "$server_pid" 2>/dev/null || true
  grep -q '500 Internal Server Error' "$tmp/server-runtime-response.txt"
  assert_json_lines "$tmp/server-runtime.jsonl"
  grep -q 'req-00000001' "$tmp/server-runtime.jsonl"
  grep -q 'D_PHRUST_SERVER_EXECUTE_END' "$tmp/server-runtime.jsonl"
  grep -q '"runtime_diagnostic_count":"1"' "$tmp/server-runtime.jsonl"
  grep -q 'E_PHP_RUNTIME_UNDEFINED_FUNCTION' "$tmp/server-runtime.jsonl"

  printf '%s\n' '[ok] diagnostics smoke passed.'
}

run_debug_smoke() {
  local tmp="$work_dir/debug"
  rm -rf "$tmp"
  mkdir -p "$tmp"

  "$bin_dir/php-vm" run --debug --error-format json fixtures/runtime/valid/variables/assignment.php >"$tmp/php-vm.out" 2>"$tmp/php-vm.err"
  printf '1\n' > "$tmp/php-vm.expected"
  cmp "$tmp/php-vm.expected" "$tmp/php-vm.out"
  assert_json_lines "$tmp/php-vm.err"
  grep -q 'D_PHRUST_FRONTEND_ANALYZE_START' "$tmp/php-vm.err"
  grep -q 'D_PHRUST_VM_TRACE' "$tmp/php-vm.err"

  env PHRUST_DEBUG=1 PHRUST_ERROR_FORMAT=json "$bin_dir/phrust-php" -r 'echo "ok\n";' >"$tmp/phrust-php.out" 2>"$tmp/phrust-php.err"
  printf 'ok\n' > "$tmp/phrust-php.expected"
  cmp "$tmp/phrust-php.expected" "$tmp/phrust-php.out"
  assert_json_lines "$tmp/phrust-php.err"
  grep -q 'D_PHRUST_VM_TRACE' "$tmp/phrust-php.err"

  env PHRUST_DEBUG_LOG="$tmp/php-vm-debug.jsonl" "$bin_dir/php-vm" run --debug --error-format json fixtures/runtime/valid/hello.php >"$tmp/php-vm-log.out" 2>"$tmp/php-vm-log.err"
  test ! -s "$tmp/php-vm-log.err"
  assert_json_lines "$tmp/php-vm-debug.jsonl"
  grep -q 'D_PHRUST_VM_EXECUTE_END' "$tmp/php-vm-debug.jsonl"

  mkdir -p "$tmp/docroot"
  printf '<?php echo "server ok\\n";\n' > "$tmp/docroot/index.php"
  "$bin_dir/phrust-server" --listen 127.0.0.1:0 --docroot "$tmp/docroot" --debug --error-format json --debug-log "$tmp/server-debug.jsonl" >"$tmp/server.out" 2>"$tmp/server.err" &
  local server_pid=$!
  local address
  address=""
  for _ in {1..100}; do
    address="$(sed -n 's/^listening http:\/\///p' "$tmp/server.out" | head -n 1)"
    if [ -n "$address" ]; then
      break
    fi
    if ! kill -0 "$server_pid" 2>/dev/null; then
      break
    fi
    sleep 0.05
  done
  if [ -z "$address" ]; then
    printf '[fail] server did not report listening address\n' >&2
    cat "$tmp/server.out" >&2 || true
    cat "$tmp/server.err" >&2 || true
    kill "$server_pid" 2>/dev/null || true
    exit 1
  fi
  http_get "$address" "/index.php" > "$tmp/server-response.txt"
  kill "$server_pid" 2>/dev/null || true
  wait "$server_pid" 2>/dev/null || true
  grep -q 'server ok' "$tmp/server-response.txt"
  assert_json_lines "$tmp/server-debug.jsonl"
  grep -q 'req-00000001' "$tmp/server-debug.jsonl"
  grep -q 'D_PHRUST_SERVER_ROUTE_RESOLVED' "$tmp/server-debug.jsonl"
  grep -q 'D_PHRUST_SERVER_SCRIPT_CACHE_END' "$tmp/server-debug.jsonl"
  grep -q 'D_PHRUST_SERVER_EXECUTE_END' "$tmp/server-debug.jsonl"
  grep -q 'D_PHRUST_SERVER_RESPONSE' "$tmp/server-debug.jsonl"

  printf '%s\n' '[ok] debug smoke passed.'
}

if [ "$mode" = "diagnostics" ] || [ "$mode" = "all" ]; then
  run_diagnostics_smoke
fi
if [ "$mode" = "debug" ] || [ "$mode" = "all" ]; then
  run_debug_smoke
fi
