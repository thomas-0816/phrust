# Runtime semantics Coverage Matrix

This matrix is the final Runtime semantics differential coverage snapshot. It was
generated with the pinned PHP 8.5.7 reference binary:

```bash
nix develop -c env REFERENCE_PHP=third_party/php-src/sapi/cli/php just runtime-semantics-diff --out target/runtime-semantics/diff-reference
```

Result:

```text
total=366 pass=306 fail=0 skip=0 known_gap=60
```

When `REFERENCE_PHP` is not set, `just runtime-semantics-diff` skips pass-candidate
runtime comparisons and still reports known-gap fixtures. The required
`verify-runtime` gate is therefore strict when a reference binary is explicitly
provided and skip-explicit when the local reference binary is unavailable.

| Category | Pass | Fail | Known gap | Skip | Fixture root or examples |
| --- | ---: | ---: | ---: | ---: | --- |
| `arrays` | 15 | 0 | 1 | 0 | `fixtures/runtime_semantics/arrays/*.php` |
| `callables` | 24 | 0 | 1 | 0 | `fixtures/runtime_semantics/callables/*.php` |
| `clone_with` | 5 | 0 | 4 | 0 | `fixtures/runtime_semantics/clone_with/*.php` |
| `closures` | 12 | 0 | 0 | 0 | `fixtures/runtime_semantics/closures/*.php` |
| `comparisons` | 5 | 0 | 0 | 0 | `fixtures/runtime_semantics/comparisons/*.php` |
| `const_expr` | 0 | 0 | 4 | 0 | `fixtures/runtime_semantics/const_expr/*.php` |
| `conversions` | 4 | 0 | 3 | 0 | `fixtures/runtime_semantics/conversions/*.php` |
| `cow` | 2 | 0 | 0 | 0 | `fixtures/runtime_semantics/cow/*.php` |
| `destructors` | 4 | 0 | 1 | 0 | `fixtures/runtime_semantics/destructors/*.php` |
| `enums` | 11 | 0 | 2 | 0 | `fixtures/runtime_semantics/enums/*.php` |
| `errors` | 6 | 0 | 0 | 0 | `fixtures/runtime_semantics/errors/*.php` |
| `fibers` | 12 | 0 | 0 | 0 | `fixtures/runtime_semantics/fibers/*.php` |
| `foreach` | 16 | 0 | 2 | 0 | `fixtures/runtime_semantics/foreach/*.php` |
| `functions` | 14 | 0 | 3 | 0 | `fixtures/runtime_semantics/functions/*.php` |
| `gc` | 0 | 0 | 4 | 0 | `fixtures/runtime_semantics/gc/*.php` |
| `generators` | 15 | 0 | 1 | 0 | `fixtures/runtime_semantics/generators/*.php` |
| `globals` | 8 | 0 | 1 | 0 | `fixtures/runtime_semantics/globals/*.php` |
| `include_eval_autoload` | 16 | 0 | 2 | 0 | `fixtures/runtime_semantics/include_eval_autoload/*.php` |
| `known_gaps` | 0 | 0 | 12 | 0 | `fixtures/runtime_semantics/known_gaps/*.php` |
| `magic` | 12 | 0 | 2 | 0 | `fixtures/runtime_semantics/magic/*.php` |
| `objects` | 24 | 0 | 0 | 0 | `fixtures/runtime_semantics/objects/*.php` |
| `pipe` | 6 | 0 | 0 | 0 | `fixtures/runtime_semantics/pipe/*.php` |
| `properties` | 6 | 0 | 0 | 0 | `fixtures/runtime_semantics/properties/*.php` |
| `property_hooks` | 6 | 0 | 2 | 0 | `fixtures/runtime_semantics/property_hooks/*.php` |
| `real_world` | 1 | 0 | 2 | 0 | `fixtures/runtime_semantics/real_world/*.php` |
| `reflection` | 11 | 0 | 3 | 0 | `fixtures/runtime_semantics/reflection/*.php` |
| `refs` | 5 | 0 | 1 | 0 | `fixtures/runtime_semantics/refs/*.php` |
| `regressions` | 2 | 0 | 1 | 0 | `fixtures/runtime_semantics/regressions/**/*.php` |
| `statics` | 2 | 0 | 0 | 0 | `fixtures/runtime_semantics/statics/*.php` |
| `strings` | 2 | 0 | 1 | 0 | `fixtures/runtime_semantics/strings/*.php` |
| `superglobals` | 1 | 0 | 0 | 0 | `fixtures/runtime_semantics/superglobals/*.php` |
| `traits` | 9 | 0 | 0 | 0 | `fixtures/runtime_semantics/traits/*.php` |
| `types` | 8 | 0 | 6 | 0 | `fixtures/runtime_semantics/types/*.php` |
| `variables` | 2 | 0 | 0 | 0 | `fixtures/runtime_semantics/variables/*.php` |
| `void_cast` | 0 | 0 | 1 | 0 | `fixtures/runtime_semantics/void_cast/*.php` |
| `wordpress_blockers` | 7 | 0 | 0 | 0 | `fixtures/runtime_semantics/wordpress_blockers/*.php` |
| `wp_language_vm` | 17 | 0 | 0 | 0 | `fixtures/runtime_semantics/wp_language_vm/**/*.php` |
| `wp_autoload_stdlib` | 16 | 0 | 0 | 0 | `fixtures/runtime_semantics/wp_autoload_stdlib/**/*.php` |

## Known-Gap Summary

Every known-gap fixture in the reference-backed report declares a stable
`known_gap=<ID>` in the fixture metadata. The active known-gap groups are:

- Reference/property lvalue gaps: property references, static-property
  by-reference parameter aliases, array-element return references, and
  by-reference temporary foreach sources.
- PHP-exact diagnostics and warning channels: numeric-string warning output,
  include warning rendering, undefined-variable warnings, and fatal error text.
- Deferred runtime breadth: standard-library/SPL/Reflection expansion,
  serialization, `ArrayAccess`, public GC APIs, enum serialization, and
  Composer-style autoload/stdlib coverage.
- Deferred execution matrices: constant-expression runtime values, string
  offset COW writes, generator by-reference yields, clone-with restricted
  property rules, namespaced string callables, property-hook recursion/visibility
  edges, and destructor/GC cycle behavior.

## Unsupported ID Cleanup

The final audit replaced newly discovered generic pass-fixture mismatches with
specific IDs:

- `E_PHP_RUNTIME_NUMERIC_STRING_WARNING_CHANNEL`
- `E_PHP_RUNTIME_TYPEERROR_TEXT_COMPAT`
- `E_PHP_RUNTIME_UNINITIALIZED_PROPERTY_TEXT_COMPAT`
- `E_PHP_RUNTIME_UNION_TYPEERROR_TEXT_COMPAT`

Remaining broad IDs in `docs/runtime-semantics-known-gaps.md` are intentionally reserved
for whole Standard library+ capability areas that Runtime semantics does not execute, such as
standard-library breadth, stream wrappers, Zend ABI, SAPI, Opcache, and JIT.
