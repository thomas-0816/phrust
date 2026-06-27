set shell := ["bash", "-euo", "pipefail", "-c"]

help:
    @printf '%s\n' \
      'Available commands:' \
      '  just help                 Show this help' \
      '  just check                Format, lint, and run workspace tests' \
      '  just verify               Run the full local verification gate' \
      '  just verify-frontend      Lexer, parser, CST, AST, semantics, CLI snapshots' \
      '  just verify-runtime       IR, VM, runtime fixtures, hardening checks' \
      '  just verify-stdlib        Builtins, streams, JSON/PCRE/date, SPL/reflection' \
      '  just verify-performance   Optimizer, cache, quickening, IC, JIT smoke gates' \
      '  just verify-phpt          PHPT tooling, manifests, and source-integrity checks' \
      '  just fmt                  Check Rust formatting' \
      '  just lint                 Run Rust linting' \
      '  just test                 Run Rust workspace tests' \
      '  just quality              Run additive Rust quality/tooling gates' \
      '  just quality-deps         Check advisories, licenses, bans, sources' \
      '  just quality-unused-deps  Check for unused Cargo dependencies' \
      '  just quality-coverage     Run opt-in cargo-llvm-cov coverage' \
      '  just quality-mutants      Run opt-in cargo-mutants mutation testing' \
      '  just quality-fuzz         Run deterministic fuzz/property smokes' \
      '  just quality-docs         Treat rustdoc warnings and doctests as failures' \
      '  just quality-api          Check public API semver against a git baseline' \
      '  just quality-lints        Report pedantic/nursery Clippy findings' \
      '  just bootstrap-ref        Clone/pin the PHP reference checkout' \
      '  just verify-ref           Verify PHP reference checkout against lockfile' \
      '' \
      'Frontend and syntax:' \
      '  just lexer-fixtures       Run lexer fixture diff' \
      '  just parser-fixtures      Run parser fixture oracle harness' \
      '  just parser-diff          Compare parser acceptance with php -l' \
      '  just cst-roundtrip        Check exact CST reconstruction' \
      '  just semantic-fixtures    Run semantic fixture harness' \
      '  just semantic-diff        Compare semantic acceptance with PHP reference' \
      '  just frontend-snapshots   Run frontend CLI/API snapshot smoke tests' \
      '' \
      'Runtime and VM:' \
      '  just bytecode-snapshots   Run bytecode snapshot checks' \
      '  just vm-smoke             Run VM CLI smoke checks' \
      '  just vm-trace-smoke       Run VM trace/debug smoke checks' \
      '  just runtime-fixtures     Run runtime fixture checks' \
      '  just runtime-diff         Compare runtime output with PHP reference when configured' \
      '  just runtime-known-gaps   Validate runtime known-gap catalog' \
      '' \
      'Standard library and compatibility:' \
      '  just diff-stdlib          Run standard-library differential gate' \
      '  just diff-streams         Run streams differential gate' \
      '  just diff-json-pcre-date  Run JSON/PCRE/Date differential gate' \
      '  just diff-spl-reflection  Run SPL/Reflection differential gate' \
      '  just composer-smoke       Run Composer compatibility smoke gate' \
      '' \
      'Performance:' \
      '  just perf-flag-matrix     Run performance flag A/B matrix' \
      '  just cache-roundtrip      Run bytecode-cache roundtrip gate' \
      '  just optimizer-diff       Run optimizer differential gate' \
      '  just quickening-smoke     Run quickening smoke gate' \
      '  just inline-cache-smoke   Run inline-cache smoke gate' \
      '  just jit-smoke            Run default-off JIT smoke gate' \
      '  just perf-report          Generate performance report' \
      '' \
      'PHPT:' \
      '  just phpt-index           Index the PHPT corpus' \
      '  just phpt-source-index    Index pinned php-src source and PHPT hashes' \
      '  just phpt-runner-smoke    Run PHPT runner tests' \
      '  just phpt-reference-smoke Run Reference PHP smoke' \
      '  just phpt-target-smoke    Run Target PHP smoke' \
      '  just phpt-build           Build PHPT runner and target binaries' \
      '  just phpt-dev-build       Build PHPT binaries with local incremental mode' \
      '  just phpt-dev-shell       Open one nix shell after building PHPT dev binaries' \
      '  just phpt-generate-module MODULE=<module>  Generate module tests' \
      '  just phpt-module MODULE=<module>           Run a module batch' \
      '  just phpt-dev-module MODULE=<module>       Run module batch after dev build' \
      '  just phpt-module-target MODULE=<module>    Run target-only module batch' \
      '  just phpt-fast MODULE=<module> [FILE=<path>|PATTERN=<text>]  Run fast target-only module loop' \
      '  just phpt-dev-fast MODULE=<module> [...]   Run fast loop with explicit dev PASS reuse' \
      '  just phpt-rerun-failures MODULE=<module>   Rerun only last non-green module outcomes' \
      '  just phpt-triage         Generate PHPT triage report and module plan' \
      '  just phpt-full-regression Run the full PHPT no-regression gate with PHPT_RUN_FULL=1' \
      '  just phpt-full-fast      Run full PHPT gate after dev build with explicit local reuse' \
      '  just phpt-verify-baseline Verify committed PHPT full baseline files' \
      '  just phpt-verify-source-integrity Verify pinned php-src was not mutated' \
      '  just install-hooks       Install versioned git hooks' \
      '  just ci-local            Run the local GitHub Actions parity gate'

install-hooks:
    scripts/git/install-hooks.sh

fmt:
    cargo fmt --all --check

lint:
    cargo clippy --workspace --all-targets -- -D warnings

runtime-hardening-lints:
    cargo clippy -p php_runtime -p php_vm --all-targets -- -D warnings -D unsafe-code

test:
    cargo test --workspace

test-lexer:
    cargo test -p php_lexer

check:
    @just fmt
    @just lint
    @just test

quality:
    @just quality-deps
    @just quality-unused-deps
    @just quality-coverage
    @just quality-mutants
    @just quality-fuzz
    @just quality-docs
    @just quality-api
    @just quality-lints

quality-deps:
    @if ! command -v cargo-deny >/dev/null 2>&1; then \
      printf '%s\n' '[skip] cargo-deny unavailable; enter nix develop or install cargo-deny.'; \
      exit 0; \
    fi; \
    cargo deny check advisories bans licenses sources

quality-unused-deps:
    @if ! command -v cargo-machete >/dev/null 2>&1; then \
      printf '%s\n' '[skip] cargo-machete unavailable; enter nix develop or install cargo-machete.'; \
      exit 0; \
    fi; \
    cargo machete

quality-coverage:
    @if [[ "${PHRUST_RUN_COVERAGE:-0}" != "1" ]]; then \
      printf '%s\n' '[skip] set PHRUST_RUN_COVERAGE=1 to run cargo-llvm-cov coverage.'; \
      exit 0; \
    fi; \
    if ! command -v cargo-llvm-cov >/dev/null 2>&1; then \
      printf '%s\n' '[skip] cargo-llvm-cov unavailable; enter nix develop or install cargo-llvm-cov.'; \
      exit 0; \
    fi; \
    if cargo nextest --version >/dev/null 2>&1; then \
      cargo llvm-cov nextest --workspace --summary-only; \
    else \
      cargo llvm-cov --workspace --summary-only; \
    fi

quality-mutants:
    @if [[ "${PHRUST_RUN_MUTANTS:-0}" != "1" ]]; then \
      printf '%s\n' '[skip] set PHRUST_RUN_MUTANTS=1 to run cargo-mutants mutation testing.'; \
      exit 0; \
    fi; \
    if command -v cargo-mutants >/dev/null 2>&1; then \
      cargo-mutants --workspace; \
    elif cargo mutants --version >/dev/null 2>&1; then \
      cargo mutants --workspace; \
    else \
      printf '%s\n' '[skip] cargo-mutants unavailable; enter nix develop or install cargo-mutants.'; \
      exit 0; \
    fi

quality-fuzz:
    @just fuzz-lexer-smoke
    @just fuzz-parser-smoke
    @just runtime-fuzz-smoke
    @just fuzz-vm-smoke
    @if command -v cargo-fuzz >/dev/null 2>&1 || cargo fuzz --version >/dev/null 2>&1; then \
      printf '%s\n' '[ok] cargo-fuzz is available for coverage-guided fuzz target expansion.'; \
    else \
      printf '%s\n' '[skip] cargo-fuzz unavailable; deterministic fuzz/property smokes ran, but coverage-guided cargo-fuzz is not installed.'; \
    fi

quality-docs:
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --lib --no-deps
    cargo test --doc --workspace

quality-api:
    @baseline="${PHRUST_SEMVER_BASELINE:-HEAD}"; \
    if command -v cargo-semver-checks >/dev/null 2>&1 || cargo semver-checks --version >/dev/null 2>&1; then \
      cargo semver-checks check-release --workspace --baseline-rev "$baseline"; \
    else \
      printf '%s\n' '[skip] cargo-semver-checks unavailable; enter nix develop or install cargo-semver-checks.'; \
      exit 0; \
    fi

quality-lints:
    cargo clippy --workspace --all-targets -- \
      -W clippy::pedantic \
      -W clippy::nursery \
      -A clippy::missing_errors_doc \
      -A clippy::missing_panics_doc \
      -A clippy::module_name_repetitions \
      -A clippy::must_use_candidate \
      -A clippy::too_many_lines

verify:
    @just check
    @just verify-frontend
    @just verify-runtime
    @just verify-stdlib
    @just verify-performance
    @just verify-phpt

ci-rust:
    @just fmt
    @just lint
    @just test

ci-domain-gates:
    @just verify-frontend
    @just verify-runtime
    @just verify-stdlib
    @just verify-performance

ci-phpt-smoke:
    scripts/phpt/ci_smoke.sh

ci-local:
    @just ci-rust
    @just ci-domain-gates
    @just ci-phpt-smoke

verify-frontend:
    @just lexer-fixtures
    @just parser-fixtures
    @just cst-roundtrip
    @just semantic-fixtures
    @just semantic-diff
    @just frontend-snapshots

verify-runtime:
    @just bytecode-snapshots
    @just vm-smoke
    @just vm-trace-smoke
    @just runtime-fixtures
    @just runtime-known-gaps
    @just runtime-semantics-fixtures
    @just runtime-semantics-diff
    @just runtime-hardening-lints

verify-stdlib:
    @just stdlib-docs
    @just stdlib-coverage
    @just diff-stdlib
    @just diff-streams
    @just diff-json-pcre-date
    @just diff-spl-reflection

verify-performance:
    @just performance-tests
    @just performance-regression
    @just perf-flag-matrix
    @just benchmark-smoke
    @just cache-roundtrip
    @just optimizer-diff
    @just quickening-smoke
    @just inline-cache-smoke
    @just callgrind-smoke
    @just jit-smoke
    @just safety-audit-smoke
    @just hotpath-inventory
    @just perf-report
    @printf '%s\n' '[pass] performance verification complete'

verify-phpt:
    scripts/phpt/verify_foundation.sh
    @just phpt-verify-baseline
    scripts/phpt/verify_source_integrity.sh
    cargo test -p php_phpt_tools

verify-foundation:
    scripts/verify/foundation.sh

verify-lexer:
    scripts/verify/lexer.sh

bootstrap-ref:
    scripts/bootstrap-php-reference.sh

verify-ref:
    scripts/verify-php-reference.sh

