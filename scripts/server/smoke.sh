#!/usr/bin/env bash
set -euo pipefail

if ! command -v curl >/dev/null 2>&1; then
  printf '%s\n' '[skip] curl is required for server smoke.'
  exit 0
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

cargo build -p php_server --bin phrust-server

docroot="$(mktemp -d "${TMPDIR:-/tmp}/phrust-server-smoke.XXXXXX")"
log_file="$(mktemp "${TMPDIR:-/tmp}/phrust-server-smoke-log.XXXXXX")"
server_pid=""

fail() {
  printf '%s\n' "$1"
  if [[ -s "$log_file" ]]; then
    printf '%s\n' '--- phrust-server log ---'
    cat "$log_file"
  fi
  exit 1
}

cleanup() {
  if [[ -n "$server_pid" ]] && kill -0 "$server_pid" >/dev/null 2>&1; then
    kill "$server_pid" >/dev/null 2>&1 || true
    wait "$server_pid" >/dev/null 2>&1 || true
  fi
  rm -rf "$docroot" "$log_file"
}
trap cleanup EXIT

printf '%s\n' 'static smoke' > "$docroot/static.txt"
printf '%s\n' 'static smoke gzip' > "$docroot/static.txt.gz"
cat > "$docroot/hello.php" <<'PHP'
<?php
echo "hello\n";
PHP
cat > "$docroot/query.php" <<'PHP'
<?php
echo $_GET["name"], "\n";
PHP
cat > "$docroot/preload.txt" <<'EOF'
hello.php
EOF

"${CARGO_TARGET_DIR:-target}/debug/phrust-server" \
  --listen 127.0.0.1:0 \
  --docroot "$docroot" \
  --script-cache-max-entries 8 \
  --script-cache-preload "$docroot/preload.txt" \
  --enable-cache-clear-endpoint \
  >"$log_file" 2>&1 &
server_pid="$!"

address=""
for _ in {1..100}; do
  address="$(sed -n 's/^listening http:\/\///p' "$log_file" | tail -n 1)"
  if [[ -n "$address" ]]; then
    break
  fi
  sleep 0.05
done

if [[ -z "$address" ]]; then
  fail '[fail] server did not print listening address'
fi

assert_body() {
  local path="$1"
  local expected="$2"
  local actual
  if ! actual="$(curl --fail --show-error --silent --connect-timeout 2 --max-time 5 "http://$address$path")"; then
    fail "[fail] request failed: $path"
  fi
  if [[ "$actual" != "$expected" ]]; then
    fail "$(printf '[fail] %s expected %q got %q' "$path" "$expected" "$actual")"
  fi
}

assert_body '/healthz' 'ok'
assert_body '/static.txt' 'static smoke'
assert_body '/hello.php' 'hello'
assert_body '/query.php?name=phrust' 'phrust'

range_body="$(curl -fsS -H 'Range: bytes=0-5' "http://$address/static.txt")"
if [[ "$range_body" != 'static' ]]; then
  printf '[fail] static range expected %q got %q\n' 'static' "$range_body"
  exit 1
fi

precompressed_body="$(curl -fsS -H 'Accept-Encoding: gzip' "http://$address/static.txt")"
if [[ "$precompressed_body" != 'static smoke gzip' ]]; then
  printf '[fail] precompressed static expected %q got %q\n' 'static smoke gzip' "$precompressed_body"
  exit 1
fi

# Warm-worker compiled-unit reuse: the same script served repeatedly must
# hit the process-local compiled-script cache instead of recompiling.
assert_body '/hello.php' 'hello'
assert_body '/hello.php' 'hello'

metrics="$(curl -fsS "http://$address/__phrust/metrics")"
if ! grep -q '^phrust_server_script_cache_preload_successes_total 1$' <<<"$metrics"; then
  printf '%s\n' '[fail] metrics did not report one script-cache preload success'
  printf '%s\n' "$metrics"
  exit 1
fi
script_cache_hits="$(sed -n 's/^phrust_server_script_cache_hits_total \([0-9][0-9]*\)$/\1/p' <<<"$metrics")"
if [[ -z "$script_cache_hits" || "$script_cache_hits" -lt 2 ]]; then
  printf '[fail] expected at least 2 script-cache hits for repeated requests, got %s\n' "${script_cache_hits:-none}"
  printf '%s\n' "$metrics"
  exit 1
fi

clear_body="$(curl -fsS -X POST "http://$address/__phrust/cache/clear")"
if [[ "$clear_body" != 'cache cleared' ]]; then
  printf '[fail] cache clear expected %q got %q\n' 'cache cleared' "$clear_body"
  exit 1
fi

printf '%s\n' '[ok] phrust-server smoke passed'
