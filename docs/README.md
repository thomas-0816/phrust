# Phrust Documentation

Phrust is a Rust implementation of a PHP 8.5-compatible engine. It currently
provides a developer CLI, an integrated HTTP server, PHPT compatibility tooling,
and the engine layers needed to compare behavior with a pinned PHP reference.

The project is useful for engine development and compatibility work. It is not
a drop-in production PHP runtime, Zend extension ABI, FPM replacement, Opcache
replacement, or production JIT.

## Use Phrust

Start here if you want to run PHP code or try the server.

- [Getting started](getting-started.md): install prerequisites, enter the dev
  shell, and run the first PHP file.
- [CLI usage](cli.md): run PHP scripts with `php-vm` and understand the current
  command surface.
- [Web server](web-server.md): start `phrust-server`, serve a document root,
  configure request limits, TLS, access logs, and cache behavior.
- [Compatibility](compatibility.md): current PHP target, supported surfaces,
  explicit non-goals, and where to find known gaps.

## Develop Phrust

Start here if you are changing the engine, tests, compatibility fixtures, or
documentation.

- [Contributor guide](contributing.md): repository workflow, validation gates,
  reference PHP setup, and where generated artifacts belong.
- [Validate a change](how-to/validate-a-change.md): choose the focused and
  aggregate gate for a change.
- [Work with PHPT](how-to/work-with-phpt.md): run module batches, debug
  failures, and keep php-src tests read-only.
- [Oracle workflow](oracle/README.md): turn php-src and reference PHP behavior
  into a prioritized API and runtime gap queue.
- [PHPT reference](phpt/README.md): details of the PHPT runner, manifests,
  generated tests, and full-regression workflow.

## Reference

Use these documents when you need exact contracts or current status.

- [PHP compatibility target](foundation/compatibility-target.md)
- [API facades](api-facades.md)
- [Server functionality](server-functionality.md)
- [Server architecture](server-architecture.md)
- [Known-gap manifests](known_gaps/README.md)
- [Runtime known gaps](runtime-known-gaps.md)
- [Standard library known gaps](stdlib-known-gaps.md)
- [Performance known gaps](performance-known-gaps.md)
- [PHPT known gaps](phpt/known-gaps.md)
- [PHP source callables reference](php-src-callables-reference.md)
- [PHP source oracle gap closure](php-source-oracle-gap-closure.md)

## Internals

These documents explain how the engine is structured. They are intended for
contributors who need to change the implementation.

- Source and syntax: [lexer architecture](lexer/lexer-architecture.md),
  [token model](lexer/token-model.md), [parser architecture](parser/parser-architecture.md),
  and [CST model](parser/cst-model.md).
- Frontend: [semantic frontend architecture](frontend/semantic-frontend-architecture.md),
  [HIR model](frontend/hir-model.md), and [declaration model](frontend/declaration-model.md).
- Runtime and VM: [runtime reference](runtime-reference.md),
  [runtime VM structure](runtime-vm-structure.md), [runtime values](runtime-values.md),
  and [runtime semantics status](runtime-semantics-status.md).
- Standard library: [standard library](stdlib-standard-library.md),
  [extension coverage](stdlib-extension-coverage.md), and
  [standard library roadmap](stdlib-roadmap.md).
- Performance: [performance methodology](performance-methodology.md),
  [performance runtime](performance-runtime.md),
  [optimization gates](performance-optimization-gates.md), and
  [bytecode cache](performance-bytecode-cache.md).
- Decisions and research: [ADRs](adr/) and [research notes](research/).

## Current Status

The most useful current-status pages are:

- [Compatibility](compatibility.md)
- [Runtime known gaps](runtime-known-gaps.md)
- [Standard library known gaps](stdlib-known-gaps.md)
- [Server known gaps](server-known-gaps.md)
- [PHPT known gaps](phpt/known-gaps.md)