dump-reference-tokens:
    @php_bin="${REFERENCE_PHP:-}"; \
    if [[ -z "$php_bin" ]]; then \
      if [[ -x third_party/php-src/sapi/cli/php ]]; then php_bin="third_party/php-src/sapi/cli/php"; \
      elif command -v php >/dev/null 2>&1; then php_bin="$(command -v php)"; \
      else printf '%s\n' 'No PHP binary found. Set REFERENCE_PHP or build third_party/php-src/sapi/cli/php.' >&2; exit 1; fi; \
    fi; \
    "$php_bin" scripts/dump-reference-tokens.php

tokenize-ref file:
    @php_bin="${REFERENCE_PHP:-}"; \
    if [[ -z "$php_bin" ]]; then \
      if [[ -x third_party/php-src/sapi/cli/php ]]; then php_bin="third_party/php-src/sapi/cli/php"; \
      elif command -v php >/dev/null 2>&1; then php_bin="$(command -v php)"; \
      else printf '%s\n' 'No PHP binary found. Set REFERENCE_PHP or build third_party/php-src/sapi/cli/php.' >&2; exit 1; fi; \
    fi; \
    "$php_bin" scripts/tokenize-reference.php --file "{{file}}"

lexer-ref file:
    @just tokenize-ref "{{file}}"

lex file:
    cargo run -p php_lexer_cli -- --file "{{file}}" --pretty

lexer-fixtures:
    scripts/compare-lexer-fixtures.py

lexer-diff:
    scripts/compare-lexer-fixtures.py

lexer-diff-report:
    scripts/compare-lexer-fixtures.py --all-diffs --json-report target/lexer-diff-report.json

fuzz-lexer-smoke:
    cargo test -p php_lexer lexer_invariants

bench-lexer:
    cargo bench -p php_lexer --bench lexer_throughput

lexer-corpus-smoke:
    scripts/lexer-corpus-smoke.py

parser-lint-oracle:
    scripts/run_parser_fixtures.py

parser-fixtures:
    scripts/run_parser_fixtures.py

parser-diff:
    scripts/compare_parser_acceptance.py --strict

cst-roundtrip:
    cargo test -p php_syntax --test fixture_roundtrip

extract-parser-corpus:
    scripts/extract_parser_corpus.py

parser-corpus-smoke:
    scripts/run_parser_corpus_smoke.py

fuzz-parser-smoke:
    cargo test -p php_syntax --test parser_properties
    PARSER_FUZZ_CASES=1024 cargo test -p php_syntax --test parser_properties -- --ignored

bench-parser:
    cargo test -p php_syntax --test perf_smoke -- --ignored --nocapture

parser-snapshots:
    UPDATE_PARSER_SNAPSHOTS=1 cargo test -p php_syntax --test parser_snapshots
    UPDATE_PARSER_SNAPSHOTS=1 cargo test -p php_syntax --test diagnostic_snapshots

extract-ref-metadata:
    scripts/extract-php-reference-metadata.py --php-src third_party/php-src --out references/php-src.metadata.json

build-ref-php:
    scripts/build-reference-php.sh

ref-php-version:
    @if [[ -x third_party/php-src/sapi/cli/php ]]; then \
      third_party/php-src/sapi/cli/php -v; \
    else \
      printf '%s\n' 'Reference PHP CLI is not built; run `nix develop -c just build-ref-php`.'; \
      exit 1; \
    fi

ref-tokenizer-check:
    @if [[ -x third_party/php-src/sapi/cli/php ]]; then \
      third_party/php-src/sapi/cli/php -r 'var_export(function_exists("token_get_all")); echo "\n";'; \
    else \
      printf '%s\n' 'Reference PHP CLI is not built; run `nix develop -c just build-ref-php`.'; \
      exit 1; \
    fi

phpt-index *args:
    cargo build -q -p php_phpt_tools --bin php-phpt-tools
    ${CARGO_TARGET_DIR:-target}/debug/php-phpt-tools phpt-index {{args}}

phpt-source-index *args:
    cargo build -q -p php_phpt_tools --bin php-phpt-tools
    ${CARGO_TARGET_DIR:-target}/debug/php-phpt-tools source-index {{args}}
    ${CARGO_TARGET_DIR:-target}/debug/php-phpt-tools symbol-index {{args}}

phpt-runner-smoke *args:
    scripts/phpt/runner_smoke.sh

phpt-reference-smoke *args:
    scripts/phpt/binary_smoke.sh reference

phpt-target-smoke *args:
    scripts/phpt/binary_smoke.sh target

phpt-build:
    cargo build -q -p php_phpt_tools --bin php-phpt-tools -p php_vm_cli --bin phrust-php

phpt-dev-build:
    RUSTC_WRAPPER= CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-1}" cargo build -q -p php_phpt_tools --bin php-phpt-tools -p php_vm_cli --bin phrust-php

phpt-dev-shell:
    @if [[ -n "${IN_NIX_SHELL:-}" ]]; then \
      just phpt-dev-build; \
      exec "${SHELL:-bash}"; \
    else \
      nix develop -c bash -lc 'just phpt-dev-build; exec "${SHELL:-bash}"'; \
    fi

phpt-generate-module *args:
    scripts/phpt/generate_module.sh {{args}}

phpt-module *args:
    scripts/phpt/module_run.sh {{args}}

phpt-dev-module *args:
    @PHPT_SKIP_BUILD=1 PHPT_REUSE_LAST="${PHPT_REUSE_LAST:-1}" PHPT_TIMEOUT_SECONDS="${PHPT_TIMEOUT_SECONDS:-10}" PHPT_WORK_DIR="${PHPT_WORK_DIR:-/private/tmp/phrust-phpt-work}" scripts/phpt/module_run.sh {{args}}

phpt-module-target *args:
    scripts/phpt/module_target.sh {{args}}

phpt-fast *args:
    @PHPT_REQUIRE_FOCUS=1 PHPT_SKIP_BUILD=1 PHPT_REUSE_LAST="${PHPT_REUSE_LAST:-1}" PHPT_TIMEOUT_SECONDS="${PHPT_TIMEOUT_SECONDS:-3}" PHPT_WORK_DIR="${PHPT_WORK_DIR:-/private/tmp/phrust-phpt-work}" scripts/phpt/module_target.sh {{args}}

phpt-dev-fast *args:
    @PHPT_REQUIRE_FOCUS=1 PHPT_SKIP_BUILD=1 PHPT_DEV_REUSE_PASS=1 PHPT_REUSE_LAST="${PHPT_REUSE_LAST:-1}" PHPT_TIMEOUT_SECONDS="${PHPT_TIMEOUT_SECONDS:-3}" PHPT_WORK_DIR="${PHPT_WORK_DIR:-/private/tmp/phrust-phpt-work}" scripts/phpt/module_target.sh {{args}}

phpt-rerun-failures *args:
    @PHPT_SKIP_BUILD=1 PHPT_TIMEOUT_SECONDS="${PHPT_TIMEOUT_SECONDS:-3}" PHPT_WORK_DIR="${PHPT_WORK_DIR:-/private/tmp/phrust-phpt-work}" scripts/phpt/rerun_failures.sh {{args}}

phpt-full-regression *args:
    scripts/phpt/full_regression.sh {{args}}

phpt-full-fast *args:
    @PHPT_RUN_FULL=1 PHPT_SKIP_BUILD=1 PHPT_DEV_REUSE_PASS=1 PHPT_TIMEOUT_SECONDS="${PHPT_TIMEOUT_SECONDS:-30}" PHPT_WORK_DIR="${PHPT_WORK_DIR:-/private/tmp/phrust-phpt-work}" scripts/phpt/full_regression.sh {{args}}

phpt-triage *args:
    cargo build -q -p php_phpt_tools --bin php-phpt-tools
    ${CARGO_TARGET_DIR:-target}/debug/php-phpt-tools triage {{args}}

phpt-verify-baseline:
    cargo build -q -p php_phpt_tools --bin php-phpt-tools
    ${CARGO_TARGET_DIR:-target}/debug/php-phpt-tools verify-baseline

phpt-verify-source-integrity:
    scripts/phpt/verify_source_integrity.sh

phpt-source-lookup *args:
    cargo build -q -p php_phpt_tools --bin php-phpt-tools
    ${CARGO_TARGET_DIR:-target}/debug/php-phpt-tools lookup-symbol {{args}}

phpt-official-smoke *args:
    scripts/phpt/official_smoke.sh {{args}}

stdlib-docs:
    scripts/stdlib-docs.sh

stdlib-coverage:
    scripts/stdlib-coverage.sh

generate-arginfo php_src="third_party/php-src" out="crates/php_std/src/generated/arginfo.rs":
    scripts/stdlib/generate_arginfo.py --php-src "{{php_src}}" --overrides fixtures/stdlib/arginfo_overrides.txt --out "{{out}}"
    rustfmt --edition 2024 "{{out}}"

diff-stdlib:
    cargo build -q -p php_vm_cli --bin php-vm
    scripts/stdlib_diff.py --area stdlib --out target/stdlib/diff-stdlib --vm-binary ${CARGO_TARGET_DIR:-target}/debug/php-vm

diff-streams:
    cargo build -q -p php_vm_cli --bin php-vm
    scripts/stdlib_diff.py --area streams --out target/stdlib/diff-streams --vm-binary ${CARGO_TARGET_DIR:-target}/debug/php-vm

diff-json-pcre-date:
    cargo build -q -p php_vm_cli --bin php-vm
    scripts/stdlib_diff.py --area json-pcre-date --out target/stdlib/diff-json-pcre-date --vm-binary ${CARGO_TARGET_DIR:-target}/debug/php-vm

diff-spl-reflection:
    cargo build -q -p php_vm_cli --bin php-vm
    scripts/stdlib_diff.py --area spl-reflection --out target/stdlib/diff-spl-reflection --vm-binary ${CARGO_TARGET_DIR:-target}/debug/php-vm

composer-smoke:
    cargo build -q -p php_vm_cli --bin php-vm
    scripts/stdlib_diff.py --area composer --out target/stdlib/composer-smoke --vm-binary ${CARGO_TARGET_DIR:-target}/debug/php-vm

composer-fixture-prepare:
    scripts/stdlib/prepare_composer_fixture.sh

composer-smoke-source:
    scripts/stdlib/composer_source_smoke.sh

composer-smoke-autoload:
    cargo build -q -p php_vm_cli --bin php-vm
    scripts/stdlib_diff.py --file tests/fixtures/stdlib/_harness/composer/basic_project_autoload_order.php --out target/stdlib/composer-smoke-autoload --vm-binary ${CARGO_TARGET_DIR:-target}/debug/php-vm

composer-smoke-platform:
    cargo build -q -p php_vm_cli --bin php-vm
    scripts/stdlib_diff.py --file tests/fixtures/stdlib/_harness/composer/basic_project_platform_check.php --file tests/fixtures/stdlib/_harness/composer/platform_version_compare.php --out target/stdlib/composer-smoke-platform --vm-binary ${CARGO_TARGET_DIR:-target}/debug/php-vm

process-capability-smoke:
    scripts/stdlib/process_capability_smoke.sh

semantic-fixtures:
    scripts/run_semantic_fixtures.py
    scripts/run_semantic_fixtures.py --write-snapshots

semantic-reference-smoke:
    scripts/reference_php_frontend_json.py --file fixtures/semantic/valid/minimal.php

semantic-diff:
    scripts/compare_semantic_acceptance.py

