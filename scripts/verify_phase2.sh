#!/usr/bin/env bash
set -euo pipefail

exec "$(dirname "$0")/verify-phase2.sh" "$@"
