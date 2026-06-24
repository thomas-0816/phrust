set shell := ["bash", "-euo", "pipefail", "-c"]

help:
    @printf '%s\n' \
      'Available commands:' \
      '  just help           Show this help' \
      '  just verify         Run the central verification gate' \
      '  just verify-foundation  Run foundation verification' \
      '  just verify-lexer   Run lexer verification' \
      '  just fmt            Check Rust formatting' \
      '  just lint           Run Rust linting' \
      '  just test           Run Rust tests' \
      '  just test-lexer     Run lexer crate tests' \
      '  just check          Run local non-recursive checks' \
      '  just verify-phase0  Run Phase 0 verification' \
      '  just verify-phase1  Run Phase 1 verification' \
      '  just verify-phase2  Run Phase 2 verification' \
      '  just verify-phase3  Run Phase 3 semantic frontend verification' \
      '  just verify-phase4  Run Phase 4 IR/VM/runtime verification' \
      '  just verify-phase5  Run Phase 5 runtime semantics verification' \
      '  just verify-phase6  Run Phase 6 standard-library verification' \
      '  just test-phase6  Run Phase 6 documentation and preflight tests' \
      '  just coverage-phase6  Run Phase 6 coverage document checks' \
      '  just phase6-generate-arginfo  Generate optional Phase 6 arginfo metadata from php-src stubs' \
      '  just diff-stdlib  Run Phase 6 stdlib differential gate' \
      '  just diff-streams  Run Phase 6 streams differential gate' \
      '  just diff-json-pcre-date  Run Phase 6 JSON/PCRE/Date differential gate' \
      '  just diff-spl-reflection  Run Phase 6 SPL/Reflection differential gate' \
      '  just composer-smoke  Run Phase 6 Composer compatibility smoke gate' \
      '  just composer-smoke-source  Run Phase 6 Composer source-mode smoke gate' \
      '  just composer-smoke-platform  Run Phase 6 Composer platform-check smoke gate' \
      '  just process-capability-smoke  Run Phase 6 default-off process capability smoke gate' \
      '  just phase6-phpt-smoke  Run Phase 6 selected extension PHPT smoke gate' \
      '  just phase6-corpus-smoke  Run Phase 6 Composer/framework-style regression corpus' \
      '  just verify-phase7  Run Phase 7 performance-layer verification' \
      '  just test-phase7  Run Phase 7 Rust test gate' \
      '  just regression-phase7  Run Phase 7 baseline regression smoke' \
      '  just perf-flag-matrix  Run Phase 7 performance flag A/B matrix' \
      '  just bench-phase7-smoke  Run Phase 7 benchmark smoke gate' \
      '  just bench-phase7-callgrind-smoke  Run optional Phase 7 Callgrind smoke' \
      '  just bench-rust-phase7  Run Phase 7 Criterion Rust hot-path benchmarks' \
      '  just bench-phase7  Run Phase 7 benchmark suite' \
      '  just profile-phase7-dispatch  Print/run optional VM dispatch profiling recipe' \
      '  just profile-phase7-arrays  Print/run optional array-heavy profiling recipe' \
      '  just profile-phase7-calls  Print/run optional call-heavy profiling recipe' \
      '  just profile-phase7-composer  Print/run optional Composer-like profiling recipe' \
      '  just release-profile-plan-phase7  Print optional Phase 7 LTO/PGO release build plan' \
      '  just framework-smoke-phase7  Run optional offline framework-like Phase 7 smokes' \
      '  just hotpaths-phase7  Generate Phase 7 hot-path inventory' \
      '  just ir-verify-phase7  Run Phase 7 IR verifier gate' \
      '  just perf-baseline  Capture Phase 7 performance baseline' \
      '  just perf-compare  Compare Phase 7 performance reports' \
      '  just cache-fingerprint-smoke  Run Phase 7 bytecode-cache fingerprint smoke' \
      '  just cache-roundtrip  Run Phase 7 bytecode-cache roundtrip gate' \
      '  just optimizer-diff  Run Phase 7 optimizer differential gate' \
      '  just quickening-smoke  Run Phase 7 quickening smoke gate' \
      '  just inline-cache-smoke  Run Phase 7 inline-cache smoke gate' \
      '  just polymorphic-inline-cache-smoke  Run Phase 7 polymorphic IC smoke gate' \
      '  just jit-smoke  Run Phase 7 default-off JIT smoke gate' \
      '  just jit-cranelift-smoke  Run Phase 7 Cranelift feature-gating smoke gate' \
      '  just jit-cranelift-diff  Run Phase 7 Cranelift off-vs-on differential gate' \
      '  just jit-cranelift-bench-smoke  Run Phase 7 Cranelift int-arithmetic bench/report smoke' \
      '  just jit-cranelift-report  Generate Phase 7 Cranelift big-win report' \
      '  just cranelift-guard-report  Generate Phase 7 Cranelift side-exit/guard report' \
      '  just jit-cranelift-disasm  Generate optional Phase 7 Cranelift code-size/CLIF dumps' \
      '  just jit-cranelift-fuzz-smoke  Run bounded Cranelift eligible-IR fuzz smoke' \
      '  just jit-cranelift-poly-ic-experiment  Run optional local polymorphic IC guard experiment' \
      '  just jit-cranelift-framework-smoke  Run optional offline framework-like Cranelift smokes' \
      '  just verify-phase7-cranelift  Run Phase 7 Cranelift addendum verification' \
      '  just dump-cranelift-clif  Write and verify the Phase 7 Cranelift CLIF smoke dump' \
      '  just phase7-safety-audit-smoke  Run optional Phase 7 safety audit smoke' \
      '  just perf-report  Generate Phase 7 performance report' \
      '  just bootstrap-ref  Clone/pin the PHP reference checkout' \
      '  just verify-ref     Verify PHP reference checkout against lockfile' \
      '  just dump-reference-tokens  Dump PHP T_* constants as JSON' \
      '  just tokenize-ref FILE  Tokenize FILE with reference PHP' \
      '  just lex FILE        Tokenize FILE with Rust lexer CLI' \
      '  just lexer-ref FILE  Alias for tokenize-ref' \
      '  just lexer-fixtures  Run lexer fixture diff' \
      '  just lexer-diff      Run strict lexer fixture diff' \
      '  just lexer-diff-report  Write lexer diff JSON report' \
      '  just fuzz-lexer-smoke  Run lightweight lexer invariant tests' \
      '  just bench-lexer     Run lexer throughput baseline' \
      '  just lexer-corpus-smoke  Smoke-test extracted php-src corpus' \
      '  just parser-lint-oracle  Run parser fixtures through php -l JSON oracle' \
      '  just parser-fixtures  Run parser fixture oracle harness' \
      '  just parser-diff      Compare Rust parser acceptance with php -l' \
      '  just cst-roundtrip    Check exact CST reconstruction for parser fixtures' \
      '  just extract-parser-corpus  Extract optional php-src parser corpus' \
      '  just parser-corpus-smoke  Smoke-test extracted php-src parser corpus' \
      '  just fuzz-parser-smoke  Run parser property/fuzz smoke tests' \
      '  just bench-parser    Run parser performance smoke baseline' \
      '  just semantic-fixtures  Run semantic fixture harness' \
      '  just semantic-reference-smoke  Run reference PHP frontend smoke check' \
      '  just semantic-diff  Compare semantic acceptance with PHP reference' \
      '  just semantic-diff-strict  Strict semantic acceptance comparison' \
      '  just frontend-snapshots  Run frontend CLI/API snapshot smoke tests' \
      '  just semantic-corpus-smoke  Optional semantic corpus smoke gate' \
      '  just fuzz-frontend-smoke  Optional frontend fuzz/property smoke gate' \
      '  just bench-frontend  Optional frontend benchmark gate' \
      '  just bytecode-snapshots  Run Phase 4 bytecode snapshot checks' \
      '  just vm-smoke  Run Phase 4 VM smoke checks' \
      '  just vm-trace-smoke  Run Phase 4 VM trace/debug smoke checks' \
      '  just runtime-fixtures  Run Phase 4 runtime fixture checks' \
      '  just runtime-corpus-smoke  Run Phase 4 self-contained corpus smoke checks' \
      '  just runtime-reference-smoke  Run optional Phase 4 PHP reference smoke; skips when REFERENCE_PHP is unset' \
      '  just runtime-diff  Compare Phase 4 runtime output with PHP reference when REFERENCE_PHP is set' \
      '  just phpt-smoke  Run selected PHP .phpt smoke checks' \
      '  just runtime-known-gaps  Validate Phase 4 runtime known-gap catalog' \
      '  just bench-vm-smoke  Run optional Phase 4 VM benchmark smoke' \
      '  just fuzz-vm-smoke  Run optional Phase 4 VM fuzz/property smoke' \
      '  just phase5-fixtures  Run Phase 5 runtime semantics fixture gates' \
      '  just phase5-diff  Run Phase 5 differential smoke gate' \
      '  just phase5-toolchain-audit  Check Phase 5 devshell tools' \
      '  just runtime-hardening-lints  Run runtime/VM hardening lints' \
      '  just phase5-miri-smoke  Opt-in Miri smoke for runtime/VM model tests' \
      '  just phase5-sanitizer-smoke  Opt-in sanitizer smoke when supported' \
      '  just phase5-fuzz-smoke  Opt-in deterministic fuzz smoke for refs, COW arrays, and foreach' \
      '  just phase5-bench-smoke  Opt-in local microbenchmark smoke for Phase 5 categories' \
      '  just phase5-composer-smoke  Opt-in local Composer fixture smoke via PHPRUST_COMPOSER_FIXTURE_DIR' \
      '  just refs-cow-fixtures  Run Phase 5 references/COW fixture gate' \
      '  just object-semantics-fixtures  Run Phase 5 object semantics fixture gate' \
      '  just generator-fiber-fixtures  Run Phase 5 generator/fiber fixture gate' \
      '  just real-world-fixtures  Run Phase 5 offline real-world fixture gate' \
      '  just regression-fixtures  Run Phase 5 minimized regression fixture gate' \
      '  just phase5-local-composer-smoke <paths>  Opt-in local Composer-style smoke over user-provided paths' \
      '  just phase5-phpt-smoke  Run Phase 5 PHPT smoke gate' \
      '  just parser-snapshots Update parser CST and diagnostic snapshots' \
      '  just extract-ref-metadata  Extract deterministic PHP reference metadata' \
      '  just build-ref-php  Build optional minimal reference PHP CLI' \
      '  just ref-php-version  Show reference PHP CLI version when built' \
      '  just ref-tokenizer-check  Check token_get_all in reference PHP CLI'

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