semantic-diff-strict:
    scripts/compare_semantic_acceptance.py --strict

frontend-snapshots:
    cargo build -p php_frontend_cli
    ${CARGO_TARGET_DIR:-target}/debug/php-frontend --help >/dev/null
    ${CARGO_TARGET_DIR:-target}/debug/php-frontend analyze fixtures/semantic/valid/hello.php --format json >/dev/null
    ${CARGO_TARGET_DIR:-target}/debug/php-frontend diagnostics fixtures/semantic/functions/duplicate-param-invalid.php --format json >/dev/null
    ${CARGO_TARGET_DIR:-target}/debug/php-frontend symbols fixtures/semantic/classes/basic.php --format json >/dev/null
    ${CARGO_TARGET_DIR:-target}/debug/php-frontend scopes fixtures/semantic/scopes/closure-use.php --format json >/dev/null
    ${CARGO_TARGET_DIR:-target}/debug/php-frontend hir fixtures/semantic/php85/clone-with.php --format json >/dev/null
    ${CARGO_TARGET_DIR:-target}/debug/php-frontend snapshot fixtures/semantic/valid/minimal.php --output target/frontend-minimal.snap >/dev/null
    test -s target/frontend-minimal.snap

semantic-corpus-smoke:
    @printf '%s\n' '[skip] semantic corpus smoke is not configured for Semantic frontend; curated fixtures are covered by semantic-fixtures.'

fuzz-frontend-smoke:
    @printf '%s\n' '[skip] frontend fuzz smoke is not configured for Semantic frontend; parser fuzz smoke remains available via just fuzz-parser-smoke.'

bench-frontend:
    @printf '%s\n' '[skip] frontend benchmarks are not configured for Semantic frontend; no benchmark baseline is defined yet.'

bytecode-snapshots:
    cargo test -p php_ir --test bytecode_snapshots -- --nocapture

vm-smoke:
    cargo build -p php_vm_cli
    @tmp_dir="$PWD/target/vm-smoke"; \
    mkdir -p "$tmp_dir"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm compile fixtures/runtime/valid/hello.php --json > "$tmp_dir/hello.json"; \
    grep -q '"ok":true' "$tmp_dir/hello.json"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm dump-ir fixtures/runtime/valid/hello.php > "$tmp_dir/hello.ir"; \
    grep -q 'echo r0' "$tmp_dir/hello.ir"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/hello.php > "$tmp_dir/hello.out"; \
    printf 'hello runtime\n' > "$tmp_dir/hello.expected"; \
    cmp "$tmp_dir/hello.expected" "$tmp_dir/hello.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/scalars/echo.php > "$tmp_dir/scalar.out"; \
    printf 'scalar echo\n' > "$tmp_dir/scalar.expected"; \
    cmp "$tmp_dir/scalar.expected" "$tmp_dir/scalar.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/bytecode/lower/valid/empty.php > "$tmp_dir/empty.out"; \
    test ! -s "$tmp_dir/empty.out"; \
    printf '%s\n' '[ok] runtime VM smoke fixtures passed.'

vm-trace-smoke:
    cargo build -p php_vm_cli
    @tmp_dir="$PWD/target/runtime/failures"; \
    mkdir -p "$tmp_dir"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run --trace fixtures/runtime/valid/variables/assignment.php > "$tmp_dir/trace-smoke.out" 2> "$tmp_dir/trace-smoke.trace"; \
    printf '1\n' > "$tmp_dir/trace-smoke.expected"; \
    cmp "$tmp_dir/trace-smoke.expected" "$tmp_dir/trace-smoke.out"; \
    grep -q 'vm-trace:' "$tmp_dir/trace-smoke.trace"; \
    grep -q 'function=main(0)' "$tmp_dir/trace-smoke.trace"; \
    grep -q 'output_len=' "$tmp_dir/trace-smoke.trace"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm dump-ir fixtures/runtime/valid/variables/assignment.php --with-source > "$tmp_dir/trace-smoke.ir"; \
    grep -q '^source path=' "$tmp_dir/trace-smoke.ir"; \
    grep -q '^source 0001:' "$tmp_dir/trace-smoke.ir"; \
    grep -q '^--- ir ---' "$tmp_dir/trace-smoke.ir"; \
    printf '%s\n' '[ok] runtime VM trace/debug smoke passed.'

