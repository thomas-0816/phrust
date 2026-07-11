#!/usr/bin/env bash
set -euo pipefail

scripts/stdlib_preflight.py --out target/stdlib/preflight.json
cargo test -p php_std
cargo test -p php_vm std_builtins
cargo build -q -p php_vm_cli --bin php-vm
scripts/stdlib_diff.py --area stdlib --out target/stdlib/diff-stdlib-test --vm-binary "${CARGO_TARGET_DIR:-target}/debug/php-vm"

test -s target/stdlib/preflight.json
grep -q '"version": "8.5.7"' target/stdlib/preflight.json
grep -q '"verify-runtime": true' target/stdlib/preflight.json
grep -q 'crates/php_std' target/stdlib/preflight.json
grep -q '"docs/stdlib/known-gaps.md": true' target/stdlib/preflight.json

test -f docs/stdlib/preflight.md
test -f docs/stdlib/standard-library.md
test -f docs/stdlib/extension-coverage.md
test -f docs/stdlib/function-coverage.md
test -f docs/stdlib/composer-compatibility.md
test -f docs/stdlib/security-capabilities.md
test -f docs/stdlib/known-gaps.md
test -f docs/stdlib/phpt-extension-smoke.md
test -f docs/stdlib/regression-corpus.md
test -f docs/stdlib/stabilization-06-54.md
test -f docs/stdlib/arginfo-coercion.md
test -f docs/stdlib/platform-constants.md
test -f docs/stdlib/validation-summary.md
test -f docs/stdlib/canonical-extension-surfaces.md
test -f scripts/stdlib/diff_builtin_function.php
test -x scripts/stdlib/function_coverage.py
test -x scripts/stdlib/generate_arginfo.py
test -x scripts/stdlib/generate_extension_surfaces.py
test -x scripts/stdlib/verify_generated_extension_surfaces.sh
test -f fixtures/stdlib/extensions/index.json
test -f scripts/stdlib/list_reference_functions.php
test -f scripts/stdlib/list_reference_classes.php
test -f scripts/stdlib/list_reference_constants.php
test -x scripts/stdlib/normalize_php_output.py
test -x scripts/stdlib/composer_source_smoke.sh
test -x scripts/stdlib/phpt_extension_selector.py
test -x scripts/stdlib_diff.py
test -f fixtures/stdlib/phpt_extension_manifest.toml
test -f tests/fixtures/stdlib/corpus/known_gaps.toml
test "$(find tests/fixtures/stdlib/corpus -maxdepth 1 -name '*.php' | wc -l | tr -d ' ')" -ge 7
test -f tests/fixtures/stdlib/_harness/known_gaps.toml
test -f fixtures/stdlib/arginfo_overrides.txt
test "$(find tests/fixtures/stdlib/_harness/stdlib -name '*.php' | wc -l | tr -d ' ')" -ge 5
test "$(find tests/fixtures/stdlib/_harness/streams -name '*.php' | wc -l | tr -d ' ')" -ge 2
test "$(find tests/fixtures/stdlib/_harness/json-pcre-date -name '*.php' | wc -l | tr -d ' ')" -ge 3
test "$(find tests/fixtures/stdlib/_harness/spl-reflection -name '*.php' | wc -l | tr -d ' ')" -ge 2