verify:
    @just verify-phase4

verify-foundation:
    @just verify-phase0

verify-lexer:
    @just verify-phase1

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

verify-phase0:
    scripts/verify-phase0.sh

verify-phase1:
    scripts/verify-phase1.sh

verify-phase2:
    scripts/verify-phase2.sh

verify-phase3:
    scripts/verify-phase3.sh

verify-phase4:
    scripts/verify-phase4.sh

verify-phase5:
    scripts/verify-phase5.sh

verify-phase6:
    scripts/verify-phase6.sh

test-phase6:
    scripts/test-phase6.sh

coverage-phase6:
    scripts/coverage-phase6.sh

phase6-generate-arginfo php_src="third_party/php-src" out="target/phase6/generated/arginfo.rs":
    scripts/phase6/generate_arginfo.py --php-src "{{php_src}}" --overrides fixtures/phase6/arginfo_overrides.txt --out "{{out}}"

diff-stdlib:
    cargo build -q -p php_vm_cli --bin php-vm
    scripts/phase6_diff.py --area stdlib --out target/phase6/diff-stdlib --vm-binary ${CARGO_TARGET_DIR:-target}/debug/php-vm

diff-streams:
    cargo build -q -p php_vm_cli --bin php-vm
    scripts/phase6_diff.py --area streams --out target/phase6/diff-streams --vm-binary ${CARGO_TARGET_DIR:-target}/debug/php-vm