runtime-fixtures:
    cargo build -p php_vm_cli
    @tmp_dir="target/runtime-fixtures"; \
    mkdir -p "$tmp_dir"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/hello.php > "$tmp_dir/hello.out"; \
    printf 'hello runtime\n' > "$tmp_dir/hello.expected"; \
    cmp "$tmp_dir/hello.expected" "$tmp_dir/hello.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/scalars/echo.php > "$tmp_dir/echo.out"; \
    printf 'scalar echo\n' > "$tmp_dir/echo.expected"; \
    cmp "$tmp_dir/echo.expected" "$tmp_dir/echo.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/scalars/expressions.php > "$tmp_dir/expressions.out"; \
    printf '7|8|ab|1|-1\n' > "$tmp_dir/expressions.expected"; \
    cmp "$tmp_dir/expressions.expected" "$tmp_dir/expressions.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/scalars/comparisons.php > "$tmp_dir/comparisons.out"; \
    printf '1|1|1|1|-1\n' > "$tmp_dir/comparisons.expected"; \
    cmp "$tmp_dir/comparisons.expected" "$tmp_dir/comparisons.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/scalars/casts.php > "$tmp_dir/casts.out"; \
    printf '12|1|\n' > "$tmp_dir/casts.expected"; \
    cmp "$tmp_dir/casts.expected" "$tmp_dir/casts.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/variables/assignment.php > "$tmp_dir/variables-assignment.out"; \
    printf '1\n' > "$tmp_dir/variables-assignment.expected"; \
    cmp "$tmp_dir/variables-assignment.expected" "$tmp_dir/variables-assignment.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/variables/compound.php > "$tmp_dir/variables-compound.out"; \
    printf '3x\n' > "$tmp_dir/variables-compound.expected"; \
    cmp "$tmp_dir/variables-compound.expected" "$tmp_dir/variables-compound.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/variables/inc-dec.php > "$tmp_dir/variables-inc-dec.out"; \
    printf '1|3|3|1\n' > "$tmp_dir/variables-inc-dec.expected"; \
    cmp "$tmp_dir/variables-inc-dec.expected" "$tmp_dir/variables-inc-dec.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/superglobals/argc.php > "$tmp_dir/superglobals-argc.out"; \
    printf '1\n' > "$tmp_dir/superglobals-argc.expected"; \
    cmp "$tmp_dir/superglobals-argc.expected" "$tmp_dir/superglobals-argc.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/superglobals/argv.php -- alpha beta > "$tmp_dir/superglobals-argv.out"; \
    printf '3|alpha|beta\n' > "$tmp_dir/superglobals-argv.expected"; \
    cmp "$tmp_dir/superglobals-argv.expected" "$tmp_dir/superglobals-argv.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/superglobals/server-argv.php -- red > "$tmp_dir/superglobals-server-argv.out"; \
    printf '2|red\n' > "$tmp_dir/superglobals-server-argv.expected"; \
    cmp "$tmp_dir/superglobals-server-argv.expected" "$tmp_dir/superglobals-server-argv.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/superglobals/empty-superglobals.php > "$tmp_dir/superglobals-empty.out"; \
    printf 'get-empty|post-empty|request-empty|env-empty\n' > "$tmp_dir/superglobals-empty.expected"; \
    cmp "$tmp_dir/superglobals-empty.expected" "$tmp_dir/superglobals-empty.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/references/by-value.php > "$tmp_dir/references-by-value.out"; \
    printf '12\n' > "$tmp_dir/references-by-value.expected"; \
    cmp "$tmp_dir/references-by-value.expected" "$tmp_dir/references-by-value.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/references/local-alias.php > "$tmp_dir/references-local-alias.out"; \
    printf '23\n' > "$tmp_dir/references-local-alias.expected"; \
    cmp "$tmp_dir/references-local-alias.expected" "$tmp_dir/references-local-alias.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/constants/global.php > "$tmp_dir/constants-global.out"; \
    printf '42|runtime\n' > "$tmp_dir/constants-global.expected"; \
    cmp "$tmp_dir/constants-global.expected" "$tmp_dir/constants-global.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/constants/builtin.php > "$tmp_dir/constants-builtin.out"; \
    printf '8.5.7\n' > "$tmp_dir/constants-builtin.expected"; \
    cmp "$tmp_dir/constants-builtin.expected" "$tmp_dir/constants-builtin.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/constants/magic-top-level.php > "$tmp_dir/constants-magic-top.out"; \
    printf '%s\n%s\n4||||\n' "$PWD/fixtures/runtime/valid/constants/magic-top-level.php" "$PWD/fixtures/runtime/valid/constants" > "$tmp_dir/constants-magic-top.expected"; \
    cmp "$tmp_dir/constants-magic-top.expected" "$tmp_dir/constants-magic-top.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/constants/magic-function.php > "$tmp_dir/constants-magic-function.out"; \
    printf 'magic_function_fixture|3||magic_function_fixture|\n' > "$tmp_dir/constants-magic-function.expected"; \
    cmp "$tmp_dir/constants-magic-function.expected" "$tmp_dir/constants-magic-function.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/constants/magic-method.php > "$tmp_dir/constants-magic-method.out"; \
    printf 'MagicMethodFixture|MagicMethodFixture::show\n' > "$tmp_dir/constants-magic-method.expected"; \
    cmp "$tmp_dir/constants-magic-method.expected" "$tmp_dir/constants-magic-method.out"; \
    set +e; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/invalid/constants/undefined.php > "$tmp_dir/constants-undefined.out" 2> "$tmp_dir/constants-undefined.err"; \
    code=$?; \
    set -e; \
    test "$code" -eq 3; \
    grep -q 'E_PHP_RUNTIME_UNDEFINED_CONSTANT' "$tmp_dir/constants-undefined.err"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/known_gaps/variables/undefined.php > "$tmp_dir/variables-undefined.out" 2> "$tmp_dir/variables-undefined.err"; \
    sed -E 's/on line [0-9]+/on line <line>/' "$tmp_dir/variables-undefined.out" > "$tmp_dir/variables-undefined.normalized"; \
    printf '\nWarning: Undefined variable $missing in %s/fixtures/runtime/known_gaps/variables/undefined.php on line <line>\nx\n' "$PWD" > "$tmp_dir/variables-undefined.expected"; \
    cmp "$tmp_dir/variables-undefined.expected" "$tmp_dir/variables-undefined.normalized"; \
    test ! -s "$tmp_dir/variables-undefined.err"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/errors/warning-continuation.php > "$tmp_dir/errors-warning-continuation.out" 2> "$tmp_dir/errors-warning-continuation.err"; \
    sed -E 's/on line [0-9]+/on line <line>/' "$tmp_dir/errors-warning-continuation.out" > "$tmp_dir/errors-warning-continuation.normalized"; \
    printf '\nWarning: Undefined variable $missing in %s/fixtures/runtime/valid/errors/warning-continuation.php on line <line>\nok\n' "$PWD" > "$tmp_dir/errors-warning-continuation.expected"; \
    cmp "$tmp_dir/errors-warning-continuation.expected" "$tmp_dir/errors-warning-continuation.normalized"; \
    test ! -s "$tmp_dir/errors-warning-continuation.err"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/control_flow/if-true-false.php > "$tmp_dir/control-if.out"; \
    printf 'tf\n' > "$tmp_dir/control-if.expected"; \
    cmp "$tmp_dir/control-if.expected" "$tmp_dir/control-if.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/control_flow/nested-if.php > "$tmp_dir/control-nested-if.out"; \
    printf 'nested\n' > "$tmp_dir/control-nested-if.expected"; \
    cmp "$tmp_dir/control-nested-if.expected" "$tmp_dir/control-nested-if.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/control_flow/while-counter.php > "$tmp_dir/control-while.out"; \
    printf '012\n' > "$tmp_dir/control-while.expected"; \
    cmp "$tmp_dir/control-while.expected" "$tmp_dir/control-while.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/control_flow/do-while-once.php > "$tmp_dir/control-do.out"; \
    printf 'once\n' > "$tmp_dir/control-do.expected"; \
    cmp "$tmp_dir/control-do.expected" "$tmp_dir/control-do.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/control_flow/for-loop.php > "$tmp_dir/control-for.out"; \
    printf '012\n' > "$tmp_dir/control-for.expected"; \
    cmp "$tmp_dir/control-for.expected" "$tmp_dir/control-for.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/control_flow/break.php > "$tmp_dir/control-break.out"; \
    printf '12\n' > "$tmp_dir/control-break.expected"; \
    cmp "$tmp_dir/control-break.expected" "$tmp_dir/control-break.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/control_flow/continue.php > "$tmp_dir/control-continue.out"; \
    printf '134\n' > "$tmp_dir/control-continue.expected"; \
    cmp "$tmp_dir/control-continue.expected" "$tmp_dir/control-continue.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/control_flow/short-circuit.php > "$tmp_dir/control-short-circuit.out"; \
    printf 'ok0|ok0\n' > "$tmp_dir/control-short-circuit.expected"; \
    cmp "$tmp_dir/control-short-circuit.expected" "$tmp_dir/control-short-circuit.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/control_flow/ternary.php > "$tmp_dir/control-ternary.out"; \
    printf 'yes|fallback|kept\n' > "$tmp_dir/control-ternary.expected"; \
    cmp "$tmp_dir/control-ternary.expected" "$tmp_dir/control-ternary.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/control_flow/null-coalesce.php > "$tmp_dir/control-null-coalesce.out"; \
    printf 'fallback|value\n' > "$tmp_dir/control-null-coalesce.expected"; \
    cmp "$tmp_dir/control-null-coalesce.expected" "$tmp_dir/control-null-coalesce.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/control_flow/switch-fallthrough.php > "$tmp_dir/control-switch.out"; \
    printf 'zeroone\n' > "$tmp_dir/control-switch.expected"; \
    cmp "$tmp_dir/control-switch.expected" "$tmp_dir/control-switch.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/control_flow/match-success.php > "$tmp_dir/control-match.out"; \
    printf 'one\n' > "$tmp_dir/control-match.expected"; \
    cmp "$tmp_dir/control-match.expected" "$tmp_dir/control-match.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/control_flow/return.php > "$tmp_dir/control-return.out"; \
    printf 'before\n' > "$tmp_dir/control-return.expected"; \
    cmp "$tmp_dir/control-return.expected" "$tmp_dir/control-return.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/functions/simple.php > "$tmp_dir/functions-simple.out"; \
    printf 'hi\n' > "$tmp_dir/functions-simple.expected"; \
    cmp "$tmp_dir/functions-simple.expected" "$tmp_dir/functions-simple.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/functions/two-args.php > "$tmp_dir/functions-two-args.out"; \
    printf '5\n' > "$tmp_dir/functions-two-args.expected"; \
    cmp "$tmp_dir/functions-two-args.expected" "$tmp_dir/functions-two-args.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/functions/local-scope.php > "$tmp_dir/functions-local-scope.out"; \
    printf '2|10\n' > "$tmp_dir/functions-local-scope.expected"; \
    cmp "$tmp_dir/functions-local-scope.expected" "$tmp_dir/functions-local-scope.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/functions/factorial.php > "$tmp_dir/functions-factorial.out"; \
    printf '120\n' > "$tmp_dir/functions-factorial.expected"; \
    cmp "$tmp_dir/functions-factorial.expected" "$tmp_dir/functions-factorial.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/functions/return-no-value.php > "$tmp_dir/functions-return-no-value.out"; \
    printf 'x\n' > "$tmp_dir/functions-return-no-value.expected"; \
    cmp "$tmp_dir/functions-return-no-value.expected" "$tmp_dir/functions-return-no-value.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/functions/defaults.php > "$tmp_dir/functions-defaults.out"; \
    printf 'hi world!|hi php?\n' > "$tmp_dir/functions-defaults.expected"; \
    cmp "$tmp_dir/functions-defaults.expected" "$tmp_dir/functions-defaults.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/functions/variadic-sum.php > "$tmp_dir/functions-variadic-sum.out"; \
    printf '5\n' > "$tmp_dir/functions-variadic-sum.expected"; \
    cmp "$tmp_dir/functions-variadic-sum.expected" "$tmp_dir/functions-variadic-sum.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/functions/return-types.php > "$tmp_dir/functions-return-types.out"; \
    printf 'ok|4|x\n' > "$tmp_dir/functions-return-types.expected"; \
    cmp "$tmp_dir/functions-return-types.expected" "$tmp_dir/functions-return-types.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/functions/closure-simple.php > "$tmp_dir/functions-closure-simple.out"; \
    printf '3\n' > "$tmp_dir/functions-closure-simple.expected"; \
    cmp "$tmp_dir/functions-closure-simple.expected" "$tmp_dir/functions-closure-simple.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/functions/closure-use.php > "$tmp_dir/functions-closure-use.out"; \
    printf '5\n' > "$tmp_dir/functions-closure-use.expected"; \
    cmp "$tmp_dir/functions-closure-use.expected" "$tmp_dir/functions-closure-use.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/functions/arrow-capture.php > "$tmp_dir/functions-arrow-capture.out"; \
    printf '7\n' > "$tmp_dir/functions-arrow-capture.expected"; \
    cmp "$tmp_dir/functions-arrow-capture.expected" "$tmp_dir/functions-arrow-capture.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/functions/closure-return.php > "$tmp_dir/functions-closure-return.out"; \
    printf '9\n' > "$tmp_dir/functions-closure-return.expected"; \
    cmp "$tmp_dir/functions-closure-return.expected" "$tmp_dir/functions-closure-return.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/php85/pipe-user-function.php > "$tmp_dir/php85-pipe-user-function.out"; \
    printf '3\n' > "$tmp_dir/php85-pipe-user-function.expected"; \
    cmp "$tmp_dir/php85-pipe-user-function.expected" "$tmp_dir/php85-pipe-user-function.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/php85/pipe-closure.php > "$tmp_dir/php85-pipe-closure.out"; \
    printf '4\n' > "$tmp_dir/php85-pipe-closure.expected"; \
    cmp "$tmp_dir/php85-pipe-closure.expected" "$tmp_dir/php85-pipe-closure.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/php85/pipe-builtin.php > "$tmp_dir/php85-pipe-builtin.out"; \
    printf 'a|2|HI\n' > "$tmp_dir/php85-pipe-builtin.expected"; \
    cmp "$tmp_dir/php85-pipe-builtin.expected" "$tmp_dir/php85-pipe-builtin.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/php85/pipe-side-effects.php > "$tmp_dir/php85-pipe-side-effects.out"; \
    printf '7|7\n' > "$tmp_dir/php85-pipe-side-effects.expected"; \
    cmp "$tmp_dir/php85-pipe-side-effects.expected" "$tmp_dir/php85-pipe-side-effects.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/builtins/print.php > "$tmp_dir/builtins-print.out"; \
    printf 'x1\n' > "$tmp_dir/builtins-print.expected"; \
    cmp "$tmp_dir/builtins-print.expected" "$tmp_dir/builtins-print.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/builtins/gettype.php > "$tmp_dir/builtins-gettype.out"; \
    printf 'NULL|integer|boolean|string\n' > "$tmp_dir/builtins-gettype.expected"; \
    cmp "$tmp_dir/builtins-gettype.expected" "$tmp_dir/builtins-gettype.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/builtins/is-types.php > "$tmp_dir/builtins-is-types.out"; \
    printf '1111\n' > "$tmp_dir/builtins-is-types.expected"; \
    cmp "$tmp_dir/builtins-is-types.expected" "$tmp_dir/builtins-is-types.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/builtins/var-dump-scalars.php > "$tmp_dir/builtins-var-dump-scalars.out"; \
    printf 'NULL\nbool(true)\nint(7)\nstring(2) "hi"\n' > "$tmp_dir/builtins-var-dump-scalars.expected"; \
    cmp "$tmp_dir/builtins-var-dump-scalars.expected" "$tmp_dir/builtins-var-dump-scalars.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/builtins/var-dump-array.php > "$tmp_dir/builtins-var-dump-array.out"; \
    printf 'array(2) {\n  [0]=>\n  int(1)\n  [1]=>\n  string(1) "x"\n}\n' > "$tmp_dir/builtins-var-dump-array.expected"; \
    cmp "$tmp_dir/builtins-var-dump-array.expected" "$tmp_dir/builtins-var-dump-array.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/arrays/indexed.php > "$tmp_dir/arrays-indexed.out"; \
    printf '1|2|3\n' > "$tmp_dir/arrays-indexed.expected"; \
    cmp "$tmp_dir/arrays-indexed.expected" "$tmp_dir/arrays-indexed.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/arrays/string-keys.php > "$tmp_dir/arrays-string-keys.out"; \
    printf '1|2\n' > "$tmp_dir/arrays-string-keys.expected"; \
    cmp "$tmp_dir/arrays-string-keys.expected" "$tmp_dir/arrays-string-keys.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/arrays/append-overwrite.php > "$tmp_dir/arrays-append-overwrite.out"; \
    printf '1|5|7\n' > "$tmp_dir/arrays-append-overwrite.expected"; \
    cmp "$tmp_dir/arrays-append-overwrite.expected" "$tmp_dir/arrays-append-overwrite.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/arrays/nested-fetch.php > "$tmp_dir/arrays-nested-fetch.out"; \
    printf '4|8\n' > "$tmp_dir/arrays-nested-fetch.expected"; \
    cmp "$tmp_dir/arrays-nested-fetch.expected" "$tmp_dir/arrays-nested-fetch.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/arrays/missing-key.php > "$tmp_dir/arrays-missing-key.out" 2> "$tmp_dir/arrays-missing-key.err"; \
    printf 'x\n' > "$tmp_dir/arrays-missing-key.expected"; \
    cmp "$tmp_dir/arrays-missing-key.expected" "$tmp_dir/arrays-missing-key.out"; \
    grep -q 'E_PHP_RUNTIME_UNDEFINED_ARRAY_KEY_WARNING' "$tmp_dir/arrays-missing-key.err"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/arrays/isset-empty-unset.php > "$tmp_dir/arrays-isset-empty-unset.out"; \
    printf '1|111|\n' > "$tmp_dir/arrays-isset-empty-unset.expected"; \
    cmp "$tmp_dir/arrays-isset-empty-unset.expected" "$tmp_dir/arrays-isset-empty-unset.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/arrays/var-dump-mixed.php > "$tmp_dir/arrays-var-dump-mixed.out"; \
    printf 'array(3) {\n  [0]=>\n  int(1)\n  ["name"]=>\n  string(3) "php"\n  [4]=>\n  bool(true)\n}\n' > "$tmp_dir/arrays-var-dump-mixed.expected"; \
    cmp "$tmp_dir/arrays-var-dump-mixed.expected" "$tmp_dir/arrays-var-dump-mixed.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/foreach/values.php > "$tmp_dir/foreach-values.out"; \
    printf '123\n' > "$tmp_dir/foreach-values.expected"; \
    cmp "$tmp_dir/foreach-values.expected" "$tmp_dir/foreach-values.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/foreach/key-value.php > "$tmp_dir/foreach-key-value.out"; \
    printf 'a:1;4:2;b:3;\n' > "$tmp_dir/foreach-key-value.expected"; \
    cmp "$tmp_dir/foreach-key-value.expected" "$tmp_dir/foreach-key-value.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/foreach/break-continue.php > "$tmp_dir/foreach-break-continue.out"; \
    printf '13\n' > "$tmp_dir/foreach-break-continue.expected"; \
    cmp "$tmp_dir/foreach-break-continue.expected" "$tmp_dir/foreach-break-continue.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/foreach/nested.php > "$tmp_dir/foreach-nested.out"; \
    printf 'a1;a2;b1;b2;\n' > "$tmp_dir/foreach-nested.expected"; \
    cmp "$tmp_dir/foreach-nested.expected" "$tmp_dir/foreach-nested.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/foreach/snapshot-mutation.php > "$tmp_dir/foreach-snapshot-mutation.out"; \
    printf '12|1299\n' > "$tmp_dir/foreach-snapshot-mutation.expected"; \
    cmp "$tmp_dir/foreach-snapshot-mutation.expected" "$tmp_dir/foreach-snapshot-mutation.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/includes/include-return.php > "$tmp_dir/includes-return.out"; \
    printf 'before|child:value|after\n' > "$tmp_dir/includes-return.expected"; \
    cmp "$tmp_dir/includes-return.expected" "$tmp_dir/includes-return.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/includes/share-variable.php > "$tmp_dir/includes-share-variable.out"; \
    printf 'parent|included\n' > "$tmp_dir/includes-share-variable.expected"; \
    cmp "$tmp_dir/includes-share-variable.expected" "$tmp_dir/includes-share-variable.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/includes/include-once.php > "$tmp_dir/includes-once.out"; \
    printf '1\n' > "$tmp_dir/includes-once.expected"; \
    cmp "$tmp_dir/includes-once.expected" "$tmp_dir/includes-once.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/includes/include-missing.php > "$tmp_dir/includes-missing.out" 2> "$tmp_dir/includes-missing.err"; \
    printf 'before|after\n' > "$tmp_dir/includes-missing.expected"; \
    cmp "$tmp_dir/includes-missing.expected" "$tmp_dir/includes-missing.out"; \
    grep -q 'E_PHP_VM_INCLUDE_MISSING' "$tmp_dir/includes-missing.err"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/objects/instantiate.php > "$tmp_dir/objects-instantiate.out"; \
    printf 'object\n' > "$tmp_dir/objects-instantiate.expected"; \
    cmp "$tmp_dir/objects-instantiate.expected" "$tmp_dir/objects-instantiate.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/objects/constructor-property.php > "$tmp_dir/objects-constructor-property.out"; \
    printf '7\n' > "$tmp_dir/objects-constructor-property.expected"; \
    cmp "$tmp_dir/objects-constructor-property.expected" "$tmp_dir/objects-constructor-property.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/objects/property-read-write.php > "$tmp_dir/objects-property-read-write.out"; \
    printf '3\n' > "$tmp_dir/objects-property-read-write.expected"; \
    cmp "$tmp_dir/objects-property-read-write.expected" "$tmp_dir/objects-property-read-write.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/objects/two-objects.php > "$tmp_dir/objects-two-objects.out"; \
    printf '1|2\n' > "$tmp_dir/objects-two-objects.expected"; \
    cmp "$tmp_dir/objects-two-objects.expected" "$tmp_dir/objects-two-objects.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/objects/method-call.php > "$tmp_dir/objects-method-call.out"; \
    printf '5\n' > "$tmp_dir/objects-method-call.expected"; \
    cmp "$tmp_dir/objects-method-call.expected" "$tmp_dir/objects-method-call.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/objects/method-return.php > "$tmp_dir/objects-method-return.out"; \
    printf '42\n' > "$tmp_dir/objects-method-return.expected"; \
    cmp "$tmp_dir/objects-method-return.expected" "$tmp_dir/objects-method-return.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/objects/this-property-method.php > "$tmp_dir/objects-this-property-method.out"; \
    printf '7|12\n' > "$tmp_dir/objects-this-property-method.expected"; \
    cmp "$tmp_dir/objects-this-property-method.expected" "$tmp_dir/objects-this-property-method.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/objects/static-method.php > "$tmp_dir/objects-static-method.out"; \
    printf 'static-ok\n' > "$tmp_dir/objects-static-method.expected"; \
    cmp "$tmp_dir/objects-static-method.expected" "$tmp_dir/objects-static-method.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/objects/clone-object.php > "$tmp_dir/objects-clone-object.out"; \
    printf '1|1\n' > "$tmp_dir/objects-clone-object.expected"; \
    cmp "$tmp_dir/objects-clone-object.expected" "$tmp_dir/objects-clone-object.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/objects/clone-independent.php > "$tmp_dir/objects-clone-independent.out"; \
    printf '1|2\n' > "$tmp_dir/objects-clone-independent.expected"; \
    cmp "$tmp_dir/objects-clone-independent.expected" "$tmp_dir/objects-clone-independent.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/objects/clone-with.php > "$tmp_dir/objects-clone-with.out"; \
    printf 'old:1|new:2\n' > "$tmp_dir/objects-clone-with.expected"; \
    cmp "$tmp_dir/objects-clone-with.expected" "$tmp_dir/objects-clone-with.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/exceptions/catch-exception.php > "$tmp_dir/exceptions-catch-exception.out"; \
    printf 'caught\n' > "$tmp_dir/exceptions-catch-exception.expected"; \
    cmp "$tmp_dir/exceptions-catch-exception.expected" "$tmp_dir/exceptions-catch-exception.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/exceptions/finally-return.php > "$tmp_dir/exceptions-finally-return.out"; \
    printf 'finally|body\n' > "$tmp_dir/exceptions-finally-return.expected"; \
    cmp "$tmp_dir/exceptions-finally-return.expected" "$tmp_dir/exceptions-finally-return.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/exceptions/catch-finally.php > "$tmp_dir/exceptions-catch-finally.out"; \
    printf 'catch|finally\n' > "$tmp_dir/exceptions-catch-finally.expected"; \
    cmp "$tmp_dir/exceptions-catch-finally.expected" "$tmp_dir/exceptions-catch-finally.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/runtime_types/param-int.php > "$tmp_dir/runtime-types-param-int.out"; \
    printf '5\n' > "$tmp_dir/runtime-types-param-int.expected"; \
    cmp "$tmp_dir/runtime-types-param-int.expected" "$tmp_dir/runtime-types-param-int.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/runtime_types/return-string.php > "$tmp_dir/runtime-types-return-string.out"; \
    printf 'ok\n' > "$tmp_dir/runtime-types-return-string.expected"; \
    cmp "$tmp_dir/runtime-types-return-string.expected" "$tmp_dir/runtime-types-return-string.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/runtime_types/void-return.php > "$tmp_dir/runtime-types-void-return.out"; \
    printf 'before||after\n' > "$tmp_dir/runtime-types-void-return.expected"; \
    cmp "$tmp_dir/runtime-types-void-return.expected" "$tmp_dir/runtime-types-void-return.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/runtime_types/nullable-simple.php > "$tmp_dir/runtime-types-nullable-simple.out"; \
    printf 'none|ok\n' > "$tmp_dir/runtime-types-nullable-simple.expected"; \
    cmp "$tmp_dir/runtime-types-nullable-simple.expected" "$tmp_dir/runtime-types-nullable-simple.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/runtime_types/property-type.php > "$tmp_dir/runtime-types-property-type.out"; \
    printf '7\n' > "$tmp_dir/runtime-types-property-type.expected"; \
    cmp "$tmp_dir/runtime-types-property-type.expected" "$tmp_dir/runtime-types-property-type.out"; \
    set +e; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/invalid/objects/unknown-class.php > "$tmp_dir/objects-unknown-class.out" 2> "$tmp_dir/objects-unknown-class.err"; \
    code=$?; \
    set -e; \
    test "$code" -eq 3; \
    grep -q 'E_PHP_VM_UNKNOWN_CLASS' "$tmp_dir/objects-unknown-class.err"; \
    set +e; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/objects/private-property.php > "$tmp_dir/objects-private-property.out" 2> "$tmp_dir/objects-private-property.err"; \
    code=$?; \
    set -e; \
    test "$code" -eq 0; \
    set +e; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/objects/private-method.php > "$tmp_dir/objects-private-method.out" 2> "$tmp_dir/objects-private-method.err"; \
    code=$?; \
    set -e; \
    test "$code" -eq 0; \
    printf '1\n' > "$tmp_dir/objects-private-method.expected"; \
    cmp "$tmp_dir/objects-private-method.expected" "$tmp_dir/objects-private-method.out"; \
    set +e; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/invalid/objects/this-outside-method.php > "$tmp_dir/objects-this-outside-method.out" 2> "$tmp_dir/objects-this-outside-method.err"; \
    code=$?; \
    set -e; \
    test "$code" -eq 3; \
    grep -q 'E_PHP_VM_THIS_OUTSIDE_METHOD' "$tmp_dir/objects-this-outside-method.err"; \
    set +e; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/objects/static-property.php > "$tmp_dir/objects-static-property.out" 2> "$tmp_dir/objects-static-property.err"; \
    code=$?; \
    set -e; \
    test "$code" -eq 0; \
    printf '1\n' > "$tmp_dir/objects-static-property.expected"; \
    cmp "$tmp_dir/objects-static-property.expected" "$tmp_dir/objects-static-property.out"; \
    set +e; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/known_gaps/objects/clone-with-private.php > "$tmp_dir/objects-clone-with-private.out" 2> "$tmp_dir/objects-clone-with-private.err"; \
    code=$?; \
    set -e; \
    test "$code" -eq 3; \
    grep -q 'E_PHP_VM_UNSUPPORTED_PROPERTY_MODIFIER' "$tmp_dir/objects-clone-with-private.err"; \
    set +e; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/known_gaps/objects/clone-with-readonly.php > "$tmp_dir/objects-clone-with-readonly.out" 2> "$tmp_dir/objects-clone-with-readonly.err"; \
    code=$?; \
    set -e; \
    test "$code" -eq 3; \
    grep -q 'E_PHP_VM_UNSUPPORTED_PROPERTY_MODIFIER' "$tmp_dir/objects-clone-with-readonly.err"; \
    set +e; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/invalid/exceptions/throw-uncaught.php > "$tmp_dir/exceptions-throw-uncaught.out" 2> "$tmp_dir/exceptions-throw-uncaught.err"; \
    code=$?; \
    set -e; \
    test "$code" -eq 3; \
    grep -q 'E_PHP_VM_UNCAUGHT_EXCEPTION' "$tmp_dir/exceptions-throw-uncaught.err"; \
    set +e; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/invalid/exceptions/finally-throw.php > "$tmp_dir/exceptions-finally-throw.out" 2> "$tmp_dir/exceptions-finally-throw.err"; \
    code=$?; \
    set -e; \
    test "$code" -eq 3; \
    grep -qx 'finally' "$tmp_dir/exceptions-finally-throw.out"; \
    grep -q 'Uncaught Exception: boom' "$tmp_dir/exceptions-finally-throw.out"; \
    grep -q 'E_PHP_VM_UNCAUGHT_EXCEPTION' "$tmp_dir/exceptions-finally-throw.err"; \
    set +e; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/invalid/exceptions/rethrow.php > "$tmp_dir/exceptions-rethrow.out" 2> "$tmp_dir/exceptions-rethrow.err"; \
    code=$?; \
    set -e; \
    test "$code" -eq 3; \
    grep -qx 'catch' "$tmp_dir/exceptions-rethrow.out"; \
    grep -q 'Uncaught Exception: boom' "$tmp_dir/exceptions-rethrow.out"; \
    grep -q 'E_PHP_VM_UNCAUGHT_EXCEPTION' "$tmp_dir/exceptions-rethrow.err"; \
    set +e; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/invalid/runtime_types/param-int-fail.php > "$tmp_dir/runtime-types-param-int-fail.out" 2> "$tmp_dir/runtime-types-param-int-fail.err"; \
    code=$?; \
    set -e; \
    test "$code" -eq 3; \
    grep -q 'E_PHP_VM_PARAM_TYPE_MISMATCH' "$tmp_dir/runtime-types-param-int-fail.err"; \
    set +e; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/invalid/runtime_types/return-string-fail.php > "$tmp_dir/runtime-types-return-string-fail.out" 2> "$tmp_dir/runtime-types-return-string-fail.err"; \
    code=$?; \
    set -e; \
    test "$code" -eq 3; \
    grep -q 'E_PHP_VM_RETURN_TYPE_MISMATCH' "$tmp_dir/runtime-types-return-string-fail.err"; \
    set +e; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/invalid/runtime_types/void-return-value.php > "$tmp_dir/runtime-types-void-return-value.out" 2> "$tmp_dir/runtime-types-void-return-value.err"; \
    code=$?; \
    set -e; \
    test "$code" -eq 2; \
    grep -q 'E_PHP_RETURN_VALUE_FROM_VOID_FUNCTION' "$tmp_dir/runtime-types-void-return-value.err"; \
    set +e; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/invalid/runtime_types/property-type-fail.php > "$tmp_dir/runtime-types-property-type-fail.out" 2> "$tmp_dir/runtime-types-property-type-fail.err"; \
    code=$?; \
    set -e; \
    test "$code" -eq 3; \
    grep -q 'E_PHP_VM_PROPERTY_TYPE_MISMATCH' "$tmp_dir/runtime-types-property-type-fail.err"; \
    set +e; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/invalid/exceptions/nonmatching-catch-type.php > "$tmp_dir/exceptions-catch-type.out" 2> "$tmp_dir/exceptions-catch-type.err"; \
    code=$?; \
    set -e; \
    test "$code" -eq 3; \
    grep -q 'E_PHP_VM_UNCAUGHT_EXCEPTION' "$tmp_dir/exceptions-catch-type.err"; \
    set +e; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/invalid/includes/require-missing.php > "$tmp_dir/includes-require-missing.out" 2> "$tmp_dir/includes-require-missing.err"; \
    code=$?; \
    set -e; \
    test "$code" -eq 3; \
    printf 'before|' > "$tmp_dir/includes-require-missing.expected"; \
    cmp "$tmp_dir/includes-require-missing.expected" "$tmp_dir/includes-require-missing.out"; \
    grep -q 'E_PHP_VM_INCLUDE_MISSING' "$tmp_dir/includes-require-missing.err"; \
    set +e; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/known_gaps/foreach/by-ref.php > "$tmp_dir/foreach-by-ref.out" 2> "$tmp_dir/foreach-by-ref.err"; \
    code=$?; \
    set -e; \
    test "$code" -eq 2; \
    grep -q 'E_PHP_IR_UNSUPPORTED_BY_REF_FOREACH' "$tmp_dir/foreach-by-ref.err"; \
    set +e; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/references/by-ref-param.php > "$tmp_dir/references-by-ref-param.out" 2> "$tmp_dir/references-by-ref-param.err"; \
    code=$?; \
    set -e; \
    test "$code" -eq 0; \
    printf '2' > "$tmp_dir/references-by-ref-param.expected"; \
    cmp "$tmp_dir/references-by-ref-param.expected" "$tmp_dir/references-by-ref-param.out"; \
    set +e; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/references/by-ref-return.php > "$tmp_dir/references-by-ref-return.out" 2> "$tmp_dir/references-by-ref-return.err"; \
    code=$?; \
    set -e; \
    test "$code" -eq 0; \
    printf '1' > "$tmp_dir/references-by-ref-return.expected"; \
    cmp "$tmp_dir/references-by-ref-return.expected" "$tmp_dir/references-by-ref-return.out"; \
    set +e; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/references/array-element-ref.php > "$tmp_dir/references-array-element-ref.out" 2> "$tmp_dir/references-array-element-ref.err"; \
    code=$?; \
    set -e; \
    test "$code" -eq 0; \
    printf '2' > "$tmp_dir/references-array-element-ref.expected"; \
    cmp "$tmp_dir/references-array-element-ref.expected" "$tmp_dir/references-array-element-ref.out"; \
    set +e; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/invalid/division-by-zero.php > "$tmp_dir/division.out" 2> "$tmp_dir/division.err"; \
    code=$?; \
    set -e; \
    test "$code" -eq 3; \
    grep -q 'runtime_error: division by zero' "$tmp_dir/division.err"; \
    grep -q 'E_PHP_RUNTIME_DIVISION_BY_ZERO' "$tmp_dir/division.err"; \
    set +e; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/invalid/errors/undefined-function.php > "$tmp_dir/errors-undefined-function.out" 2> "$tmp_dir/errors-undefined-function.err"; \
    code=$?; \
    set -e; \
    test "$code" -eq 3; \
    grep -q 'runtime_error: undefined function runtime_missing_function' "$tmp_dir/errors-undefined-function.err"; \
    grep -q 'E_PHP_RUNTIME_UNDEFINED_FUNCTION' "$tmp_dir/errors-undefined-function.err"; \
    set +e; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/invalid/type-error.php > "$tmp_dir/type.out" 2> "$tmp_dir/type.err"; \
    code=$?; \
    set -e; \
    test "$code" -eq 3; \
    grep -q 'E_PHP_RUNTIME_NON_NUMERIC_STRING' "$tmp_dir/type.err"; \
    set +e; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/invalid/match-no-arm.php > "$tmp_dir/match-no-arm.out" 2> "$tmp_dir/match-no-arm.err"; \
    code=$?; \
    set -e; \
    test "$code" -eq 3; \
    grep -q 'E_PHP_VM_UNHANDLED_MATCH: match expression did not match any arm' "$tmp_dir/match-no-arm.err"; \
    set +e; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/invalid/functions/call-stack-error.php > "$tmp_dir/functions-call-stack.out" 2> "$tmp_dir/functions-call-stack.err"; \
    code=$?; \
    set -e; \
    test "$code" -eq 3; \
    grep -q 'runtime_error: division by zero' "$tmp_dir/functions-call-stack.err"; \
    grep -q 'call_stack:' "$tmp_dir/functions-call-stack.err"; \
    grep -q 'at boom' "$tmp_dir/functions-call-stack.err"; \
    grep -q 'at wrap' "$tmp_dir/functions-call-stack.err"; \
    set +e; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/invalid/functions/missing-arg.php > "$tmp_dir/functions-missing-arg.out" 2> "$tmp_dir/functions-missing-arg.err"; \
    code=$?; \
    set -e; \
    test "$code" -eq 3; \
    grep -q 'ArgumentCountError: function one expects at least 1 argument(s), got 0' "$tmp_dir/functions-missing-arg.err"; \
    set +e; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/invalid/functions/extra-arg.php > "$tmp_dir/functions-extra-arg.out" 2> "$tmp_dir/functions-extra-arg.err"; \
    code=$?; \
    set -e; \
    test "$code" -eq 3; \
    grep -q 'ArgumentCountError: function one expects at most 1 argument(s), got 2' "$tmp_dir/functions-extra-arg.err"; \
    set +e; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/invalid/functions/return-type-error.php > "$tmp_dir/functions-return-type-error.out" 2> "$tmp_dir/functions-return-type-error.err"; \
    code=$?; \
    set -e; \
    test "$code" -eq 3; \
    grep -q 'E_PHP_VM_RETURN_TYPE_MISMATCH' "$tmp_dir/functions-return-type-error.err"; \
    set +e; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/functions/by-ref-capture.php > "$tmp_dir/functions-by-ref-capture.out" 2> "$tmp_dir/functions-by-ref-capture.err"; \
    code=$?; \
    set -e; \
    test "$code" -eq 0; \
    printf '3' > "$tmp_dir/functions-by-ref-capture.expected"; \
    cmp "$tmp_dir/functions-by-ref-capture.expected" "$tmp_dir/functions-by-ref-capture.out"; \
    set +e; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/invalid/php85/pipe-not-callable.php > "$tmp_dir/php85-pipe-not-callable.out" 2> "$tmp_dir/php85-pipe-not-callable.err"; \
    code=$?; \
    set -e; \
    test "$code" -eq 3; \
    grep -q 'E_PHP_VM_PIPE_RHS_NOT_CALLABLE' "$tmp_dir/php85-pipe-not-callable.err"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/generators/yield.php > "$tmp_dir/generator-gap.out" 2> "$tmp_dir/generator-gap.err"; \
    printf '1' > "$tmp_dir/generator-gap.expected"; \
    cmp "$tmp_dir/generator-gap.expected" "$tmp_dir/generator-gap.out"; \
    printf '%s\n' '[ok] runtime fixtures passed.'

