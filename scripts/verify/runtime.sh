#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT}"

require_file() {
  local path="$1"
  if [[ ! -e "$path" ]]; then
    printf '[error] missing required file: %s\n' "$path" >&2
    exit 1
  fi
  printf '[ok] file exists: %s\n' "$path"
}

require_grep() {
  local pattern="$1"
  local path="$2"
  if ! grep -Eq "$pattern" "$path"; then
    printf '[error] missing expected pattern in %s: %s\n' "$path" "$pattern" >&2
    exit 1
  fi
  printf '[ok] pattern found in %s: %s\n' "$path" "$pattern"
}

require_file docs/runtime/contract.md
require_file docs/runtime/values.md
require_file docs/runtime/vm.md
require_file docs/runtime/ir.md
require_file docs/runtime/reference.md
require_file docs/runtime/reference-diff.md
require_file docs/runtime/supported-subset.md
require_file docs/runtime/known-gaps.md
require_file docs/runtime/semantics-status.md
require_file docs/research/zend-opcode-mapping-runtime.md
require_file docs/research/embedding-spike.md
require_file docs/research/runtime-bench-smoke.md
require_file docs/adr/0010-runtime-known-gap-policy.md
require_file fixtures/bytecode/valid/manual-basic.ir.snap
require_file fixtures/bytecode/valid/literals-single.ir.snap
require_file fixtures/bytecode/valid/literals-multiple.ir.snap
require_file fixtures/bytecode/valid/source-map.ir.snap
require_file fixtures/bytecode/valid/foreach.ir.snap
require_file fixtures/bytecode/valid/include.ir.snap
require_file fixtures/bytecode/lower/valid/empty.php
require_file fixtures/bytecode/lower/valid/open-tag.php
require_file fixtures/bytecode/lower/valid/echo.php
require_file fixtures/bytecode/lower/valid/foreach.php
require_file fixtures/bytecode/lower/valid/include.php
require_file fixtures/bytecode/lower/known_gaps/generator.php
require_file fixtures/bytecode/literals/valid/echo-int.php
require_file fixtures/bytecode/literals/valid/echo-multiple.php
require_file fixtures/bytecode/literals/valid/echo-source-map.php
require_file crates/php_testkit/src/runtime_reference.rs
require_file crates/php_testkit/src/runtime_fixture.rs
require_file crates/php_testkit/src/normalize_output.rs
require_file crates/php_testkit/src/phpt.rs
require_file crates/php_testkit/src/bin/compare_runtime.rs
require_file crates/php_testkit/src/bin/run_phpt_smoke.rs
require_file crates/php_ir/Cargo.toml
require_file crates/php_ir/src/lib.rs
require_file crates/php_ir/src/block.rs
require_file crates/php_ir/src/builder.rs
require_file crates/php_ir/src/constants.rs
require_file crates/php_ir/src/display.rs
require_file crates/php_ir/src/function.rs
require_file crates/php_ir/src/ids.rs
require_file crates/php_ir/src/instruction.rs
require_file crates/php_ir/src/lower.rs
require_file crates/php_ir/src/module.rs
require_file crates/php_ir/src/operand.rs
require_file crates/php_ir/src/source_map.rs
require_file crates/php_ir/src/verify.rs
require_file crates/php_ir/tests/bytecode_snapshots.rs
require_file crates/php_runtime/Cargo.toml
require_file crates/php_runtime/src/lib.rs
require_file crates/php_runtime/src/array.rs
require_file crates/php_runtime/src/builtins.rs
require_file crates/php_runtime/src/context.rs
require_file crates/php_runtime/src/diagnostic.rs
require_file crates/php_runtime/src/output.rs
require_file crates/php_runtime/src/reference.rs
require_file crates/php_runtime/src/status.rs
require_file crates/php_runtime/src/string.rs
require_file crates/php_runtime/src/value.rs
require_file crates/php_vm/Cargo.toml
require_file crates/php_vm/src/compiled_unit.rs
require_file crates/php_vm/src/frame.rs
require_file crates/php_vm/src/lib.rs
require_file crates/php_vm/src/vm.rs
require_file crates/php_vm_cli/Cargo.toml
require_file crates/php_vm_cli/src/main.rs
require_file tools/bench_vm_smoke.rs
require_file tools/fuzz_vm_smoke.rs
require_file fixtures/runtime/README.md
require_file fixtures/runtime/valid/hello.php
require_file fixtures/runtime/corpus_smoke/config-array.php
require_file fixtures/runtime/corpus_smoke/router-dispatch.php
require_file fixtures/runtime/corpus_smoke/class-methods.php
require_file fixtures/runtime/corpus_smoke/include-graph.php
require_file fixtures/runtime/corpus_smoke/error-case.php
require_file fixtures/runtime/corpus_smoke/lib/settings.php
require_file fixtures/runtime/corpus_smoke/lib/routes.php
require_file scripts/runtime-corpus-smoke.sh
require_file fixtures/runtime/valid/scalars/echo.php
require_file fixtures/runtime/valid/scalars/expressions.php
require_file fixtures/runtime/valid/scalars/comparisons.php
require_file fixtures/runtime/valid/scalars/casts.php
require_file fixtures/runtime/valid/variables/assignment.php
require_file fixtures/runtime/valid/variables/compound.php
require_file fixtures/runtime/valid/variables/inc-dec.php
require_file fixtures/runtime/valid/superglobals/argc.php
require_file fixtures/runtime/valid/superglobals/argv.php
require_file fixtures/runtime/valid/superglobals/server-argv.php
require_file fixtures/runtime/valid/superglobals/empty-superglobals.php
require_file fixtures/runtime/valid/references/by-value.php
require_file fixtures/runtime/valid/references/local-alias.php
require_file fixtures/runtime/valid/references/by-ref-param.php
require_file fixtures/runtime/valid/references/array-element-ref.php
require_file fixtures/runtime/valid/control_flow/if-true-false.php
require_file fixtures/runtime/valid/control_flow/nested-if.php
require_file fixtures/runtime/valid/control_flow/while-counter.php
require_file fixtures/runtime/valid/control_flow/do-while-once.php
require_file fixtures/runtime/valid/control_flow/for-loop.php
require_file fixtures/runtime/valid/control_flow/break.php
require_file fixtures/runtime/valid/control_flow/continue.php
require_file fixtures/runtime/valid/control_flow/short-circuit.php
require_file fixtures/runtime/valid/control_flow/ternary.php
require_file fixtures/runtime/valid/control_flow/null-coalesce.php
require_file fixtures/runtime/valid/control_flow/switch-fallthrough.php
require_file fixtures/runtime/valid/control_flow/match-success.php
require_file fixtures/runtime/valid/control_flow/return.php
require_file fixtures/runtime/valid/functions/simple.php
require_file fixtures/runtime/valid/functions/two-args.php
require_file fixtures/runtime/valid/functions/local-scope.php
require_file fixtures/runtime/valid/functions/factorial.php
require_file fixtures/runtime/valid/functions/return-no-value.php
require_file fixtures/runtime/valid/functions/defaults.php
require_file fixtures/runtime/valid/functions/variadic-sum.php
require_file fixtures/runtime/valid/functions/return-types.php
require_file fixtures/runtime/valid/functions/closure-simple.php
require_file fixtures/runtime/valid/functions/closure-use.php
require_file fixtures/runtime/valid/functions/arrow-capture.php
require_file fixtures/runtime/valid/functions/closure-return.php
require_file fixtures/runtime/valid/php85/pipe-user-function.php
require_file fixtures/runtime/valid/php85/pipe-closure.php
require_file fixtures/runtime/valid/php85/pipe-builtin.php
require_file fixtures/runtime/valid/php85/pipe-side-effects.php
require_file fixtures/runtime/valid/builtins/print.php
require_file fixtures/runtime/valid/builtins/gettype.php
require_file fixtures/runtime/valid/builtins/is-types.php
require_file fixtures/runtime/valid/builtins/var-dump-scalars.php
require_file fixtures/runtime/valid/builtins/var-dump-array.php
require_file fixtures/runtime/valid/arrays/indexed.php
require_file fixtures/runtime/valid/arrays/string-keys.php
require_file fixtures/runtime/valid/arrays/append-overwrite.php
require_file fixtures/runtime/valid/arrays/nested-fetch.php
require_file fixtures/runtime/valid/arrays/missing-key.php
require_file fixtures/runtime/valid/arrays/isset-empty-unset.php
require_file fixtures/runtime/valid/arrays/var-dump-mixed.php
require_file fixtures/runtime/valid/foreach/values.php
require_file fixtures/runtime/valid/foreach/key-value.php
require_file fixtures/runtime/valid/foreach/break-continue.php
require_file fixtures/runtime/valid/foreach/nested.php
require_file fixtures/runtime/valid/foreach/snapshot-mutation.php
require_file fixtures/runtime/valid/objects/instantiate.php
require_file fixtures/runtime/valid/objects/constructor-property.php
require_file fixtures/runtime/valid/objects/property-read-write.php
require_file fixtures/runtime/valid/objects/two-objects.php
require_file fixtures/runtime/valid/objects/private-property.php
require_file fixtures/runtime/valid/objects/private-method.php
require_file fixtures/runtime/valid/errors/warning-continuation.php
require_file fixtures/runtime/invalid/division-by-zero.php
require_file fixtures/runtime/invalid/type-error.php
require_file fixtures/runtime/invalid/runtime-error.php
require_file fixtures/runtime/invalid/match-no-arm.php
require_file fixtures/runtime/invalid/errors/undefined-function.php
require_file fixtures/runtime/invalid/functions/call-stack-error.php
require_file fixtures/runtime/invalid/functions/missing-arg.php
require_file fixtures/runtime/invalid/functions/extra-arg.php
require_file fixtures/runtime/invalid/functions/return-type-error.php
require_file fixtures/runtime/valid/functions/by-ref-capture.php
require_file fixtures/runtime/invalid/objects/unknown-class.php
require_file fixtures/runtime/invalid/php85/pipe-not-callable.php
require_file fixtures/runtime/valid/generators/yield.php
require_file fixtures/runtime/valid/generators/yield-from.php
require_file fixtures/runtime/valid/fibers/fiber.php
require_file fixtures/runtime/valid/eval/eval.php
require_file fixtures/runtime/valid/traits/trait-use.php
require_file fixtures/runtime/valid/enums/unit-enum.php
require_file fixtures/runtime/valid/property_hooks/get-hook.php
require_file fixtures/runtime/known_gaps/variables/undefined.php
require_file fixtures/runtime/known_gaps/functions/dynamic-call.php
require_file fixtures/runtime/known_gaps/foreach/by-ref.php
require_file fixtures/runtime/valid/references/by-ref-return.php
require_file fixtures/runtime/valid/superglobals/globals-alias.php
require_file fixtures/phpt_smoke/README.md
require_file fixtures/phpt_smoke/hello.phpt
require_file fixtures/phpt_smoke/variables.phpt
require_file fixtures/phpt_smoke/function.phpt
require_file fixtures/phpt_smoke/array.phpt
require_file fixtures/phpt_smoke/exception.phpt
require_file fixtures/phpt_smoke/skipif-skipped.phpt
require_file fixtures/phpt_smoke/ini-known-gap.phpt
require_file docs/runtime/contract.md