diff-json-pcre-date:
    cargo build -q -p php_vm_cli --bin php-vm
    scripts/phase6_diff.py --area json-pcre-date --out target/phase6/diff-json-pcre-date --vm-binary ${CARGO_TARGET_DIR:-target}/debug/php-vm

diff-spl-reflection:
    cargo build -q -p php_vm_cli --bin php-vm
    scripts/phase6_diff.py --area spl-reflection --out target/phase6/diff-spl-reflection --vm-binary ${CARGO_TARGET_DIR:-target}/debug/php-vm

composer-smoke:
    cargo build -q -p php_vm_cli --bin php-vm
    scripts/phase6_diff.py --area composer --out target/phase6/composer-smoke --vm-binary ${CARGO_TARGET_DIR:-target}/debug/php-vm

composer-fixture-prepare:
    scripts/phase6_prepare_composer_fixture.sh

composer-smoke-source:
    scripts/phase6/composer_source_smoke.sh

composer-smoke-autoload:
    cargo build -q -p php_vm_cli --bin php-vm
    scripts/phase6_diff.py --file tests/fixtures/phase6/_harness/composer/basic_project_autoload_order.php --out target/phase6/composer-smoke-autoload --vm-binary ${CARGO_TARGET_DIR:-target}/debug/php-vm

composer-smoke-platform:
    cargo build -q -p php_vm_cli --bin php-vm
    scripts/phase6_diff.py --file tests/fixtures/phase6/_harness/composer/basic_project_platform_check.php --file tests/fixtures/phase6/_harness/composer/platform_version_compare.php --out target/phase6/composer-smoke-platform --vm-binary ${CARGO_TARGET_DIR:-target}/debug/php-vm

process-capability-smoke:
    scripts/phase6_process_capability_smoke.sh

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
    @printf '%s\n' '[skip] semantic corpus smoke is not configured for Phase 3; curated fixtures are covered by semantic-fixtures.'

fuzz-frontend-smoke:
    @printf '%s\n' '[skip] frontend fuzz smoke is not configured for Phase 3; parser fuzz smoke remains available via just fuzz-parser-smoke.'

bench-frontend:
    @printf '%s\n' '[skip] frontend benchmarks are not configured for Phase 3; no benchmark baseline is defined yet.'

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
    printf 'hello phase4\n' > "$tmp_dir/hello.expected"; \
    cmp "$tmp_dir/hello.expected" "$tmp_dir/hello.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/scalars/echo.php > "$tmp_dir/scalar.out"; \
    printf 'scalar echo\n' > "$tmp_dir/scalar.expected"; \
    cmp "$tmp_dir/scalar.expected" "$tmp_dir/scalar.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/bytecode/lower/valid/empty.php > "$tmp_dir/empty.out"; \
    test ! -s "$tmp_dir/empty.out"; \
    printf '%s\n' '[ok] Phase 4 VM smoke fixtures passed.'

vm-trace-smoke:
    cargo build -p php_vm_cli
    @tmp_dir="$PWD/target/phase4/failures"; \
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
    printf '%s\n' '[ok] Phase 4 VM trace/debug smoke passed.'