runtime-corpus-smoke:
    scripts/runtime-corpus-smoke.sh

runtime-reference-smoke:
    cargo test -p php_testkit runtime_reference_smoke -- --nocapture

runtime-diff:
    cargo build -p php_vm_cli -p php_testkit --bin compare-runtime
    ${CARGO_TARGET_DIR:-target}/debug/compare-runtime --fixtures fixtures/runtime --out target/runtime/runtime-diff --rust-vm ${CARGO_TARGET_DIR:-target}/debug/php-vm

phpt-smoke:
    cargo build -p php_vm_cli -p php_testkit --bin run-phpt-smoke
    ${CARGO_TARGET_DIR:-target}/debug/run-phpt-smoke --fixtures fixtures/phpt_smoke --out target/runtime/phpt-smoke --rust-vm ${CARGO_TARGET_DIR:-target}/debug/php-vm

runtime-known-gaps:
    cargo build -p php_vm_cli
    test -s docs/runtime-known-gaps.md
    grep -q 'E_PHP_RUNTIME_UNSUPPORTED_REFERENCE_SEMANTICS' docs/runtime-known-gaps.md
    grep -q 'E_PHP_IR_UNSUPPORTED_GENERATOR' docs/runtime-known-gaps.md
    grep -q 'E_PHP_IR_UNSUPPORTED_YIELD_FROM' docs/runtime-known-gaps.md
    grep -q 'E_PHP_IR_UNSUPPORTED_FIBER' docs/runtime-known-gaps.md
    grep -q 'E_PHP_IR_UNSUPPORTED_EVAL' docs/runtime-known-gaps.md
    grep -q 'E_PHP_IR_UNSUPPORTED_AUTOLOAD' docs/runtime-known-gaps.md
    grep -q 'E_PHP_IR_UNSUPPORTED_REFLECTION' docs/runtime-known-gaps.md
    grep -q 'E_PHP_IR_UNSUPPORTED_TRAIT_RUNTIME' docs/runtime-known-gaps.md
    grep -q 'E_PHP_IR_UNSUPPORTED_ENUM_RUNTIME' docs/runtime-known-gaps.md
    grep -q 'E_PHP_IR_UNSUPPORTED_PROPERTY_HOOKS' docs/runtime-known-gaps.md
    grep -q 'E_PHP_IR_UNSUPPORTED_BY_REF_FOREACH' docs/runtime-known-gaps.md
    grep -q 'E_PHP_RUNTIME_SUPERGLOBALS_FULL_MATRIX' docs/runtime-known-gaps.md
    grep -q 'E_PHP_RUNTIME_GLOBALS_ALIAS_MATRIX' docs/runtime-known-gaps.md
    test -f fixtures/runtime/valid/generators/yield.php
    test -f fixtures/runtime/valid/generators/yield-from.php
    test -f fixtures/runtime/valid/fibers/fiber.php
    test -f fixtures/runtime/valid/eval/eval.php
    test -f fixtures/runtime/known_gaps/autoload/spl-autoload-register.php
    test -f fixtures/runtime/known_gaps/reflection/reflection-class.php
    test -f fixtures/runtime/valid/traits/trait-use.php
    test -f fixtures/runtime/valid/enums/unit-enum.php
    test -f fixtures/runtime/valid/property_hooks/get-hook.php
    test -f fixtures/runtime/valid/references/by-ref-return.php
    test -f fixtures/runtime/valid/references/array-element-ref.php
    test -f fixtures/runtime/known_gaps/foreach/by-ref.php
    test -f fixtures/runtime/valid/superglobals/globals-alias.php
    test -f fixtures/runtime/known_gaps/objects/clone-with-private.php
    test -f fixtures/runtime/known_gaps/objects/clone-with-readonly.php
    test -f fixtures/runtime/invalid/exceptions/nonmatching-catch-type.php

    @tmp_dir="target/runtime-known-gaps"; \
    mkdir -p "$tmp_dir"; \
    for fixture_id in \
      "fixtures/runtime/known_gaps/foreach/by-ref.php:E_PHP_IR_UNSUPPORTED_BY_REF_FOREACH:foreach-by-ref"; do \
      IFS=':' read -r fixture diagnostic name <<< "$fixture_id"; \
      set +e; \
      ${CARGO_TARGET_DIR:-target}/debug/php-vm run "$fixture" > "$tmp_dir/$name.out" 2> "$tmp_dir/$name.err"; \
      code=$?; \
      set -e; \
      test "$code" -eq 2; \
      grep -q "$diagnostic" "$tmp_dir/$name.err"; \
    done; \
    for fixture_id in \
      "fixtures/runtime/known_gaps/autoload/spl-autoload-register.php:E_PHP_VM_UNKNOWN_CLASS:autoload" \
      "fixtures/runtime/known_gaps/reflection/reflection-class.php:E_PHP_VM_REFLECTION_UNKNOWN_CLASS:reflection" \
      "fixtures/runtime/known_gaps/objects/clone-with-private.php:E_PHP_VM_UNSUPPORTED_PROPERTY_MODIFIER:clone-with-private" \
      "fixtures/runtime/known_gaps/objects/clone-with-readonly.php:E_PHP_VM_UNSUPPORTED_PROPERTY_MODIFIER:clone-with-readonly"; do \
      IFS=':' read -r fixture diagnostic name <<< "$fixture_id"; \
      set +e; \
      ${CARGO_TARGET_DIR:-target}/debug/php-vm run "$fixture" > "$tmp_dir/$name.out" 2> "$tmp_dir/$name.err"; \
      code=$?; \
      set -e; \
      test "$code" -eq 3; \
      grep -q "$diagnostic" "$tmp_dir/$name.err"; \
    done; \
    printf '%s\n' '[ok] runtime known-gap catalog and reference fixtures passed.'

