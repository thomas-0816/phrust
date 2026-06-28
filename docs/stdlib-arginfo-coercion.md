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

## Optional php-src Stub Generation

```bash
nix develop -c just generate-arginfo
```

The generator reads php-src `*.stub.php` declarations without executing C or
PHP code, applies deterministic manual overrides from
`fixtures/stdlib/arginfo_overrides.txt`, and writes a manually reviewable Rust
metadata file under `crates/php_std/src/generated/arginfo.rs` by default. The
`stdlib-docs` gate runs the same generator against a local fixture so the
parser, header, by-reference metadata, variadic metadata, and override path
stay covered without requiring a vendored php-src checkout.
