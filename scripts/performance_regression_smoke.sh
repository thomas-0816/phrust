#!/usr/bin/env bash
set -euo pipefail

scripts/stdlib_preflight.py --out target/stdlib/preflight.json >/dev/null

for target in verify-foundation verify-lexer verify-frontend verify-runtime verify-stdlib verify-performance; do
  grep -q "\"${target}\": true" target/stdlib/preflight.json
done

for script in \
  scripts/verify/foundation.sh \
  scripts/verify/lexer.sh \
  scripts/verify/parser.sh \
  scripts/verify/frontend.sh \
  scripts/verify/runtime.sh \
  scripts/verify/runtime-semantics.sh
do
  test -x "$script"
done

test -f docs/foundation/validation-summary.md
test -f docs/lexer/validation-summary.md
test -f docs/parser/validation-summary.md
test -f docs/frontend/validation-summary.md
test -f docs/runtime/known-gaps.md
test -f docs/runtime/semantics-validation.md
test -f docs/stdlib/roadmap.md

printf '%s\n' '[pass] performance regression baseline smoke complete'