bench-vm-smoke:
    cargo build -p php_vm_cli
    mkdir -p target/runtime/bench-vm-smoke
    rustc --edition=2024 tools/bench_vm_smoke.rs -o target/runtime/bench-vm-smoke/bench-vm-smoke
    target/runtime/bench-vm-smoke/bench-vm-smoke

fuzz-vm-smoke:
    cargo build -p php_vm_cli
    mkdir -p target/runtime/fuzz-vm-smoke
    rustc --edition=2024 tools/fuzz_vm_smoke.rs -o target/runtime/fuzz-vm-smoke/fuzz-vm-smoke
    target/runtime/fuzz-vm-smoke/fuzz-vm-smoke

runtime-semantics-fixtures:
    @just refs-cow-fixtures
    @just object-semantics-fixtures
    @just generator-fiber-fixtures
    @just real-world-fixtures
    @just regression-fixtures
    @printf '%s\n' '[ok] runtime semantics fixture gates complete.'

runtime-semantics-diff *args:
    cargo build -p php_vm_cli
    scripts/runtime_semantics_diff.py {{args}}

runtime-toolchain-audit:
    @for tool in cargo rustc rustfmt cargo-clippy just jq python3 rg clang sccache; do \
      if ! command -v "$tool" >/dev/null 2>&1; then \
        printf '%s\n' "[missing] required runtime semantics devshell tool: $tool" >&2; \
        exit 1; \
      fi; \
    done; \
    @if ! command -v shellcheck >/dev/null 2>&1; then \
      case "$$(uname -s)" in \
        Darwin) printf '%s\n' '[skip] shellcheck unavailable; Darwin devshell omits it to avoid the Haskell closure';; \
        *) printf '%s\n' '[missing] required runtime semantics devshell tool: shellcheck' >&2; exit 1;; \
      esac; \
    fi; \
    test "${PHP_REF_SERIES:-}" = "8.5"; \
    test "${PHP_REF_VERSION:-}" = "8.5.7"; \
    test "${PHP_REF_TAG:-}" = "php-8.5.7"; \
    test -n "${CARGO_TARGET_DIR:-}"; \
    test -n "${SCCACHE_DIR:-}"; \
    printf '%s\n' '[ok] runtime semantics devshell toolchain audit passed'

