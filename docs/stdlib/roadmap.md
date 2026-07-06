# Standard Library Roadmap

Standard library work builds on the runtime semantics boundary and consumes the
existing frontend, HIR, IR, runtime, VM, fixture harnesses, and known-gap
catalog. It must not add a second lexer, parser, AST, semantic frontend, or
source-string execution path.

## Current Inputs

- Final Runtime semantics gate: `nix develop -c just verify-runtime`
- Reference-backed coverage snapshot: `docs/runtime/semantics-coverage-matrix.md`
- Known-gap catalog: `docs/runtime/semantics-known-gaps.md`
- Runtime contract: `docs/runtime/semantics-contract.md`
- Hardening audit: `docs/runtime/semantics-hardening.md`

## Standard Library Topics

| Topic | Concrete next work | Starting evidence |
| --- | --- | --- |
| Standard library | Add Tier 1 builtin coverage for framework boot paths: array helpers, string helpers, `count`, `is_*`, `class_exists`/`interface_exists` edge cases, and argument/type diagnostics. Keep every unsupported builtin behind a specific diagnostic. | `E_PHP_RUNTIME_UNSUPPORTED_STDLIB`, `E_PHP_RUNTIME_BUILTIN_ARITY`, `fixtures/runtime_semantics/real_world/*.php` |
| SPL and Reflection expansion | Expand `Iterator`, `IteratorAggregate`, `ArrayAccess`, Reflection classes, ReflectionEnum APIs, callable reflection, constructor/new-instance paths, and attribute target/repetition enforcement. | `docs/runtime/semantics-reflection-attributes.md`, `fixtures/runtime_semantics/reflection/*.php`, `fixtures/runtime_semantics/foreach/arrayaccess-known-gap.php` |
| Streams | Introduce deterministic stream/file wrappers for local file reads, include path behavior, path normalization, and warning/fatal rendering. Do not implement network streams in the required gate. | `E_PHP_RUNTIME_UNSUPPORTED_STREAM_WRAPPER`, `fixtures/runtime_semantics/include_eval_autoload/*.php` |
| JSON, PCRE, Date | Add small but real extension-like surfaces for `json_encode`/`json_decode`, `preg_*` basics, and DateTime construction/formatting because Composer/framework smokes commonly need them. Keep extension breadth explicit. | `E_PHP_RUNTIME_UNSUPPORTED_STDLIB`, Composer-style known gaps |
| Composer smokes | Add local, offline Composer-subset fixtures that are checked into `fixtures/stdlib/` or generated deterministically. Keep user-provided Composer projects opt-in and out of required CI. | `just local-composer-smoke <paths>`, `just runtime-composer-smoke`, `E_PHP_RUNTIME_COMPOSER_AUTOLOAD_MATRIX`, `E_PHP_RUNTIME_COMPOSER_STDLIB_MATRIX` |
| Performance Tier 1 | Add stable microbenchmarks for parse-to-run, function calls, array append/read, property access, method dispatch, generator resume, fiber suspend/resume, and Reflection metadata reads. Treat them as trend evidence, not compatibility gates. | `just runtime-bench-smoke`, `docs/runtime/semantics-generators-fibers.md`, `docs/runtime/semantics-array-semantics.md`, `docs/runtime/semantics-object-semantics.md` |
| Fuzz/property expansion | Promote the optional deterministic reference/COW/foreach fuzz smoke into a larger property suite with minimization, corpus promotion rules, and stable seeds per bug class. | `just runtime-fuzz-smoke`, `scripts/minimize_runtime_failure.py`, `fixtures/runtime_semantics/regressions/` |
| Bytecode cache | Define a versioned cache format for lowered IR/bytecode plus invalidation on source hash, PHP target version, feature flags, and semantic metadata version. | `php_ir`, `php_vm_cli`, Runtime IR docs |
| Extension API | Define a minimal Rust-native internal extension boundary for builtins and predefined classes before considering Zend ABI compatibility. Zend ABI emulation remains out of scope unless a later layer explicitly accepts it. | `E_PHP_RUNTIME_UNSUPPORTED_ZEND_ABI`, `crates/php_runtime/src/builtins/mod.rs`, `crates/php_runtime/src/builtins/modules/` |

## Verification Shape

Standard library validation should preserve the earlier layer gates:

```bash
nix develop -c just verify-foundation
nix develop -c just verify-lexer
nix develop -c just verify-frontend
nix develop -c just verify-frontend
nix develop -c just verify-runtime
nix develop -c just verify-runtime
```

The `verify-stdlib` target runs deterministic standard-library fixtures and
preserves clear skip behavior for reference-dependent checks when
`REFERENCE_PHP` is unavailable.

## Rules for Closing Gaps

- Move a known gap to implemented only when it has fixture evidence and passes
  against `REFERENCE_PHP` when reference behavior is observable.
- Split broad `UNSUPPORTED` IDs before adding executable paths beneath them.
- Keep generated reports under `target/`; do not commit reference output or
  vendored Composer/php-src trees.
- Preserve byte-based spans and structured diagnostics even when adding more
  PHP-compatible user-facing error text.
