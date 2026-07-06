#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT}"

printf '%s\n' '[info] runtime semantics verification starts from the runtime baseline.'

just fmt
just lint
just runtime-hardening-lints
just runtime-toolchain-audit
just test
just bytecode-snapshots
just vm-smoke
just vm-trace-smoke
just runtime-fixtures
just runtime-known-gaps
just runtime-semantics-fixtures
just runtime-semantics-diff
just vm-semantics-oracle
test -f docs/runtime/semantics-validation.md
test -f docs/runtime/semantics-coverage-matrix.md
test -f docs/stdlib/roadmap.md
grep -q 'runtime-semantics Coverage Matrix' docs/runtime/semantics-coverage-matrix.md
grep -q 'Unsupported ID Cleanup' docs/runtime/semantics-coverage-matrix.md
grep -q 'Standard Library Roadmap' docs/stdlib/roadmap.md
grep -q 'Standard Library Topics' docs/stdlib/roadmap.md
grep -q 'SPL and Reflection expansion' docs/stdlib/roadmap.md
grep -q 'Bytecode cache' docs/stdlib/roadmap.md
grep -q 'Extension API' docs/stdlib/roadmap.md
grep -q 'Runtime semantics validation' docs/runtime/semantics-validation.md
test -f fixtures/runtime_semantics/phpt_allowlist.toml
grep -q 'category = "references_cow"' fixtures/runtime_semantics/phpt_allowlist.toml
grep -q 'category = "foreach"' fixtures/runtime_semantics/phpt_allowlist.toml
grep -q 'category = "traits"' fixtures/runtime_semantics/phpt_allowlist.toml
grep -q 'category = "enums"' fixtures/runtime_semantics/phpt_allowlist.toml
grep -q 'category = "generators"' fixtures/runtime_semantics/phpt_allowlist.toml
grep -q 'category = "fibers"' fixtures/runtime_semantics/phpt_allowlist.toml
grep -q 'category = "property_hooks"' fixtures/runtime_semantics/phpt_allowlist.toml
grep -q 'category = "reflection"' fixtures/runtime_semantics/phpt_allowlist.toml
test -f fixtures/runtime_semantics/real_world/framework-style-direct-service.php
test -f fixtures/runtime_semantics/real_world/composer-style-autoload-service.php
test -f fixtures/runtime_semantics/real_world/framework-container-reflection-known-gap.php
test -x scripts/minimize_runtime_failure.py
test -f fixtures/runtime_semantics/regressions/pass/array-element-reference-cow.php
test -f fixtures/runtime_semantics/regressions/pass/fiber-suspend-stdout.php
test -f fixtures/runtime_semantics/regressions/known_gaps/object-property-reference.php
grep -q 'regression_category=' fixtures/runtime_semantics/regressions/pass/array-element-reference-cow.php
grep -q 'reference_behavior=' fixtures/runtime_semantics/regressions/pass/array-element-reference-cow.php
grep -q 'regression_case=' fixtures/runtime_semantics/regressions/pass/array-element-reference-cow.php
just runtime-phpt-smoke

printf '%s\n' '[pass] runtime semantics verification complete'