runtime-fixtures:
    cargo build -p php_vm_cli
    @tmp_dir="target/runtime-fixtures"; \
    mkdir -p "$tmp_dir"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/hello.php > "$tmp_dir/hello.out"; \
    printf 'hello phase4\n' > "$tmp_dir/hello.expected"; \
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
    printf '42|phase4\n' > "$tmp_dir/constants-global.expected"; \
    cmp "$tmp_dir/constants-global.expected" "$tmp_dir/constants-global.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/constants/builtin.php > "$tmp_dir/constants-builtin.out"; \
    printf '8.5.7\n' > "$tmp_dir/constants-builtin.expected"; \
    cmp "$tmp_dir/constants-builtin.expected" "$tmp_dir/constants-builtin.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/constants/magic-top-level.php > "$tmp_dir/constants-magic-top.out"; \
    printf '%s\n%s\n4||||\n' "$PWD/fixtures/runtime/valid/constants/magic-top-level.php" "$PWD/fixtures/runtime/valid/constants" > "$tmp_dir/constants-magic-top.expected"; \
    cmp "$tmp_dir/constants-magic-top.expected" "$tmp_dir/constants-magic-top.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/constants/magic-function.php > "$tmp_dir/constants-magic-function.out"; \
    printf 'prompt24_magic_function|3||prompt24_magic_function|\n' > "$tmp_dir/constants-magic-function.expected"; \
    cmp "$tmp_dir/constants-magic-function.expected" "$tmp_dir/constants-magic-function.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/constants/magic-method.php > "$tmp_dir/constants-magic-method.out"; \
    printf 'Prompt24MagicMethod|Prompt24MagicMethod::show\n' > "$tmp_dir/constants-magic-method.expected"; \
    cmp "$tmp_dir/constants-magic-method.expected" "$tmp_dir/constants-magic-method.out"; \
    set +e; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/invalid/constants/undefined.php > "$tmp_dir/constants-undefined.out" 2> "$tmp_dir/constants-undefined.err"; \
    code=$?; \
    set -e; \
    test "$code" -eq 3; \
    grep -q 'E_PHP_RUNTIME_UNDEFINED_CONSTANT' "$tmp_dir/constants-undefined.err"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/known_gaps/variables/undefined.php > "$tmp_dir/variables-undefined.out" 2> "$tmp_dir/variables-undefined.err"; \
    printf 'x\n' > "$tmp_dir/variables-undefined.expected"; \
    cmp "$tmp_dir/variables-undefined.expected" "$tmp_dir/variables-undefined.out"; \
    grep -q 'E_PHP_RUNTIME_UNDEFINED_VARIABLE_WARNING' "$tmp_dir/variables-undefined.err"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/errors/warning-continuation.php > "$tmp_dir/errors-warning-continuation.out" 2> "$tmp_dir/errors-warning-continuation.err"; \
    printf 'ok\n' > "$tmp_dir/errors-warning-continuation.expected"; \
    cmp "$tmp_dir/errors-warning-continuation.expected" "$tmp_dir/errors-warning-continuation.out"; \
    grep -q 'runtime-diagnostic:' "$tmp_dir/errors-warning-continuation.err"; \
    grep -q 'E_PHP_RUNTIME_UNDEFINED_VARIABLE_WARNING' "$tmp_dir/errors-warning-continuation.err"; \
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
    printf 'finally\n' > "$tmp_dir/exceptions-finally-throw.expected"; \
    cmp "$tmp_dir/exceptions-finally-throw.expected" "$tmp_dir/exceptions-finally-throw.out"; \
    grep -q 'E_PHP_VM_UNCAUGHT_EXCEPTION' "$tmp_dir/exceptions-finally-throw.err"; \
    set +e; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/invalid/exceptions/rethrow.php > "$tmp_dir/exceptions-rethrow.out" 2> "$tmp_dir/exceptions-rethrow.err"; \
    code=$?; \
    set -e; \
    test "$code" -eq 3; \
    printf 'catch\n' > "$tmp_dir/exceptions-rethrow.expected"; \
    cmp "$tmp_dir/exceptions-rethrow.expected" "$tmp_dir/exceptions-rethrow.out"; \
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
    grep -q 'runtime_error: undefined function phase4_missing_function' "$tmp_dir/errors-undefined-function.err"; \
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
    grep -q 'E_PHP_VM_TOO_FEW_ARGS' "$tmp_dir/functions-missing-arg.err"; \
    set +e; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/invalid/functions/extra-arg.php > "$tmp_dir/functions-extra-arg.out" 2> "$tmp_dir/functions-extra-arg.err"; \
    code=$?; \
    set -e; \
    test "$code" -eq 3; \
    grep -q 'E_PHP_VM_TOO_MANY_ARGS' "$tmp_dir/functions-extra-arg.err"; \
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
    printf '%s\n' '[ok] Phase 4 runtime fixtures passed.'

runtime-corpus-smoke:
    scripts/runtime-corpus-smoke.sh

runtime-reference-smoke:
    cargo test -p php_testkit runtime_reference_smoke -- --nocapture

runtime-diff:
    cargo build -p php_vm_cli -p php_testkit --bin compare-runtime
    ${CARGO_TARGET_DIR:-target}/debug/compare-runtime --fixtures fixtures/runtime --out target/phase4/runtime-diff --rust-vm ${CARGO_TARGET_DIR:-target}/debug/php-vm

