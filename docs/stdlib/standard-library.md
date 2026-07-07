# Standard library Standard Library Scope

Standard library targets PHP 8.5.7 (`php-8.5.7`) visible behavior for a deterministic,
offline standard-library subset. Work must run through the Nix workflow:

```bash
nix develop -c just verify-stdlib
```

## In Scope

- builtin-function ABI, arginfo, coercion, and diagnostics
- registry drift checks between runtime builtin registration and `php_std`
  function metadata
- core constants and extension/symbol introspection
- variable, type, string, array, math, config, error, output-buffering,
  environment, and superglobal functions
- streams, local filesystem wrappers, `php://` MVP, filesystem functions,
  directory and glob helpers, and stream contexts
- `json`, `pcre`, `date`, `spl`, `reflection`, and `tokenizer` MVP coverage
- Composer-local compatibility through offline fixtures, generated autoloaders,
  platform checks, and source-mode smokes

## Out Of Scope

Standard library does not implement JIT, Opcache, quickening, inline caches, Zend C
extension ABI compatibility, FPM, Apache SAPI, CGI production behavior, full
network/TLS/curl/openssl behavior, the full `mbstring`/`intl`/ICU/DOM/XML/PDO/
curl/session ecosystem, online Packagist integration, or unrestricted process
and shell functions. DOM/XML, PDO, curl, PHAR, mbstring, intl, and FPM are
explicitly bounded.

PHAR is not a required gate. Composer source mode is the required path. ADR 0013
keeps PHAR as a known gap unless a later optional read-only MVP is explicitly
accepted with archive, wrapper, stub, and diagnostic boundaries.

## Registry Drift

`nix develop -c just stdlib-registry-drift` compares
`crates/php_runtime/src/builtins/registry.rs` with `php_std` function metadata.
The generated report is written to `target/stdlib/registry-drift/`; only the
policy allowlist in `scripts/stdlib/registry_drift_allowlist.jsonl` is
committed. New runtime-only or metadata-only symbols must either be closed or
documented there with a reason.

## References

- Reference repository: `https://github.com/php/php-src.git`
- Reference tag: `php-8.5.7`
- Runtime behavior oracle: pinned `REFERENCE_PHP`
- Manual areas: Function Reference, Streams, SPL, Reflection, JSON, PCRE,
  Date/Time, Tokenizer, Composer platform requirements
