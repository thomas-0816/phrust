#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

cargo build -p php_vm_cli -p php_server --bins >/dev/null

php="$repo_root/target/debug/phrust-php"
docroot="$(mktemp -d "${TMPDIR:-/tmp}/phrust-cli-server.XXXXXX")"
log="$(mktemp "${TMPDIR:-/tmp}/phrust-cli-server-log.XXXXXX")"
cleanup() {
  if [[ -n "${server_pid:-}" ]] && kill -0 "$server_pid" 2>/dev/null; then
    kill "$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true
  fi
  rm -rf "$docroot"
  rm -f "$log"
}
trap cleanup EXIT

cat >"$docroot/index.php" <<'PHP'
<?php echo "index:", $_SERVER["REQUEST_URI"];
PHP
cat >"$docroot/sapi.php" <<'PHP'
<?php echo PHP_SAPI, "|", php_sapi_name();
PHP
cat >"$docroot/router.php" <<'PHP'
<?php
if ($_SERVER["REQUEST_URI"] === "/sapi.php") {
    echo "router-output:", $_SERVER["REQUEST_URI"];
    return true;
}
return false;
PHP
printf 'static-ok\n' >"$docroot/static.txt"

"$php" -S 127.0.0.1:0 -t "$docroot" >"$log" 2>&1 &
server_pid=$!
url=""
for _ in {1..100}; do
  if ! kill -0 "$server_pid" 2>/dev/null; then
    cat "$log" >&2 || true
    exit 1
  fi
  url="$(sed -n 's/^listening //p' "$log" | tail -n 1)"
  [[ -n "$url" ]] && break
  sleep 0.05
done
[[ -n "$url" ]]

curl -fsS "$url/" | grep -q '^index:/$'
curl -fsS "$url/sapi.php" | grep -q '^cli-server|cli-server$'
curl -fsS "$url/static.txt" | grep -q '^static-ok$'
status="$(curl -sS -o /dev/null -w '%{http_code}' "$url/missing.php")"
[[ "$status" == "404" ]]

kill "$server_pid" 2>/dev/null || true
wait "$server_pid" 2>/dev/null || true
server_pid=""
: >"$log"

"$php" -S 127.0.0.1:0 -t "$docroot" "$docroot/router.php" >"$log" 2>&1 &
server_pid=$!
url=""
for _ in {1..100}; do
  if ! kill -0 "$server_pid" 2>/dev/null; then
    cat "$log" >&2 || true
    exit 1
  fi
  url="$(sed -n 's/^listening //p' "$log" | tail -n 1)"
  [[ -n "$url" ]] && break
  sleep 0.05
done
[[ -n "$url" ]]

curl -fsS "$url/sapi.php" | grep -q '^router-output:/sapi.php$'
curl -fsS "$url/" | grep -q '^index:/$'

printf '%s\n' '[ok] phrust-php built-in server smoke passed'
