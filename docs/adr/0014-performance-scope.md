# ADR 0014: Performance Scope

## Status

Accepted.

## Context

Performance follows the Foundation through Standard library PHP 8.5.7 Rust engine work. The
engine already has lexer, parser, typed AST views, semantic lowering, IR,
interpreter VM, runtime values, references, COW, object features, generators,
fibers, reflection MVPs, include/autoload basics, and a standard-library
surface.

The Performance goal is to add performance infrastructure and conservative
optimizations without changing visible PHP behavior. Performance work must be
measured, disableable, verifiable, and reversible.

Reference material for this layer:

- `php-src/php-8.5.7/Zend/zend_vm_def.h`
- `php-src/php-8.5.7/ext/opcache/`
- `php-src/php-8.5.7/ext/opcache/jit/README.md`
- PEP 659: https://peps.python.org/pep-0659/
- Cranelift: https://cranelift.dev/
- Criterion.rs: https://bheisler.github.io/criterion.rs/book/
- iai-callgrind: https://docs.rs/iai-callgrind

## Decision

Performance introduces these layers, in order:

1. Measurement and benchmark infrastructure.
2. Bytecode/IR cache.
3. Optimizer pass framework.
4. Quickening.
5. Inline caches.
6. Runtime fast paths.
7. Experimental JIT behind feature flags.

Each layer must preserve the semantic baseline. The default baseline is
`--opt-level=0` with quickening, inline caches, bytecode cache, and JIT disabled
where those switches exist. Optimized modes must produce A/B-identical output,
exit status, diagnostics, exceptions, warnings, notices, deprecations, and
timing-independent side effects.

Guarded fast paths must fall back to the generic path on guard failure. Cache and
inline-cache entries must be invalidated or ignored when their assumptions are
not provably current. JIT code is experimental, default-off, feature-gated, and
must never be the only execution strategy.

## Non-Goals

- No PHP syntax changes.
- No PHP 8.6 or nightly behavior.
- No rewrite of the lexer, parser, HIR, IR, VM, runtime, or standard library.
- No new standard-library or extension compatibility layer.
- No Zend C extension ABI compatibility.
- No production SAPI, FPM, Apache, CGI, or daemon lifecycle.
- No semantic deviation for speed.
- No architecture-specific assembly without a feature gate, ADR, tests, and
  fallback.
- No executable-memory JIT path without a documented W^X or equivalent safety
  model.
- No wall-clock-only CI gate.

## Correctness Rules

- `--opt-level=0` is the semantic engine baseline once optimization flags are
  introduced.
- Every optimization must be independently disableable.
- Every optimization must have an A/B comparison against the baseline for the
  fixtures it affects.
- Any miss, stale assumption, overflow, type mismatch, invalidation epoch
  mismatch, or unsupported construct must fall back to the generic path.
- Bytecode-cache artifacts are untrusted input and must be versioned,
  fingerprinted, verified, and safely ignored on corruption.
- Quickening and inline caches must track invalidation for functions, classes,
  methods, properties, include paths, autoload state, and relevant config.
- JIT execution remains experimental and feature-gated. Feature-off builds must
  not require executable memory.
- Known gaps must be recorded in `docs/performance/known-gaps.md` with evidence.

## Abort Criteria

An optimization path must stay disabled or be reverted if any of these happen:

- A/B output, diagnostics, or exit status diverge from the baseline.
- A guard cannot reliably fall back to the generic path.
- Invalidation cannot be proven for mutable PHP state.
- A cache artifact can panic, traverse paths unexpectedly, or execute stale code.
- A benchmark improvement depends on flaky wall-clock-only data.
- JIT safety requirements cannot be met on the target platform.
- The implementation requires broad rewrites outside the Performance layer.

## Consequences

Performance prioritizes measurable infrastructure and correctness evidence over
aggressive speedups. Some performance work may land as disabled infrastructure
until A/B tests, invalidation rules, and safety audits are strong enough to turn
it on. That is acceptable; correctness is the release condition.
