# Test Matrix

Foundation prepares the compatibility test strategy for PHP `8.5.7`. It does not
implement a lexer, parser, VM, runtime, or framework runner.

## Foundation Checks

| Check | Required | Command |
| --- | --- | --- |
| Repository contract files exist | Yes | `nix develop -c just verify-foundation` |
| Rust placeholder workspace passes | Yes | `cargo fmt`, `cargo clippy`, `cargo test` |
| Rust quality tooling runs | Optional/deep | `nix develop -c just quality` |
| PHP reference lockfile verifies when present | Yes, if bootstrapped | `nix develop -c just verify-ref` |
| PHP reference metadata is extractable | Optional network/local reference | `nix develop -c just extract-ref-metadata` |
| Minimal reference PHP CLI builds | Optional expensive | `nix develop -c just build-ref-php` |

## Later Oracle Gates

| Gate | Scope | Reference oracle | Layer |
| --- | --- | --- | --- |
| L0 | Reference files pinned | `php-src` tag, commit, critical files | Foundation |
| L1 | Lexer token compatibility | `token_get_all($source, TOKEN_PARSE)` | Lexer |
| P1 | Parser accept/reject compatibility | `php -l` | Parser |
| P2 | Parse diagnostics | `php -l` error class, line, and position approximation | Parser |
| C1 | Compile-time errors | Reference CLI and `.phpt` expectations | Semantic frontend |
| R1 | Runtime behavior | `.phpt` tests | Runtime and later layers |
| F1 | Composer/framework smoke tests | Composer projects and framework CLIs | Later |

## Syntax Coverage Gates

| Gate | Requirement | Required in |
| --- | --- | --- |
| L0 | Reference files pinned and critical scanner/parser files present. | Foundation |
| L1 | `token_get_all()` fixtures generated from reference PHP. | Lexer |
| P1 | `php -l` accepts and rejects the same inputs. | Parser |
| P2 | Parse error class, line, and position approximated. | Parser |
| C1 | Compile-time errors match reference categories. | Semantic frontend |
| R1 | Runtime behavior matches selected `.phpt` tests. | Runtime and later layers |

## Required vs Optional

Required Foundation checks must be fast, deterministic, and independent of large
network downloads after the initial setup. Network and expensive checks are
separate:

- Required local: `nix develop -c just verify-foundation`
- Optional quality: `nix develop -c just quality`
- Optional network: `nix develop -c just bootstrap-ref`
- Optional metadata: `nix develop -c just extract-ref-metadata`
- Optional expensive: `nix develop -c just build-ref-php`

## CI Scope

| Scope | Required? | Command |
| --- | --- | --- |
| CI required | Yes | `nix flake show` |
| CI required | Yes | `nix develop -c just verify-foundation` |
| CI optional/manual | No | `nix develop -c just bootstrap-ref` |
| CI optional/manual | No | `nix develop -c just extract-ref-metadata` |
| CI optional/manual | No | `nix develop -c just build-ref-php` |

`macos-latest` is not required in Foundation CI. Darwin support is validated
locally through the Nix dev shell; adding a required macOS CI job is a later
cost/reliability decision.

## Lexer Oracle

Future lexer work must compare tokens against:

```php
token_get_all($source, TOKEN_PARSE)
```

The reference PHP CLI must have the tokenizer extension enabled for this gate.

Fixtures will live under `tests/fixtures/lexer` and use generated
`*.expected.tokens.json` files.

## Parser Oracle

Future parser work must compare acceptance and rejection behavior against:

```bash
php -l file.php
```

Exact messages may be staged by approximation level, but accept/reject parity is
the first requirement.

Fixtures will live under `tests/fixtures/parser` and use generated
`*.expected.parse.json` files.

## Runtime Oracle

Future runtime work must import or adapt `.phpt` tests from the pinned
reference. Runtime gates should be grouped by value model, arrays, objects,
errors, extensions, and CLI behavior.

Runtime fixtures will live under `tests/fixtures/runtime`. Imported or adapted
PHPT material will live under `tests/fixtures/phpt` with provenance preserved.

## Runtime Gates

| Gate | Scope | Oracle | Status |
| --- | --- | --- | --- |
| V1 | Value model tests for null, bool, int, float, string, array, object, resource, and references | Reference CLI and `.phpt` | Planned |
| A1 | Array tests for packed/list, mixed maps, key normalization, order, and mutation | Reference CLI and `.phpt` | Planned |
| O1 | Object model tests for classes, traits, interfaces, enums, attributes, property hooks, and visibility | `.phpt` | Planned |
| E1 | Error behavior tests for warnings, exceptions, parse/compile/runtime errors | `php -l`, CLI, `.phpt` | Planned |
| PHT1 | PHPT import and runner | Pinned `php-src` tests | Planned |

## Fixture Plan

| Category | Directory | Gate |
| --- | --- | --- |
| Lexer differential fixtures | `tests/fixtures/lexer` | L1 |
| Parser accept/reject fixtures | `tests/fixtures/parser` | P1/P2 |
| Runtime fixtures | `tests/fixtures/runtime` | R1/V1/A1/O1/E1 |
| Imported/adapted `.phpt` fixtures | `tests/fixtures/phpt` | PHT1 |

## Composer and Framework Smoke Tests

Later layers should add smoke tests for Composer-installed packages and common
framework entry points. These tests are not part of Foundation.
