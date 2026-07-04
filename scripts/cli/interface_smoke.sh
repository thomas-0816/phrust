#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

cargo build -p php_vm_cli -p php_server --bins >/dev/null

php="$repo_root/target/debug/phrust-php"
tmp="$(mktemp -d "${TMPDIR:-/tmp}/phrust-cli-interface.XXXXXX")"
log="$tmp/server.log"
cleanup() {
  if [[ -n "${server_pid:-}" ]] && kill -0 "$server_pid" 2>/dev/null; then
    kill "$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true
  fi
  rm -rf "$tmp"
}
trap cleanup EXIT

"$php" -v | grep -q 'phrust-php'
"$php" -r 'echo PHP_SAPI, "|", php_sapi_name();' | grep -q '^cli|cli$'
"$php" -r 'echo PHP_BINARY === "" ? "empty" : "ok";' | grep -q '^ok$'
"$php" -m | grep -q '^core$'
"$php" --ini | grep -q 'Loaded Configuration File'

cat >"$tmp/valid.php" <<'PHP'
<?php echo "file:", $argv[1], "\n";
PHP
"$php" -l "$tmp/valid.php" | grep -q 'No syntax errors detected'
"$php" "$tmp/valid.php" arg | grep -q '^file:arg$'

printf '<?php echo "stdin-ok";' | "$php" | grep -q '^stdin-ok$'
printf 'payload' | "$php" -r 'echo stream_get_contents(STDIN);' | grep -q '^payload$'

if "$php" --not-php >/dev/null 2>&1; then
  printf '%s\n' 'expected unknown option to fail' >&2
  exit 1
fi
if "$php" -t "$tmp" >/dev/null 2>&1; then
  printf '%s\n' 'expected -t without -S to fail' >&2
  exit 1
fi

cat >"$tmp/index.php" <<'PHP'
<?php echo "server:", PHP_SAPI, ":", $_SERVER["REQUEST_URI"];
PHP
"$php" -S 127.0.0.1:0 -t "$tmp" >"$log" 2>&1 &
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
curl -fsS "$url/" | grep -q '^server:cli-server:/$'

printf '%s\n' '[ok] phrust-php interface smoke passed'
