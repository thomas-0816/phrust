#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
listen="${LISTEN:-127.0.0.1:8080}"
docroot="$repo_root/demo/webserver/public"

cd "$repo_root"

printf 'Starting phrust-server demo on http://%s/\n' "$listen"
printf 'Document root: %s\n\n' "$docroot"
printf 'Try these pages:\n'
printf '  http://%s/\n' "$listen"
printf '  http://%s/calculate.php?a=8&b=5\n' "$listen"
printf '  http://%s/counter.php?n=6\n' "$listen"
printf '  http://%s/response.php\n\n' "$listen"

cargo run -p php_server --bin phrust-server -- \
  --docroot "$docroot" \
  --listen "$listen"
