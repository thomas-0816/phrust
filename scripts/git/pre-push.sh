#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

if [[ "${PHRUST_SKIP_GIT_HOOKS:-0}" == "1" ]]; then
  printf '%s\n' '[pre-push] skipped via PHRUST_SKIP_GIT_HOOKS=1' >&2
  exit 0
fi

if ! command -v nix >/dev/null 2>&1; then
  printf '%s\n' '[pre-push] nix is required; install Nix or push from a configured development host' >&2
  exit 1
fi

timeout_seconds="${PHRUST_PRE_PUSH_TIMEOUT_SECONDS:-1200}"

printf '[pre-push] running bounded local push gate (timeout: %ss)\n' "$timeout_seconds"
nix develop -c timeout "$timeout_seconds" just pre-push
printf '%s\n' '[pre-push] ok'