require_grep 'FetchDim' docs/runtime/ir.md
require_grep 'AssignDim' docs/runtime/ir.md
require_grep 'ReferenceCell' docs/runtime/values.md
require_grep 'Include Execution' docs/runtime/vm.md
require_grep 'normalize_runtime_stderr' docs/runtime/reference-diff.md
require_grep 'runtime-semantics' docs/runtime/known-gaps.md
require_grep 'Feature Matrix' docs/runtime/semantics-status.md
require_grep 'Top 20 Reference Deviations' docs/runtime/semantics-status.md
require_grep 'Runtime/VM Hardening Audit' docs/runtime/semantics-status.md
require_grep 'Runtime Semantics Deferred Scope' docs/runtime/semantics-status.md
require_grep 'Zend Opcode Mapping' docs/research/zend-opcode-mapping-runtime.md
require_grep 'does not claim Zend bytecode compatibility' docs/research/zend-opcode-mapping-runtime.md
require_grep 'Embedding and WASI Spike' docs/research/embedding-spike.md
require_grep 'bench-vm-smoke' justfile
require_grep 'fuzz-vm-smoke' justfile
require_grep 'php-vm report <file> \[--format markdown\|html\]' crates/php_vm_cli/src/main.rs
require_grep 'Known-Gap Status' crates/php_vm_cli/src/main.rs

