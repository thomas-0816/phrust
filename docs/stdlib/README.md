# Standard Library Documentation

This directory owns standard-library contracts, extension coverage, generated
function coverage summaries, and standard-library known gaps.

## Fixture Areas

Differential fixtures live under `tests/fixtures/stdlib/`:

- `_harness/stdlib`: broad standard-library subset fixtures, including
  optional `hash`, `hash_hmac`, `random_bytes`, and `random_int`
  shape/range coverage.
- `_harness/streams`: resource, `php://memory`, and local filesystem path
  smoke fixtures.
- `_harness/json-pcre-date`: JSON, PCRE, and Date/Time extension smoke
  fixtures.
- `_harness/spl-reflection`: SPL iterator/container and Reflection smoke
  fixtures.
- `corpus`: Composer/framework-style regression snippets for autoload,
  environment, JSON config, routing, DateTime/version parsing, arrays,
  and reflection attributes.

The gate composition lives in the `justfile` (`just verify-stdlib` and the
focused `diff-*` recipes); prose copies of it are not maintained here.

## Stable Contracts

- [Standard library](standard-library.md)
- [Roadmap](roadmap.md)
- [Known gaps](known-gaps.md)
- [Argument info and coercion](arginfo-coercion.md)
- [ABI dispatch](abi-dispatch.md)

## Coverage And Reports

- [Extension coverage](extension-coverage.md)
- [Function coverage](function-coverage.md)
- [PHPT extension smoke](phpt-extension-smoke.md)
- [Regression corpus](regression-corpus.md)

## Implemented Areas

- [Array basics](array-basics.md)
- [String, hash, and URL encoding](encoding-hash-url.md)
- [Filesystem and path/stat functions](filesystem-path-stat.md)
- [JSON extension](json-extension.md)
- [PCRE extension](pcre-extension.md)
- [Date and timezone](date-timezone.md)
- [SPL basis](spl-basis.md)

## Additional References

- [Standard library Array Callback Functions](array-callbacks.md)
- [Standard library Array Sorting Functions](array-sorting.md)
- [Standard library Array Stack, Slice, and Merge Helpers](array-stack-merge.md)
- [Standard library Composer Compatibility](composer-compatibility.md)
- [Standard library Directory and Glob MVP](directory-glob.md)
- [Standard library Environment and Superglobals](environment.md)
- [Standard library Error Handling](error-handling.md)
- [Standard library Formatted Output Helpers](formatting.md)
- [Standard library INI Config MVP](ini-config.md)
- [Standard library Math and Numeric Functions](math-numeric.md)
- [Standard library Output Buffering](output-buffering.md)
- [Standard library Platform Constants](platform-constants.md)
- [Standard Library Preflight](preflight.md)
- [Standard library Security Capabilities](security-capabilities.md)
- [Standard library Serialization MVP](serialization.md)
- [Standard Library Stabilization](stabilization-06-54.md)
- [Standard library Stream Functions and Contexts MVP](stream-contexts.md)
- [Standard library Stream Resources](stream-resources.md)
- [Standard library Symbol Introspection](symbol-introspection.md)
