#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

cargo build --release -p php_server --bin phrust-server

docroot="$(mktemp -d "${TMPDIR:-/tmp}/phrust-server-bench.XXXXXX")"
log_file="$(mktemp "${TMPDIR:-/tmp}/phrust-server-bench-log.XXXXXX")"
server_pid=""

cleanup() {
  if [[ -n "$server_pid" ]] && kill -0 "$server_pid" >/dev/null 2>&1; then
    kill "$server_pid" >/dev/null 2>&1 || true
    wait "$server_pid" >/dev/null 2>&1 || true
  fi
  rm -rf "$docroot" "$log_file"
}
trap cleanup EXIT

cat > "$docroot/hello.php" <<'PHP'
<?php
echo "hello\n";
PHP

"${CARGO_TARGET_DIR:-target}/release/phrust-server" \
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

url="http://$address/hello.php"

if command -v oha >/dev/null 2>&1; then
  oha -n 30 -c 2 "$url"
elif command -v wrk >/dev/null 2>&1; then
  wrk -t1 -c2 -d2s "$url"
elif command -v ab >/dev/null 2>&1; then
  ab -n 30 -c 2 "$url"
elif command -v curl >/dev/null 2>&1; then
  start_ns="$(date +%s)"
  for _ in {1..20}; do
    curl -fsS "$url" >/dev/null
  done
  end_ns="$(date +%s)"
  printf '[ok] curl loop completed 20 requests in %ss\n' "$((end_ns - start_ns))"
else
  printf '%s\n' '[skip] no oha, wrk, ab, or curl available for benchmark smoke.'
  exit 0
fi
