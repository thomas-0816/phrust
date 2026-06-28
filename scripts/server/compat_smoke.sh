#!/usr/bin/env bash
set -euo pipefail

section="${1:-all}"

case "$section" in
  input|upload|cookie|session|output-buffer|include|static|all)
    ;;
  *)
    printf '[fail] unknown server compat smoke section: %s\n' "$section"
    printf '%s\n' 'usage: scripts/server/compat_smoke.sh [input|upload|cookie|session|output-buffer|include|static|all]'
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
session_dir="$(mktemp -d "${TMPDIR:-/tmp}/phrust-server-compat-sessions.XXXXXX")"
server_pid=""

cleanup() {
  if [[ -n "$server_pid" ]] && kill -0 "$server_pid" >/dev/null 2>&1; then
    kill "$server_pid" >/dev/null 2>&1 || true
    wait "$server_pid" >/dev/null 2>&1 || true
  fi
  rm -f fixtures/server/apps/compat/public/moved-upload.txt
  rm -rf "$session_dir"
  rm -f "$log_file"
}
trap cleanup EXIT

"${CARGO_TARGET_DIR:-target}/debug/phrust-server" \
  --listen 127.0.0.1:0 \
  --docroot fixtures/server/apps/compat/public \
  --front-controller fixtures/server/apps/compat/public/index.php \
  --session-save-path "$session_dir" \
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

run_upload() {
  local upload_file
  local actual
  upload_file="$(mktemp "${TMPDIR:-/tmp}/phrust-server-upload.XXXXXX")"
  printf '%s' 'PNGDATA' >"$upload_file"
  actual="$(
    curl -g -fsS \
      -F 'title=Hello' \
      -F "avatar=@$upload_file;filename=../me.png;type=image/png" \
      "http://$address/upload.php"
  )"
  rm -f "$upload_file"
  local expected=$'title=Hello\nname=me.png\ntype=image/png\nsize=7\nerror=0\nuploaded=yes\nmoved=yes\ncontent=PNGDATA\nuploaded_after=no'
  if [[ "$actual" != "$expected" ]]; then
    printf '[fail] upload expected %q got %q\n' "$expected" "$actual"
    exit 1
  fi
  printf '%s\n' '[ok] server compat upload passed'
}

run_cookie() {
  local headers_file
  local body_file
  local actual
  headers_file="$(mktemp "${TMPDIR:-/tmp}/phrust-server-cookie-headers.XXXXXX")"
  body_file="$(mktemp "${TMPDIR:-/tmp}/phrust-server-cookie-body.XXXXXX")"
  curl -g -fsS \
    -D "$headers_file" \
    -o "$body_file" \
    -H 'Cookie: theme=dark' \
    "http://$address/cookie.php"
  actual="$(cat "$body_file")"
  rm -f "$body_file"
  local expected=$'theme=dark'
  if [[ "$actual" != "$expected" ]]; then
    printf '[fail] cookie expected %q got %q\n' "$expected" "$actual"
    rm -f "$headers_file"
    exit 1
  fi
  local normalized_headers
  normalized_headers="$(tr -d '\r' <"$headers_file")"
  if ! grep -Fiqx 'Set-Cookie: login=hello%20world; Path=/; Secure; HttpOnly; SameSite=Lax' <<<"$normalized_headers"; then
    printf '%s\n' '[fail] cookie response missing encoded login Set-Cookie header'
    printf '%s\n' "$normalized_headers"
    rm -f "$headers_file"
    exit 1
  fi
  if ! grep -Fiqx 'Set-Cookie: raw=a=b; Path=/raw' <<<"$normalized_headers"; then
    printf '%s\n' '[fail] cookie response missing raw Set-Cookie header'
    printf '%s\n' "$normalized_headers"
    rm -f "$headers_file"
    exit 1
  fi
  local cookie_count
  cookie_count="$(grep -Fic 'Set-Cookie:' <<<"$normalized_headers")"
  rm -f "$headers_file"
  if [[ "$cookie_count" != "2" ]]; then
    printf '[fail] cookie expected 2 Set-Cookie headers, got %s\n' "$cookie_count"
    exit 1
  fi
  printf '%s\n' '[ok] server compat cookie passed'
}