runtime-miri-smoke:
    @if ! command -v cargo-miri >/dev/null 2>&1 && ! cargo miri --version >/dev/null 2>&1; then \
      printf '%s\n' '[skip] cargo-miri is not available in this toolchain; install a Miri-capable Rust toolchain to run this opt-in smoke.'; \
      exit 0; \
    fi; \
    if ! cargo miri --version >/dev/null 2>&1; then \
      printf '%s\n' '[skip] cargo-miri is present but not usable for the active toolchain; this opt-in smoke is not part of verify-runtime.'; \
      exit 0; \
    fi; \
    cargo miri test -p php_runtime reference::tests::slot_alias_and_copy_semantics_are_distinct

runtime-sanitizer-smoke:
    @if [[ "${PHRUST_RUN_SANITIZER:-0}" != "1" ]]; then \
      printf '%s\n' '[skip] set PHRUST_RUN_SANITIZER=1 to run the opt-in sanitizer smoke.'; \
      exit 0; \
    fi; \
    if [[ "$$(uname -s)" != "Linux" ]]; then \
      printf '%s\n' '[skip] sanitizer smoke is currently supported only on Linux devshells.'; \
      exit 0; \
    fi; \
    if ! command -v clang >/dev/null 2>&1; then \
      printf '%s\n' '[skip] clang is required for sanitizer smoke.'; \
      exit 0; \
    fi; \
    if ! rustc --version | grep -qi nightly; then \
      printf '%s\n' '[skip] sanitizer smoke requires a nightly Rust toolchain with -Zsanitizer support.'; \
      exit 0; \
    fi; \
    RUSTFLAGS="-Zsanitizer=address" cargo test -p php_runtime gc::tests::gc_scans_roots_and_refcount_metadata_without_panics

runtime-fuzz-smoke:
    cargo build -p php_vm_cli
    scripts/runtime_semantics_fuzz_smoke.py

runtime-bench-smoke:
    cargo build -p php_vm_cli
    scripts/runtime_semantics_bench_smoke.py

runtime-composer-smoke:
    @if [ -z "${PHPRUST_COMPOSER_FIXTURE_DIR:-}" ]; then \
        scripts/runtime_composer_smoke.py; \
    else \
        cargo build -p php_vm_cli; \
        scripts/runtime_composer_smoke.py; \
    fi

refs-cow-fixtures:
    cargo build -p php_vm_cli
    scripts/runtime_semantics_diff.py --category refs --category cow --out target/runtime-semantics/refs-cow

object-semantics-fixtures:
    cargo build -p php_vm_cli
    scripts/runtime_semantics_diff.py --category objects --category traits --category enums --category magic --category properties --category property_hooks --category clone_with --out target/runtime-semantics/object-semantics

generator-fiber-fixtures:
    cargo build -p php_vm_cli
    scripts/runtime_semantics_diff.py --category generators --category fibers --out target/runtime-semantics/generator-fiber

real-world-fixtures:
    cargo build -p php_vm_cli
    scripts/runtime_semantics_diff.py --category real_world --out target/runtime-semantics/real-world

regression-fixtures:
    cargo build -p php_vm_cli
    scripts/runtime_semantics_diff.py --category regressions --out target/runtime-semantics/regressions

local-composer-smoke *paths:
    @if [ -z "{{paths}}" ]; then \
        printf '%s\n' '[skip] provide one or more local Composer project paths: just local-composer-smoke path/to/project'; \
        exit 0; \
    fi; \
    cargo build -p php_vm_cli; \
    args=''; \
    for path in {{paths}}; do args="$$args --dir $$path"; done; \
    scripts/runtime_semantics_diff.py $$args --out target/runtime-semantics/local-composer-smoke

runtime-phpt-smoke:
    cargo build -p php_vm_cli -p php_testkit --bin run-phpt-smoke
    ${CARGO_TARGET_DIR:-target}/debug/run-phpt-smoke --fixtures /private/tmp/phrust-empty-phpt-smoke --out target/runtime-semantics/phpt-smoke --rust-vm ${CARGO_TARGET_DIR:-target}/debug/php-vm --allowlist fixtures/runtime_semantics/phpt_allowlist.toml

extension-phpt-smoke:
    cargo build -p php_vm_cli -p php_testkit --bin run-phpt-smoke
    scripts/stdlib/phpt_extension_selector.py

compat-corpus-smoke:
    cargo build -p php_vm_cli
    scripts/stdlib_diff.py --fixtures tests/fixtures/stdlib/corpus --area corpus --out target/stdlib/corpus

performance-tests:
    cargo test --workspace
    scripts/performance/compare_perf_json.py --self-test
    scripts/performance/hotpath_inventory.py --self-test
    scripts/performance/perf_report.py --self-test

performance-regression:
    scripts/performance_regression_smoke.sh
    cargo build -p php_vm_cli --bin php-vm
    scripts/performance/regression_smoke.sh
    @just perf-flag-matrix
    @just polymorphic-inline-cache-smoke

perf-flag-matrix:
    scripts/performance/perf_flag_matrix.py

