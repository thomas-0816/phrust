#!/usr/bin/env bash
set -euo pipefail

section="${1:-all}"

case "$section" in
  input|upload|cookie|session|output-buffer|static|all)
    ;;
  *)
    printf '[fail] unknown server compat smoke section: %s\n' "$section"
    printf '%s\n' 'usage: scripts/server/compat_smoke.sh [input|upload|cookie|session|output-buffer|static|all]'
    exit 2
    ;;
esac

if ! command -v curl >/dev/null 2>&1; then
  printf '%s\n' '[skip] curl is required for server compat smoke.'
  exit 0
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

cargo build -p php_server --bin phrust-server

log_file="$(mktemp "${TMPDIR:-/tmp}/phrust-server-compat-log.XXXXXX")"
server_pid=""

cleanup() {
  if [[ -n "$server_pid" ]] && kill -0 "$server_pid" >/dev/null 2>&1; then
    kill "$server_pid" >/dev/null 2>&1 || true
    wait "$server_pid" >/dev/null 2>&1 || true
  fi
  rm -f "$log_file"
}
trap cleanup EXIT

"${CARGO_TARGET_DIR:-target}/debug/phrust-server" \
  --listen 127.0.0.1:0 \
  --docroot fixtures/server/apps/compat/public \
  --front-controller fixtures/server/apps/compat/public/index.php \
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
  printf '%s\n' '[fail] compat server did not print listening address'
  cat "$log_file"
  exit 1
fi

assert_body() {
  local path="$1"
  local expected="$2"
  local actual
  actual="$(curl -g -fsS "http://$address$path")"
  if [[ "$actual" != "$expected" ]]; then
    printf '[fail] %s expected %q got %q\n' "$path" "$expected" "$actual"
    exit 1
  fi
}

assert_post_body() {
  local path="$1"
  local body="$2"
  local expected="$3"
  local actual
  actual="$(
    curl -g -fsS \
      -X POST \
      -H 'Content-Type: application/x-www-form-urlencoded' \
      --data "$body" \
      "http://$address$path"
  )"
  if [[ "$actual" != "$expected" ]]; then
    printf '[fail] POST %s expected %q got %q\n' "$path" "$expected" "$actual"
    exit 1
  fi
}

run_static() {
  assert_body '/static.txt' 'compat static fixture'
  printf '%s\n' '[ok] server compat static passed'
}

run_input() {
  assert_post_body \
    '/input.php?user[name]=Ada&ids[]=1&ids[]=2' \
    'form[title]=Hello' \
    $'user=Ada\nids=1,2\npost=Hello\nrequest=Ada'
  printf '%s\n' '[ok] server compat input passed'
}

skip_section() {
  local name="$1"
  printf '[skip] server compat %s awaits its Wave 2 implementation prompt.\n' "$name"
}

case "$section" in
  static)
    run_static
    ;;
  input)
    run_input
    ;;
  upload)
    skip_section upload
    ;;
  cookie)
    skip_section cookie
    ;;
  session)
    skip_section session
    ;;
  output-buffer)
    skip_section output-buffer
    ;;
  all)
    run_static
    run_input
    skip_section upload
    skip_section cookie
    skip_section session
    skip_section output-buffer
    ;;
esac