phpt-smoke:
    cargo build -p php_vm_cli -p php_testkit --bin run-phpt-smoke
    ${CARGO_TARGET_DIR:-target}/debug/run-phpt-smoke --fixtures fixtures/phpt_smoke --out target/phase4/phpt-smoke --rust-vm ${CARGO_TARGET_DIR:-target}/debug/php-vm

runtime-known-gaps:
    cargo build -p php_vm_cli
    test -s docs/phase4-known-gaps.md
    grep -q 'E_PHP_RUNTIME_UNSUPPORTED_REFERENCE_SEMANTICS' docs/phase4-known-gaps.md
    grep -q 'E_PHP_IR_UNSUPPORTED_GENERATOR' docs/phase4-known-gaps.md
    grep -q 'E_PHP_IR_UNSUPPORTED_YIELD_FROM' docs/phase4-known-gaps.md
    grep -q 'E_PHP_IR_UNSUPPORTED_FIBER' docs/phase4-known-gaps.md
    grep -q 'E_PHP_IR_UNSUPPORTED_EVAL' docs/phase4-known-gaps.md
    grep -q 'E_PHP_IR_UNSUPPORTED_AUTOLOAD' docs/phase4-known-gaps.md
    grep -q 'E_PHP_IR_UNSUPPORTED_REFLECTION' docs/phase4-known-gaps.md
    grep -q 'E_PHP_IR_UNSUPPORTED_TRAIT_RUNTIME' docs/phase4-known-gaps.md
    grep -q 'E_PHP_IR_UNSUPPORTED_ENUM_RUNTIME' docs/phase4-known-gaps.md
    grep -q 'E_PHP_IR_UNSUPPORTED_PROPERTY_HOOKS' docs/phase4-known-gaps.md
    grep -q 'E_PHP_IR_UNSUPPORTED_BY_REF_FOREACH' docs/phase4-known-gaps.md
    grep -q 'E_PHP_RUNTIME_SUPERGLOBALS_FULL_MATRIX' docs/phase4-known-gaps.md
    grep -q 'E_PHP_RUNTIME_GLOBALS_ALIAS_MATRIX' docs/phase4-known-gaps.md
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
    printf '%s\n' '[ok] Phase 4 runtime known-gap catalog and reference fixtures passed.'

bench-vm-smoke:
    cargo build -p php_vm_cli
    mkdir -p target/phase4/bench-vm-smoke
    rustc --edition=2024 tools/bench_vm_smoke.rs -o target/phase4/bench-vm-smoke/bench-vm-smoke
    target/phase4/bench-vm-smoke/bench-vm-smoke

fuzz-vm-smoke:
    cargo build -p php_vm_cli
    mkdir -p target/phase4/fuzz-vm-smoke
    rustc --edition=2024 tools/fuzz_vm_smoke.rs -o target/phase4/fuzz-vm-smoke/fuzz-vm-smoke
    target/phase4/fuzz-vm-smoke/fuzz-vm-smoke

phase5-fixtures:
    @just refs-cow-fixtures
    @just object-semantics-fixtures
    @just generator-fiber-fixtures
    @just real-world-fixtures
    @just regression-fixtures
    @printf '%s\n' '[ok] Phase 5 fixture gates complete.'

phase5-diff *args:
    cargo build -p php_vm_cli
    scripts/phase5_diff.py {{args}}

phase5-toolchain-audit:
    @for tool in cargo rustc rustfmt cargo-clippy just jq python3 rg clang sccache; do \
      if ! command -v "$tool" >/dev/null 2>&1; then \
        printf '%s\n' "[missing] required Phase 5 devshell tool: $tool" >&2; \
        exit 1; \
      fi; \
    done; \
    @if ! command -v shellcheck >/dev/null 2>&1; then \
      case "$$(uname -s)" in \
        Darwin) printf '%s\n' '[skip] shellcheck unavailable; Darwin devshell omits it to avoid the Haskell closure';; \
        *) printf '%s\n' '[missing] required Phase 5 devshell tool: shellcheck' >&2; exit 1;; \
      esac; \
    fi; \
    test "${PHP_REF_SERIES:-}" = "8.5"; \
    test "${PHP_REF_VERSION:-}" = "8.5.7"; \
    test "${PHP_REF_TAG:-}" = "php-8.5.7"; \
    test -n "${CARGO_TARGET_DIR:-}"; \
    test -n "${SCCACHE_DIR:-}"; \
    printf '%s\n' '[ok] Phase 5 devshell toolchain audit passed'

phase5-miri-smoke:
    @if ! command -v cargo-miri >/dev/null 2>&1 && ! cargo miri --version >/dev/null 2>&1; then \
      printf '%s\n' '[skip] cargo-miri is not available in this toolchain; install a Miri-capable Rust toolchain to run this opt-in smoke.'; \
      exit 0; \
    fi; \
    if ! cargo miri --version >/dev/null 2>&1; then \
      printf '%s\n' '[skip] cargo-miri is present but not usable for the active toolchain; this opt-in smoke is not part of verify-phase5.'; \
      exit 0; \
    fi; \
    cargo miri test -p php_runtime reference::tests::slot_alias_and_copy_semantics_are_distinct