ir-verify:
    cargo test -p php_ir verify --lib

benchmark-smoke:
    cargo build -p php_vm_cli --bin php-vm
    scripts/performance/bench_matrix.py --engine "${CARGO_TARGET_DIR:-target}/debug/php-vm" --out target/performance/benchmark-smoke.json --repetitions "${PHRUST_PERF_BENCH_SMOKE_REPETITIONS:-1}" --warmups "${PHRUST_PERF_BENCH_SMOKE_WARMUPS:-0}" --timeout "${PHRUST_PERF_BENCH_TIMEOUT:-10.0}"

callgrind-smoke:
    scripts/performance/callgrind_smoke.sh

rust-hotpath-bench:
    CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-target}" cargo bench --manifest-path crates/php_bench/Cargo.toml --bench perf_hotpaths

benchmark-suite:
    cargo build -p php_vm_cli --bin php-vm
    scripts/performance/bench_matrix.py --engine "${CARGO_TARGET_DIR:-target}/debug/php-vm" --out target/performance/benchmark-suite.json --repetitions "${PHRUST_PERF_BENCH_REPETITIONS:-5}" --warmups "${PHRUST_PERF_BENCH_WARMUPS:-1}" --timeout "${PHRUST_PERF_BENCH_TIMEOUT:-5.0}"
    @just rust-hotpath-bench

profile-dispatch:
    scripts/performance/profile_smoke.sh dispatch

profile-arrays:
    scripts/performance/profile_smoke.sh arrays

profile-calls:
    scripts/performance/profile_smoke.sh calls

profile-composer:
    scripts/performance/profile_smoke.sh composer

release-profile-plan:
    scripts/performance/release_profile_plan.sh

framework-smoke:
    cargo build -p php_vm_cli --bin php-vm
    scripts/performance/framework_micro_smoke.py

hotpath-inventory:
    scripts/performance/hotpath_inventory.py target/performance/benchmark-smoke.json --json-out target/performance/hotpaths.json --markdown-out docs/hotpath-inventory.md

perf-baseline:
    cargo build -p php_vm_cli --bin php-vm
    scripts/performance/bench_matrix.py --engine "${CARGO_TARGET_DIR:-target}/debug/php-vm" --out target/performance/baseline.json --repetitions "${PHRUST_PERF_BASELINE_REPETITIONS:-3}" --warmups "${PHRUST_PERF_BASELINE_WARMUPS:-1}" --timeout "${PHRUST_PERF_BENCH_TIMEOUT:-5.0}"

perf-compare:
    @if [ ! -f target/performance/baseline.json ]; then \
        printf '%s\n' '[skip] target/performance/baseline.json missing; run `just perf-baseline` first'; \
        exit 0; \
    fi
    @just benchmark-smoke
    scripts/performance/compare_perf_json.py target/performance/baseline.json target/performance/benchmark-smoke.json --out target/performance/perf-compare.md --json-out target/performance/perf-compare.json

cache-roundtrip:
    @just cache-fingerprint-smoke
    cargo test -p php_bytecode_cache bytecode_cache
    cargo test -p php_vm_cli bytecode_cache

cache-fingerprint-smoke:
    scripts/performance/cache_fingerprint_smoke.sh

optimizer-diff:
    @just ir-verify
    scripts/performance/optimizer_diff_smoke.sh

quickening-smoke:
    cargo build -p php_vm_cli --bin php-vm
    scripts/performance/quickening_smoke.sh

inline-cache-smoke:
    cargo build -p php_vm_cli --bin php-vm
    scripts/performance/inline_cache_smoke.sh

polymorphic-inline-cache-smoke:
    cargo build -p php_vm_cli --bin php-vm
    scripts/performance/polymorphic_inline_cache_smoke.sh

jit-smoke:
    scripts/performance/jit_smoke.sh

jit-cranelift-smoke:
    @set +e; scripts/performance/cranelift/platform_check.py --out target/performance/cranelift/platform.json; status=$?; set -e; if [ "$status" -eq 77 ]; then exit 0; elif [ "$status" -ne 0 ]; then exit "$status"; fi
    cargo check --workspace
    @if cargo tree -p php_jit --no-default-features -e features | rg 'cranelift-' >/dev/null; then \
        printf '%s\n' '[fail] default php_jit dependency tree unexpectedly includes Cranelift crates' >&2; \
        cargo tree -p php_jit --no-default-features -e features >&2; \
        exit 1; \
    fi
    cargo check --workspace --features jit-cranelift
    cargo test -p php_jit --features jit-cranelift
    cargo test -p php_vm --features jit-cranelift jit_
    cargo test -p php_vm --features jit-cranelift cranelift_
    cargo build -p php_vm_cli --bin php-vm --features jit-cranelift
    @printf '%s\n' '[pass] Cranelift feature-gating smoke passed'

jit-cranelift-diff:
    @set +e; scripts/performance/cranelift/platform_check.py --out target/performance/cranelift/platform.json; status=$?; set -e; if [ "$status" -eq 77 ]; then exit 0; elif [ "$status" -ne 0 ]; then exit "$status"; fi
    cargo build -p php_vm_cli --bin php-vm --features jit-cranelift
    scripts/performance/cranelift/jit_diff.py --engine "${CARGO_TARGET_DIR:-target}/debug/php-vm"

jit-cranelift-bench-smoke:
    @set +e; scripts/performance/cranelift/platform_check.py --out target/performance/cranelift/platform.json; status=$?; set -e; if [ "$status" -eq 77 ]; then exit 0; elif [ "$status" -ne 0 ]; then exit "$status"; fi
    @just jit-cranelift-diff
    cargo build -p php_vm_cli --bin php-vm --features jit-cranelift
    scripts/performance/cranelift/jit_bench_matrix.py --engine "${CARGO_TARGET_DIR:-target}/debug/php-vm" --out target/performance/cranelift/bench-smoke.json --smoke

jit-cranelift-report:
    @set +e; scripts/performance/cranelift/platform_check.py --out target/performance/cranelift/platform.json; status=$?; set -e; if [ "$status" -eq 77 ]; then exit 0; elif [ "$status" -ne 0 ]; then exit "$status"; fi
    @just jit-cranelift-diff
    cargo build -p php_vm_cli --bin php-vm --features jit-cranelift
    scripts/performance/cranelift/jit_bench_matrix.py --engine "${CARGO_TARGET_DIR:-target}/debug/php-vm" --out target/performance/cranelift/big_wins_report.json

cranelift-guard-report:
    @set +e; scripts/performance/cranelift/platform_check.py --out target/performance/cranelift/platform.json; status=$?; set -e; if [ "$status" -eq 77 ]; then exit 0; elif [ "$status" -ne 0 ]; then exit "$status"; fi
    @just jit-cranelift-report
    scripts/performance/cranelift/guard_failure_report.py --input target/performance/cranelift/big_wins_report.json --out target/performance/cranelift/guard-report.json --text-out target/performance/cranelift/guard-report.txt

jit-cranelift-disasm:
    @set +e; scripts/performance/cranelift/platform_check.py --out target/performance/cranelift/platform.json; status=$?; set -e; if [ "$status" -eq 77 ]; then exit 0; elif [ "$status" -ne 0 ]; then exit "$status"; fi
    cargo build -p php_vm_cli --bin php-vm --features jit-cranelift
    scripts/performance/cranelift/disasm_dump.py --engine "${CARGO_TARGET_DIR:-target}/debug/php-vm"

jit-cranelift-fuzz-smoke:
    @set +e; scripts/performance/cranelift/platform_check.py --out target/performance/cranelift/platform.json; status=$?; set -e; if [ "$status" -eq 77 ]; then exit 0; elif [ "$status" -ne 0 ]; then exit "$status"; fi
    cargo build -p php_vm_cli --bin php-vm --features jit-cranelift
    scripts/performance/cranelift/jit_eligible_ir_fuzz_smoke.py --engine "${CARGO_TARGET_DIR:-target}/debug/php-vm"

jit-cranelift-poly-ic-experiment:
    @set +e; scripts/performance/cranelift/platform_check.py --out target/performance/cranelift/platform.json; status=$?; set -e; if [ "$status" -eq 77 ]; then exit 0; elif [ "$status" -ne 0 ]; then exit "$status"; fi
    cargo build -p php_vm_cli --bin php-vm --features jit-cranelift
    scripts/performance/cranelift/polymorphic_ic_experiment.py --engine "${CARGO_TARGET_DIR:-target}/debug/php-vm"
    @just jit-cranelift-report
    scripts/performance/cranelift/guard_failure_report.py --input target/performance/cranelift/big_wins_report.json --out target/performance/cranelift/polymorphic-ic/guard-report.json --text-out target/performance/cranelift/polymorphic-ic/guard-report.txt --experimental-ic-report target/performance/cranelift/polymorphic-ic/report.json

jit-cranelift-framework-smoke:
    @set +e; scripts/performance/cranelift/platform_check.py --out target/performance/cranelift/platform.json; status=$?; set -e; if [ "$status" -eq 77 ]; then exit 0; elif [ "$status" -ne 0 ]; then exit "$status"; fi
    cargo build -p php_vm_cli --bin php-vm --features jit-cranelift
    scripts/performance/cranelift/framework_smoke.py --engine "${CARGO_TARGET_DIR:-target}/debug/php-vm"

verify-cranelift:
    @set +e; scripts/performance/cranelift/platform_check.py --out target/performance/cranelift/platform.json; status=$?; set -e; if [ "$status" -eq 77 ]; then exit 0; elif [ "$status" -ne 0 ]; then exit "$status"; fi
    @just jit-cranelift-smoke
    @just jit-cranelift-diff
    @just jit-cranelift-bench-smoke
    @just jit-cranelift-report
    @just cranelift-guard-report
    @just jit-cranelift-fuzz-smoke
    @printf '%s\n' '[pass] Cranelift verification complete'

dump-cranelift-clif:
    cargo run -p php_vm_cli --bin php-vm --features jit-cranelift -- dump-cranelift-clif

safety-audit-smoke:
    @if rg -n '\bunsafe\b' crates/php_bytecode_cache crates/php_vm/src/inline_cache.rs crates/php_vm/src/quickening.rs crates/php_vm/src/tiering.rs; then \
        printf '%s\n' '[fail] performance cache/JIT/adaptive surface contains Rust unsafe' >&2; \
        exit 1; \
    fi
    @if rg -n '\bunsafe\b' crates/php_jit/src --glob '!lib.rs' --glob '!helpers.rs' --glob '!cranelift_lowering.rs'; then \
        printf '%s\n' '[fail] performance default JIT surface contains unaudited Rust unsafe' >&2; \
        exit 1; \
    fi
    @test -f docs/safety-audit-cranelift.md
    cargo test -p php_bytecode_cache corrupt
    cargo test -p php_vm_cli bytecode_cache
    @if ! command -v cargo-miri >/dev/null 2>&1 && ! cargo miri --version >/dev/null 2>&1; then \
        printf '%s\n' '[skip] cargo-miri is not available in this toolchain; safety audit smoke skipped.'; \
        exit 0; \
    fi; \
    if ! cargo miri --version >/dev/null 2>&1; then \
        printf '%s\n' '[skip] cargo-miri is present but not usable for the active toolchain; safety audit smoke skipped.'; \
        exit 0; \
    fi; \
    cargo miri test -p php_bytecode_cache rejects_corrupt_input

perf-report:
    scripts/performance/perf_report.py
