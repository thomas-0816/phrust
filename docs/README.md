# Phrust Documentation

Phrust is a Rust implementation of a PHP 8.5-compatible engine. It currently
provides a developer CLI, an integrated HTTP server, PHPT compatibility tooling,
and the engine layers needed to compare behavior with a pinned PHP reference.

Phrust is useful for engine development and compatibility work. It is not a
drop-in production PHP runtime, Zend extension ABI, FPM replacement, Opcache
replacement, or production JIT.

## Users

Start here to run PHP code, try the server, or understand current compatibility.

- [Getting started](getting-started.md)
- [CLI usage](cli.md)
- [Web server](web-server.md)
- [Compatibility](compatibility.md)
- [Switching from PHP](user/switching-from-php.md)
- [PHP user interface matrix](user/php-user-interface-matrix.md)

## Contributors

Start here when changing the engine, tests, compatibility fixtures, or docs.

- [Contributor index](contributor/README.md)
- [Contributor guide](contributing.md)
- [Validate a change](how-to/validate-a-change.md)
- [Work with PHPT](how-to/work-with-phpt.md)
- [WordPress smoke workflow](contributor/wordpress-smoke.md)

## Architecture

These pages explain the implementation structure and accepted boundaries.

- [Architecture index](architecture/README.md)
- [ADRs](adr/README.md)
- [Lexer architecture](lexer/lexer-architecture.md)
- [Parser architecture](parser/parser-architecture.md)
- [Semantic frontend architecture](frontend/semantic-frontend-architecture.md)
- [Runtime and VM](runtime/README.md)
- [Standard library](stdlib/README.md)
- [Server architecture](server-architecture.md)
- [Performance](performance/README.md)

## Reference

Use these documents for current status, compatibility contracts, and machine
policy summaries.

- [Reference index](reference/README.md)
- [PHP compatibility target](foundation/compatibility-target.md)
- [Known-gap manifests](known_gaps/README.md)
- [PHPT status](reference/phpt-status.md)
- [Performance status](reference/performance-status.md)
- [Runtime known gaps](runtime/known-gaps.md)
- [Standard library known gaps](stdlib/known-gaps.md)
- [Server known gaps](server-known-gaps.md)
- [API facades](api-facades.md)

## Documentation Policy

Human-facing docs describe stable behavior, accepted architecture, current
compatibility posture, and reproducible workflows. Local benchmark outputs,
profiler captures, PHPT run reports, and generated JSON/JSONL reports belong
under `target/` unless a tool explicitly consumes a committed manifest.

`research/` contains exploratory notes and implementation options. Research
docs are not accepted project contracts unless an ADR or owning architecture
page adopts them.