phase5-sanitizer-smoke:
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

phase5-fuzz-smoke:
    cargo build -p php_vm_cli
    scripts/phase5_fuzz_smoke.py

phase5-bench-smoke:
    cargo build -p php_vm_cli
    scripts/phase5_bench_smoke.py

phase5-composer-smoke:
    @if [ -z "${PHPRUST_COMPOSER_FIXTURE_DIR:-}" ]; then \
        scripts/phase5_composer_smoke.py; \
    else \
        cargo build -p php_vm_cli; \
        scripts/phase5_composer_smoke.py; \
    fi

refs-cow-fixtures:
    cargo build -p php_vm_cli
    scripts/phase5_diff.py --category refs --category cow --out target/phase5/refs-cow

object-semantics-fixtures:
    cargo build -p php_vm_cli
    scripts/phase5_diff.py --category objects --category traits --category enums --category magic --category properties --category property_hooks --category clone_with --out target/phase5/object-semantics

generator-fiber-fixtures:
    cargo build -p php_vm_cli
    scripts/phase5_diff.py --category generators --category fibers --out target/phase5/generator-fiber

real-world-fixtures:
    cargo build -p php_vm_cli
    scripts/phase5_diff.py --category real_world --out target/phase5/real-world

regression-fixtures:
    cargo build -p php_vm_cli
    scripts/phase5_diff.py --category regressions --out target/phase5/regressions

phase5-local-composer-smoke *paths:
    @if [ -z "{{paths}}" ]; then \
        printf '%s\n' '[skip] provide one or more local Composer project paths: just phase5-local-composer-smoke path/to/project'; \
        exit 0; \
    fi; \
    cargo build -p php_vm_cli; \
    args=''; \
    for path in {{paths}}; do args="$$args --dir $$path"; done; \
    scripts/phase5_diff.py $$args --out target/phase5/local-composer-smoke

phase5-phpt-smoke:
    cargo build -p php_vm_cli -p php_testkit --bin run-phpt-smoke
    ${CARGO_TARGET_DIR:-target}/debug/run-phpt-smoke --fixtures /private/tmp/phrust-empty-phpt-smoke --out target/phase5/phpt-smoke --rust-vm ${CARGO_TARGET_DIR:-target}/debug/php-vm --allowlist fixtures/phase5/phpt_allowlist.toml

phase6-phpt-smoke:
    cargo build -p php_vm_cli -p php_testkit --bin run-phpt-smoke
    scripts/phase6/phpt_extension_selector.py

phase6-corpus-smoke:
    cargo build -p php_vm_cli
    scripts/phase6_diff.py --fixtures tests/fixtures/phase6/corpus --area corpus --out target/phase6/corpus

verify-phase7:
    @just test-phase7
    @just regression-phase7
    @just cache-roundtrip
    @just optimizer-diff
    @just quickening-smoke
    @just inline-cache-smoke
    @just bench-phase7-callgrind-smoke
    @just jit-smoke
    @just phase7-safety-audit-smoke
    @just bench-phase7-smoke
    @just hotpaths-phase7
    @just perf-report
    @printf '%s\n' '[pass] phase7 verification complete'

test-phase7:
    cargo test --workspace
    scripts/phase7/compare_perf_json.py --self-test
    scripts/phase7/hotpath_inventory.py --self-test
    scripts/phase7/perf_report.py --self-test

regression-phase7:
    scripts/phase6_regression_smoke.sh
    cargo build -p php_vm_cli --bin php-vm
    scripts/phase7/regression_smoke.sh
    @just perf-flag-matrix
    @just polymorphic-inline-cache-smoke

perf-flag-matrix:
    scripts/phase7/perf_flag_matrix.py

ir-verify-phase7:
    cargo test -p php_ir verify --lib

bench-phase7-smoke:
    cargo build -p php_vm_cli --bin php-vm
    scripts/phase7/bench_matrix.py --engine "${CARGO_TARGET_DIR:-target}/debug/php-vm" --out target/phase7/bench-phase7-smoke.json --repetitions "${PHRUST_PHASE7_BENCH_SMOKE_REPETITIONS:-1}" --warmups "${PHRUST_PHASE7_BENCH_SMOKE_WARMUPS:-0}" --timeout "${PHRUST_PHASE7_BENCH_TIMEOUT:-10.0}"

bench-phase7-callgrind-smoke:
    scripts/phase7/callgrind_smoke.sh

bench-rust-phase7:
    CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-target}" cargo bench --manifest-path crates/php_bench/Cargo.toml --bench phase7_hotpaths

bench-phase7:
    cargo build -p php_vm_cli --bin php-vm
    scripts/phase7/bench_matrix.py --engine "${CARGO_TARGET_DIR:-target}/debug/php-vm" --out target/phase7/bench-phase7.json --repetitions "${PHRUST_PHASE7_BENCH_REPETITIONS:-5}" --warmups "${PHRUST_PHASE7_BENCH_WARMUPS:-1}" --timeout "${PHRUST_PHASE7_BENCH_TIMEOUT:-5.0}"
    @just bench-rust-phase7