just verify-frontend
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p php_ir
cargo test -p php_ir control_flow
cargo test -p php_ir functions
cargo test -p php_ir lower
cargo test -p php_ir literals
cargo test -p php_runtime
cargo test -p php_runtime value
cargo test -p php_runtime array
cargo test -p php_runtime convert
cargo test -p php_runtime builtins
cargo test -p php_runtime errors
cargo test -p php_runtime reference
cargo test -p php_runtime context
cargo test -p php_testkit runtime_fixture
cargo check -p php_testkit --bin compare-runtime
cargo test -p php_testkit phpt
cargo check -p php_testkit --bin run-phpt-smoke
cargo test -p php_vm
cargo test -p php_vm vm_core
cargo test -p php_vm expressions
cargo test -p php_vm control_flow
cargo test -p php_vm functions
cargo test -p php_vm function_params
cargo test -p php_vm closures
cargo test -p php_vm pipe
cargo test -p php_vm arrays
cargo test -p php_vm foreach
cargo test -p php_vm references
cargo test -p php_vm constants
cargo test -p php_vm builtins
cargo test -p php_vm runtime_errors
cargo test -p php_vm trace
cargo test -p php_vm_cli
cargo test -p php_vm_cli args
cargo test -p php_vm_cli trace
just vm-smoke
just vm-trace-smoke
just runtime-reference-smoke
just runtime-fixtures
just runtime-corpus-smoke
just phpt-smoke
just runtime-known-gaps
just bytecode-snapshots

printf '%s\n' '[info] runtime-diff remains reference-gated outside verify-runtime'
printf '%s\n' '[pass] runtime verification complete'
