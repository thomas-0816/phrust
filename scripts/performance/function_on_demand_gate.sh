#!/usr/bin/env bash
set -euo pipefail

# Structural P0.1 performance gate. These tests execute the production native
# compile coordinator and fail if one miss publishes foreign PHP bodies,
# dormant declarations emit code, failed compiles publish a cell, or
# concurrent callers compile the same key more than once.
cargo test -p php_jit --lib \
  compile_function_does_not_publish_same_unit_callee_body
cargo test -p php_vm --lib \
  loading_declaration_heavy_unit_compiles_only_entry_and_declares_other_cells
cargo test -p php_vm --lib \
  same_unit_call_resolves_on_demand_then_calls_native
cargo test -p php_vm --lib \
  vm::native_compile_cache::tests::concurrent_same_key_compiles_once
cargo test -p php_vm --lib \
  vm::native_compile_cache::tests::compile_breadth_violation_is_rejected_and_cached
cargo test -p php_server --lib \
  worker_pool::tests::serial_requests_reuse_the_warm_worker
cargo test -p php_server --lib \
  worker_pool::tests::concurrent_requests_still_use_distinct_workers

printf '%s\n' '[pass] function-on-demand compile and warm-worker gate passed'