profile-phase7-dispatch:
    scripts/phase7/profile_smoke.sh dispatch

profile-phase7-arrays:
    scripts/phase7/profile_smoke.sh arrays

profile-phase7-calls:
    scripts/phase7/profile_smoke.sh calls

profile-phase7-composer:
    scripts/phase7/profile_smoke.sh composer

release-profile-plan-phase7:
    scripts/phase7/release_profile_plan.sh

framework-smoke-phase7:
    cargo build -p php_vm_cli --bin php-vm
    scripts/phase7/framework_micro_smoke.py

hotpaths-phase7:
    scripts/phase7/hotpath_inventory.py target/phase7/bench-phase7-smoke.json --json-out target/phase7/hotpaths.json --markdown-out docs/hotpaths-phase7.md

perf-baseline:
    cargo build -p php_vm_cli --bin php-vm
    scripts/phase7/bench_matrix.py --engine "${CARGO_TARGET_DIR:-target}/debug/php-vm" --out target/phase7/baseline.json --repetitions "${PHRUST_PHASE7_BASELINE_REPETITIONS:-3}" --warmups "${PHRUST_PHASE7_BASELINE_WARMUPS:-1}" --timeout "${PHRUST_PHASE7_BENCH_TIMEOUT:-5.0}"

perf-compare:
    @if [ ! -f target/phase7/baseline.json ]; then \
        printf '%s\n' '[skip] target/phase7/baseline.json missing; run `just perf-baseline` first'; \
        exit 0; \
    fi
    @just bench-phase7-smoke
    scripts/phase7/compare_perf_json.py target/phase7/baseline.json target/phase7/bench-phase7-smoke.json --out target/phase7/perf-compare.md --json-out target/phase7/perf-compare.json

cache-roundtrip:
    @just cache-fingerprint-smoke
    cargo test -p php_bytecode_cache bytecode_cache
    cargo test -p php_vm_cli bytecode_cache

cache-fingerprint-smoke:
    scripts/phase7/cache_fingerprint_smoke.sh

optimizer-diff:
    @just ir-verify-phase7
    scripts/phase7/optimizer_diff_smoke.sh

quickening-smoke:
    cargo build -p php_vm_cli --bin php-vm
    scripts/phase7/quickening_smoke.sh

inline-cache-smoke:
    cargo build -p php_vm_cli --bin php-vm
    scripts/phase7/inline_cache_smoke.sh

polymorphic-inline-cache-smoke:
    cargo build -p php_vm_cli --bin php-vm
    scripts/phase7/polymorphic_inline_cache_smoke.sh

jit-smoke:
    scripts/phase7/jit_smoke.sh

jit-cranelift-smoke:
    @set +e; scripts/phase7/cranelift/platform_check.py --out target/phase7/cranelift/platform.json; status=$?; set -e; if [ "$status" -eq 77 ]; then exit 0; elif [ "$status" -ne 0 ]; then exit "$status"; fi
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
    @set +e; scripts/phase7/cranelift/platform_check.py --out target/phase7/cranelift/platform.json; status=$?; set -e; if [ "$status" -eq 77 ]; then exit 0; elif [ "$status" -ne 0 ]; then exit "$status"; fi
    cargo build -p php_vm_cli --bin php-vm --features jit-cranelift
    scripts/phase7/cranelift/jit_diff.py --engine "${CARGO_TARGET_DIR:-target}/debug/php-vm"

jit-cranelift-bench-smoke:
    @set +e; scripts/phase7/cranelift/platform_check.py --out target/phase7/cranelift/platform.json; status=$?; set -e; if [ "$status" -eq 77 ]; then exit 0; elif [ "$status" -ne 0 ]; then exit "$status"; fi
    @just jit-cranelift-diff
    cargo build -p php_vm_cli --bin php-vm --features jit-cranelift
    scripts/phase7/cranelift/jit_bench_matrix.py --engine "${CARGO_TARGET_DIR:-target}/debug/php-vm" --out target/phase7/cranelift/bench-smoke.json --smoke

jit-cranelift-report:
    @set +e; scripts/phase7/cranelift/platform_check.py --out target/phase7/cranelift/platform.json; status=$?; set -e; if [ "$status" -eq 77 ]; then exit 0; elif [ "$status" -ne 0 ]; then exit "$status"; fi
    @just jit-cranelift-diff
    cargo build -p php_vm_cli --bin php-vm --features jit-cranelift
    scripts/phase7/cranelift/jit_bench_matrix.py --engine "${CARGO_TARGET_DIR:-target}/debug/php-vm" --out target/phase7/cranelift/big_wins_report.json

cranelift-guard-report:
    @set +e; scripts/phase7/cranelift/platform_check.py --out target/phase7/cranelift/platform.json; status=$?; set -e; if [ "$status" -eq 77 ]; then exit 0; elif [ "$status" -ne 0 ]; then exit "$status"; fi
    @just jit-cranelift-report
    scripts/phase7/cranelift/guard_failure_report.py --input target/phase7/cranelift/big_wins_report.json --out target/phase7/cranelift/guard-report.json --text-out target/phase7/cranelift/guard-report.txt

