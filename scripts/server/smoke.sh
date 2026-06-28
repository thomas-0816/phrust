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

cleanup() {
  if [[ -n "$server_pid" ]] && kill -0 "$server_pid" >/dev/null 2>&1; then
    kill "$server_pid" >/dev/null 2>&1 || true
    wait "$server_pid" >/dev/null 2>&1 || true
  fi
  rm -rf "$docroot" "$log_file"
}
trap cleanup EXIT

printf '%s\n' 'static smoke' > "$docroot/static.txt"
cat > "$docroot/hello.php" <<'PHP'
<?php
echo "hello\n";
PHP
cat > "$docroot/query.php" <<'PHP'
<?php
echo $_GET["name"], "\n";
PHP

"${CARGO_TARGET_DIR:-target}/debug/phrust-server" \
  --listen 127.0.0.1:0 \
  --docroot "$docroot" \
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
  printf '%s\n' '[fail] server did not print listening address'
  cat "$log_file"
  exit 1
fi

assert_body() {
  local path="$1"
  local expected="$2"
  local actual
  actual="$(curl -fsS "http://$address$path")"
  if [[ "$actual" != "$expected" ]]; then
    printf '[fail] %s expected %q got %q\n' "$path" "$expected" "$actual"
    exit 1
  fi
}

assert_body '/healthz' 'ok'
assert_body '/static.txt' 'static smoke'
assert_body '/hello.php' 'hello'
assert_body '/query.php?name=phrust' 'phrust'

printf '%s\n' '[ok] phrust-server smoke passed'
