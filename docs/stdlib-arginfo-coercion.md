# Standard library Arginfo and Coercion

Reference target: PHP 8.5.7 (`php-8.5.7`).

Standard library builtins use `php_std::arginfo::ArgumentValidator` for arity, type,
default-value, variadic, nullable, union-like, by-reference, and return metadata.
Function implementations must not duplicate missing-argument, too-many-argument,
or basic type-check logic.

## Coercion Modes

- `Strict`: values must already match the declared type atom, except nullable
  parameters accept `null`.
- `Weak`: scalar values may be coerced through the shared runtime conversion
  helpers for `bool`, `int`, `float`, and `string`.

The model stores PHP-style error class intent:

- `TypeError` for arity and type failures
- `ValueError` for valid types with invalid ranges or values

Unit tests in `php_std::arginfo` snapshot diagnostic IDs, messages, and source
spans for missing arguments, too many arguments, wrong types, weak coercion, and
ValueError construction. Standard library differential fixtures wire these into
reference-backed builtin tests as each function group is implemented.

## php-src Stub Generation

```bash
nix develop -c just generate-arginfo php_src=/path/to/php-src
```

The generator reads php-src `*.stub.php` declarations without executing C or
PHP code, applies deterministic manual overrides from
`fixtures/stdlib/arginfo_overrides.txt`, and writes a manually reviewable Rust
metadata file under `crates/php_std/src/generated/arginfo.rs` by default. The
`stdlib-docs` gate runs the same generator against a local fixture so the
parser, header, by-reference metadata, variadic metadata, and override path
stay covered without requiring a vendored php-src checkout.

`crates/php_std/src/generated/arginfo.rs` is committed and reviewable. Treat it
as a generated build input, not as an optional local artifact. After changing
the generator, overrides, or PHP reference target, regenerate the snapshot from
the pinned php-src checkout and review the diff before committing it.

## Drift Verification

Strict drift verification regenerates arginfo into `target/` and diffs it
against the committed snapshot:

```bash
PHP_SRC_DIR=/path/to/php-src nix develop -c just verify-generated-arginfo
```

The check fails clearly when no php-src checkout is available. Use the pinned
PHP 8.5.7 (`php-8.5.7`) tree unless an ADR updates the reference target. The
fast `source-integrity` gate below remains suitable for normal CI because it
does not require php-src; `verify-generated-arginfo` is the reference-backed
drift gate for generator or snapshot changes.

When the PHP reference version changes:

1. Update the reference target ADR/lockfile and this document together.
2. Regenerate with `nix develop -c just generate-arginfo php_src=/path/to/php-src`.
3. Run `PHP_SRC_DIR=/path/to/php-src nix develop -c just verify-generated-arginfo`.
4. Run `nix develop -c cargo test -p php_std` and the relevant stdlib gate.

## Source Integrity Policy

The committed generated arginfo snapshot is part of the build input. It must
remain non-empty, expose the generated metadata lookup functions consumed by
`php_std`, and contain stable core symbols used by runtime validation and
reflection smoke paths. The fast guard is:

```bash
nix develop -c just source-integrity
```

That check also verifies critical Rust module wiring such as `php_vm::vm`
declarations and re-exports. It is included in `just check` and CI so empty
module files or missing generated metadata fail before deeper workspace gates.