jit-cranelift-disasm:
    @set +e; scripts/phase7/cranelift/platform_check.py --out target/phase7/cranelift/platform.json; status=$?; set -e; if [ "$status" -eq 77 ]; then exit 0; elif [ "$status" -ne 0 ]; then exit "$status"; fi
    cargo build -p php_vm_cli --bin php-vm --features jit-cranelift
    scripts/phase7/cranelift/disasm_dump.py --engine "${CARGO_TARGET_DIR:-target}/debug/php-vm"

jit-cranelift-fuzz-smoke:
    @set +e; scripts/phase7/cranelift/platform_check.py --out target/phase7/cranelift/platform.json; status=$?; set -e; if [ "$status" -eq 77 ]; then exit 0; elif [ "$status" -ne 0 ]; then exit "$status"; fi
    cargo build -p php_vm_cli --bin php-vm --features jit-cranelift
    scripts/phase7/cranelift/jit_eligible_ir_fuzz_smoke.py --engine "${CARGO_TARGET_DIR:-target}/debug/php-vm"

jit-cranelift-poly-ic-experiment:
    @set +e; scripts/phase7/cranelift/platform_check.py --out target/phase7/cranelift/platform.json; status=$?; set -e; if [ "$status" -eq 77 ]; then exit 0; elif [ "$status" -ne 0 ]; then exit "$status"; fi
    cargo build -p php_vm_cli --bin php-vm --features jit-cranelift
    scripts/phase7/cranelift/polymorphic_ic_experiment.py --engine "${CARGO_TARGET_DIR:-target}/debug/php-vm"
    @just jit-cranelift-report
    scripts/phase7/cranelift/guard_failure_report.py --input target/phase7/cranelift/big_wins_report.json --out target/phase7/cranelift/polymorphic-ic/guard-report.json --text-out target/phase7/cranelift/polymorphic-ic/guard-report.txt --experimental-ic-report target/phase7/cranelift/polymorphic-ic/report.json

jit-cranelift-framework-smoke:
    @set +e; scripts/phase7/cranelift/platform_check.py --out target/phase7/cranelift/platform.json; status=$?; set -e; if [ "$status" -eq 77 ]; then exit 0; elif [ "$status" -ne 0 ]; then exit "$status"; fi
    cargo build -p php_vm_cli --bin php-vm --features jit-cranelift
    scripts/phase7/cranelift/framework_smoke.py --engine "${CARGO_TARGET_DIR:-target}/debug/php-vm"

verify-phase7-cranelift:
    @set +e; scripts/phase7/cranelift/platform_check.py --out target/phase7/cranelift/platform.json; status=$?; set -e; if [ "$status" -eq 77 ]; then exit 0; elif [ "$status" -ne 0 ]; then exit "$status"; fi
    @just jit-cranelift-smoke
    @just jit-cranelift-diff
    @just jit-cranelift-bench-smoke
    @just jit-cranelift-report
    @just cranelift-guard-report
    @just jit-cranelift-fuzz-smoke
    @printf '%s\n' '[pass] phase7 Cranelift addendum verification complete'

dump-cranelift-clif:
    cargo run -p php_vm_cli --bin php-vm --features jit-cranelift -- dump-cranelift-clif

phase7-safety-audit-smoke:
    @if rg -n '\bunsafe\b' crates/php_bytecode_cache crates/php_vm/src/inline_cache.rs crates/php_vm/src/quickening.rs crates/php_vm/src/tiering.rs; then \
        printf '%s\n' '[fail] Phase 7 cache/JIT/adaptive surface contains Rust unsafe' >&2; \
        exit 1; \
    fi
    @if rg -n '\bunsafe\b' crates/php_jit/src --glob '!lib.rs' --glob '!helpers.rs' --glob '!cranelift_lowering.rs'; then \
        printf '%s\n' '[fail] Phase 7 default JIT surface contains unaudited Rust unsafe' >&2; \
        exit 1; \
    fi
    @test -f docs/safety-audit-cranelift-phase7.md
    cargo test -p php_bytecode_cache corrupt
    cargo test -p php_vm_cli bytecode_cache
    @if ! command -v cargo-miri >/dev/null 2>&1 && ! cargo miri --version >/dev/null 2>&1; then \
        printf '%s\n' '[skip] cargo-miri is not available in this toolchain; Phase 7 Miri audit smoke skipped.'; \
        exit 0; \
    fi; \
    if ! cargo miri --version >/dev/null 2>&1; then \
        printf '%s\n' '[skip] cargo-miri is present but not usable for the active toolchain; Phase 7 Miri audit smoke skipped.'; \
        exit 0; \
    fi; \
    cargo miri test -p php_bytecode_cache rejects_corrupt_input

perf-report:
    scripts/phase7/perf_report.py

_phase7-todo gate prompt:
    @mkdir -p target/phase7
    @test -f docs/phase7-gate-todos.md
    @printf '{"gate":"{{gate}}","status":"todo","planned_prompt":"{{prompt}}","todo":"docs/phase7-gate-todos.md"}\n' > "target/phase7/{{gate}}.json"
    @printf '%s\n' "[todo] {{gate}} placeholder until {{prompt}}; see docs/phase7-gate-todos.md"