run_session() {
  local headers_file
  local body_file
  headers_file="$(mktemp "${TMPDIR:-/tmp}/phrust-server-session-headers.XXXXXX")"
  body_file="$(mktemp "${TMPDIR:-/tmp}/phrust-server-session-body.XXXXXX")"
  curl -g -fsS -D "$headers_file" -o "$body_file" "http://$address/session.php"
  local first_body
  first_body="$(cat "$body_file")"
  local normalized_headers
  normalized_headers="$(tr -d '\r' <"$headers_file")"
  local cookie_pair
  cookie_pair="$(
    printf '%s\n' "$normalized_headers" |
      awk '{ line=tolower($0); if (line ~ /^set-cookie: phpsessid=/) { sub(/^[^:]*: /, ""); sub(/;.*/, ""); print; exit } }'
  )"
  if [[ -z "$cookie_pair" ]]; then
    printf '%s\n' '[fail] session response missing PHPSESSID Set-Cookie header'
    printf '%s\n' "$normalized_headers"
    rm -f "$headers_file" "$body_file"
    exit 1
  fi
  if ! grep -Fiqx "Set-Cookie: $cookie_pair; Path=/; HttpOnly" <<<"$normalized_headers"; then
    printf '%s\n' '[fail] session Set-Cookie header has unexpected attributes'
    printf '%s\n' "$normalized_headers"
    rm -f "$headers_file" "$body_file"
    exit 1
  fi
  local session_id="${cookie_pair#PHPSESSID=}"
  local expected_first
  expected_first=$(printf 'id=%s\nn=1\nstatus=2' "$session_id")
  if [[ "$first_body" != "$expected_first" ]]; then
    printf '[fail] session first request expected %q got %q\n' "$expected_first" "$first_body"
    rm -f "$headers_file" "$body_file"
    exit 1
  fi
  rm -f "$headers_file" "$body_file"

  local second_body
  second_body="$(
    curl -g -fsS \
      -H "Cookie: $cookie_pair" \
      "http://$address/session.php"
  )"
  local expected_second
  expected_second=$(printf 'id=%s\nn=2\nstatus=2' "$session_id")
  if [[ "$second_body" != "$expected_second" ]]; then
    printf '[fail] session second request expected %q got %q\n' "$expected_second" "$second_body"
    exit 1
  fi

  local destroy_body
  destroy_body="$(
    curl -g -fsS \
      -H "Cookie: $cookie_pair" \
      "http://$address/session_destroy.php"
  )"
  local expected_destroy
  expected_destroy=$(printf 'id=%s\ndestroyed=yes' "$session_id")
  if [[ "$destroy_body" != "$expected_destroy" ]]; then
    printf '[fail] session destroy expected %q got %q\n' "$expected_destroy" "$destroy_body"
    exit 1
  fi
  if [[ -e "$session_dir/sess_$session_id" ]]; then
    printf '[fail] session destroy left state file: %s\n' "$session_dir/sess_$session_id"
    exit 1
  fi

  local after_destroy_body
  after_destroy_body="$(
    curl -g -fsS \
      -H "Cookie: $cookie_pair" \
      "http://$address/session.php"
  )"
  if [[ "$after_destroy_body" != "$expected_first" ]]; then
    printf '[fail] session after destroy expected %q got %q\n' "$expected_first" "$after_destroy_body"
    exit 1
  fi
  printf '%s\n' '[ok] server compat session passed'
}

run_output_buffer() {
  assert_body \
    '/output-buffer.php' \
    $'start\nclean=captured\nflush\nouterinnertail\nlevel=2'
  printf '%s\n' '[ok] server compat output-buffer passed'
}

run_include() {
  assert_body '/include-entry.php' 'compat include helper'
  printf '%s\n' '[ok] server compat include passed'
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
    run_upload
    ;;
  cookie)
    run_cookie
    ;;
  session)
    run_session
    ;;
  output-buffer)
    run_output_buffer
    ;;
  include)
    run_include
    ;;
  all)
    run_static
    run_input
    run_upload
    run_cookie
    run_session
    run_output_buffer
    run_include
    ;;
esac