grep -q 'PHP 8.5.7' docs/stdlib/standard-library.md
grep -q 'php-8.5.7' docs/stdlib/standard-library.md
grep -q 'PHAR' docs/stdlib/standard-library.md
grep -q 'mbstring' docs/stdlib/standard-library.md
grep -q 'intl' docs/stdlib/standard-library.md
grep -q 'DOM/XML' docs/stdlib/standard-library.md
grep -q 'PDO' docs/stdlib/standard-library.md
grep -q 'curl' docs/stdlib/standard-library.md
grep -q 'FPM' docs/stdlib/standard-library.md
grep -q 'nix develop -c just verify-stdlib' docs/stdlib/standard-library.md
grep -q 'generate-extension-surfaces' docs/stdlib/canonical-extension-surfaces.md
grep -q 'verify-generated-extension-surfaces' docs/stdlib/canonical-extension-surfaces.md
grep -q 'composer-smoke-source' docs/stdlib/composer-compatibility.md
grep -q 'PHRUST_STDLIB_COMPOSER_SOURCE_DIR' docs/stdlib/composer-compatibility.md
grep -q 'Standard Library Function Coverage' docs/stdlib/function-coverage.md
grep -q 'stdlib-coverage' docs/stdlib/function-coverage.md
grep -q 'STDLIB-GAP-FULL-PARITY' docs/stdlib/known-gaps.md
grep -q 'extension-phpt-smoke' docs/stdlib/phpt-extension-smoke.md
grep -q 'normalized-report.json' docs/stdlib/phpt-extension-smoke.md
grep -q 'extension-phpt-smoke' docs/stdlib/extension-coverage.md
grep -q 'STDLIB-GAP-EXTENSION-PHPT-PROMOTION' docs/stdlib/known-gaps.md
grep -q 'compat-corpus-smoke' docs/stdlib/regression-corpus.md
grep -q 'reference-output' docs/stdlib/regression-corpus.md
grep -q 'STDLIB_ARRAY_FLIP_WARNING' docs/stdlib/stabilization-06-54.md
grep -q 'STDLIB-GAP-ARRAY-WALK-BY-REF-MUTATION' docs/stdlib/stabilization-06-54.md
grep -q 'STDLIB_CORPUS_JSON_CONFIG' tests/fixtures/stdlib/corpus/json_config.php
grep -q 'purpose:' tests/fixtures/stdlib/corpus/reflection_attributes.php
grep -q 'category = "standard"' fixtures/stdlib/phpt_extension_manifest.toml
grep -q 'category = "spl"' fixtures/stdlib/phpt_extension_manifest.toml
grep -q 'category = "json"' fixtures/stdlib/phpt_extension_manifest.toml
grep -q 'category = "pcre"' fixtures/stdlib/phpt_extension_manifest.toml
grep -q 'category = "date"' fixtures/stdlib/phpt_extension_manifest.toml

for adr in 0011 0012 0013; do
  test -f "docs/adr/${adr}-"*.md
done
grep -q 'ADR 0013' docs/stdlib/standard-library.md
grep -q 'ADR 0013' docs/stdlib/composer-compatibility.md
grep -q 'STDLIB-GAP-PHAR-REQUIRED' docs/stdlib/known-gaps.md

grep -q 'performance_regression_smoke.sh' docs/stdlib/preflight.md
grep -q 'ArgumentValidator' docs/stdlib/arginfo-coercion.md
grep -q 'generate-arginfo' docs/stdlib/arginfo-coercion.md
grep -q 'Strict' docs/stdlib/arginfo-coercion.md
grep -q 'Weak' docs/stdlib/arginfo-coercion.md
grep -q 'PHP_VERSION_ID' docs/stdlib/platform-constants.md
grep -q 'DIRECTORY_SEPARATOR' docs/stdlib/platform-constants.md
grep -q 'diff-streams' docs/stdlib/validation-summary.md
grep -q 'diff-json-pcre-date' docs/stdlib/validation-summary.md
grep -q 'diff-spl-reflection' docs/stdlib/validation-summary.md
grep -q 'STDLIB-GAP-HASH-RANDOM-ALGORITHMS' docs/stdlib/known-gaps.md

scripts/stdlib/generate_arginfo.py \
  --php-src tests/fixtures/stdlib/arginfo/php-src \
  --overrides tests/fixtures/stdlib/arginfo/smoke_overrides.txt \
  --out target/stdlib/generated/arginfo-smoke.rs
grep -q '@generated by scripts/stdlib/generate_arginfo.py' target/stdlib/generated/arginfo-smoke.rs
grep -q 'name: "sort"' target/stdlib/generated/arginfo-smoke.rs
grep -q 'name: "array"' target/stdlib/generated/arginfo-smoke.rs
grep -q 'type_decl: "array"' target/stdlib/generated/arginfo-smoke.rs
grep -q 'by_ref: true' target/stdlib/generated/arginfo-smoke.rs
grep -q 'default_value: Some("SORT_REGULAR")' target/stdlib/generated/arginfo-smoke.rs
grep -q 'variadic: true' target/stdlib/generated/arginfo-smoke.rs

printf '%s\n' '[pass] standard library documentation gate complete'
