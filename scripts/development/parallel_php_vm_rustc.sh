#!/usr/bin/env bash
set -euo pipefail

if (( $# == 0 )); then
  printf '%s\n' '[parallel-rustc] missing rustc executable argument' >&2
  exit 2
fi

rustc=$1
shift

run_rustc() {
  if [[ -n "${PHRUST_RUSTC_CACHE_WRAPPER:-}" ]]; then
    exec "$PHRUST_RUSTC_CACHE_WRAPPER" "$rustc" "$@"
  fi
  exec "$rustc" "$@"
}

crate_name=''
previous=''
for argument in "$@"; do
  if [[ "$previous" == '--crate-name' ]]; then
    crate_name=$argument
    break
  fi
  previous=$argument
done

# Cargo already parallelizes independent crates. Near the end of the dependency
# graph, however, one Phrust workspace crate is often the only remaining job.
# Give every project crate a bounded frontend thread pool. Registry dependencies
# keep their original flags and remain cacheable through the optional inner
# wrapper.
if [[ "$crate_name" == php_* || "$crate_name" == 'phrust_server' ]]; then
  detected_threads=$(getconf _NPROCESSORS_ONLN 2>/dev/null || printf '1')
  default_threads=$detected_threads
  if [[ "$default_threads" =~ ^[1-9][0-9]*$ ]] && (( default_threads > 20 )); then
    default_threads=20
  fi
  threads=${PHRUST_RUSTC_THREADS:-$default_threads}
  if [[ ! "$threads" =~ ^[1-9][0-9]*$ ]]; then
    printf '%s\n' \
      "[parallel-rustc] PHRUST_RUSTC_THREADS must be a positive integer, got: $threads" >&2
    exit 2
  fi

  # The Nix shell pins the compiler version. Scope the unstable compiler
  # allowance to this workspace crate instead of enabling it for dependencies.
  if [[ -n "${PHRUST_RUSTC_CACHE_WRAPPER:-}" ]]; then
    exec env RUSTC_BOOTSTRAP="$crate_name" \
      "$PHRUST_RUSTC_CACHE_WRAPPER" "$rustc" "$@" "-Zthreads=$threads"
  fi
  exec env RUSTC_BOOTSTRAP="$crate_name" "$rustc" "$@" "-Zthreads=$threads"
fi

run_rustc "$@"
