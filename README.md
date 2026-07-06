# phrust

<p align="center">
  <img src="assets/elephrant-black.png" alt="phrust elephrant logo" width="180">
</p>

`phrust` is an experiment in recreating an existing program, PHP, by extracting
specifications from the original documentation and tests and then implementing those
specifications with enough initial structure, validation gates, and feedback
loops to steadily close the remaining gaps.

The workspace targets a PHP 8.5-compatible engine. It currently contains the
lexer, lossless parser/CST, typed AST views, semantic frontend, HIR, IR,
runtime, VM, developer CLI, PHPT tooling, and validation gates used to compare
behavior against a pinned PHP reference.

The project is not a Zend ABI implementation and does not provide a production
SAPI, extension ABI, Opcache replacement, or production JIT.

## Reference Target

- PHP series: `8.5`
- PHP version: `8.5.7`
- Git tag: `php-8.5.7`
- Repository: `https://github.com/php/php-src.git`

The reference version is fixed by ADR. Do not advance it without a new ADR.

## Quickstart

Install Nix with Flake support, then run commands through the development
shell:

```bash
nix develop
just help
```

Common local checks:

```bash
nix develop -c just fmt
nix develop -c just source-integrity
PHP_SRC_DIR=/path/to/php-src nix develop -c just verify-generated-arginfo
nix develop -c just quality-fast
nix develop -c cargo clippy --workspace --all-targets -- -D warnings
nix develop -c cargo test --workspace
nix develop -c just verify-frontend
nix develop -c just verify-runtime
nix develop -c just verify-stdlib
nix develop -c just verify-server
nix develop -c just verify-performance
nix develop -c just verify-phpt
```

The narrowest relevant gate should be used while iterating. Run the broader
domain gate before handing off a change that affects that layer.

## Documentation

Use [docs/README.md](docs/README.md) as the documentation front door.

Primary guides:

- [Getting started](docs/getting-started.md)
- [CLI usage](docs/cli.md)
- [Web server](docs/web-server.md)
- [Compatibility](docs/compatibility.md)
- [PHP user interface matrix](docs/user/php-user-interface-matrix.md)
- [Switching from PHP](docs/user/switching-from-php.md)
- [Contributor guide](docs/contributing.md)

Install the versioned git hooks once per checkout:

```bash
nix develop -c just install-hooks
```

The pre-commit hook runs formatting, clippy, and the PHPT consistency gate. The
pre-push hook runs `just ci-local`, which mirrors the default GitHub Actions
checks without the manual full-PHPT regression job. `PHRUST_SKIP_GIT_HOOKS=1`
is available only for exceptional cases where the equivalent checks have been
run manually.

## Running PHP Code

Use `phrust-php` as the PHP-compatible front door:

```bash
nix develop -c cargo run -p php_vm_cli --bin phrust-php -- path/to/file.php
nix develop -c cargo run -p php_vm_cli --bin phrust-php -- -r 'echo PHP_SAPI, "\n";'
```

For a local `php` shim backed by `phrust-php`:

```bash
nix develop -c just install-user-bin
export PATH="$PWD/target/phrust/bin:$PATH"
php -v
```

`php-vm` remains available for lower-level VM debugging and bytecode-oriented
developer workflows.

## Running the integrated web server

Run the PHP-compatible built-in server front door:

```bash
nix develop -c cargo run -p php_vm_cli --bin phrust-php -- -S 127.0.0.1:8080 -t fixtures/server/apps/basic/public
```

`phrust-php -S` executes PHP through the workspace frontend, runtime, and VM in
the server process. It does not use FPM, FastCGI, CGI, Apache, `mod_php`, or an
external PHP process fallback. The advanced server binary remains available as
`phrust-server` for explicit server diagnostics and non-PHP-compatible flags.

Static file reads, route filesystem metadata checks, and PHP execution stay on
the integrated server path. `--request-timeout-ms` bounds request body reads;
PHP execution is bounded by the cooperative `--max-execution-ms` deadline and
the server in-flight request limit. The default in-flight request limit is 200;
requests above that limit wait briefly for capacity before receiving `503
Service Unavailable`.

The server uses the process-local compiled-script cache described in
`docs/runtime/cache-architecture.md`; the CLI bytecode artifact cache remains a separate
disk cache for local execution and performance experiments.

Runtime and VM public imports should use the facades documented in
`docs/api-facades.md`: `php_runtime::api` / `php_vm::api` for stable execution
surfaces and `php_runtime::experimental` / `php_vm::experimental` for local
instrumentation and experiments.

Useful server checks:

```bash
nix develop -c just server-smoke
nix develop -c just verify-user-interfaces
nix develop -c just server-compat-smoke all
nix develop -c just server-tls-smoke
nix develop -c just server-benchmark-smoke
```

Server configuration supports CLI flags and an optional simple TOML-style
`--config <path>` file. See `docs/server-functionality.md` for config,
access-log, metrics-token, cache, and TLS options.

## Repository Layout

```text
crates/php_source/       byte-oriented source maps and spans
crates/php_lexer/        PHP lexer/tokenization
crates/php_syntax/       parser and lossless CST
crates/php_ast/          typed views over CST nodes
crates/php_semantics/    semantic frontend, HIR, symbols, diagnostics
crates/php_ir/           bytecode/IR boundary
crates/php_runtime/      runtime values and builtins
crates/php_vm/           interpreter VM
crates/php_executor/     reusable in-process PHP execution API
crates/php_server/       integrated HTTP server
crates/php_vm_cli/       developer CLI for VM execution
crates/php_phpt_tools/   PHPT indexing, execution, and reporting tools
docs/                    architecture, ADRs, contracts, audits
references/              pinned reference metadata and lock files
tests/phpt/generated/    committed derived/minimized PHPT fixtures
tests/phpt/manifests/    committed PHPT indexes, selections, and baselines
third_party/             local-only reference checkout location
target/                  build output and generated run artifacts
```

## Validation Gates

The project is organized around functional gates over the engine pipeline:

```text
php_lexer -> php_syntax -> php_ast -> php_semantics/HIR -> php_ir -> php_runtime -> php_vm -> php_vm_cli
```

Core gates:

```bash
nix develop -c just source-integrity
nix develop -c just verify-frontend
nix develop -c just verify-runtime
nix develop -c just verify-stdlib
nix develop -c just verify-performance
nix develop -c just verify-phpt
```

Useful focused gates:

```bash
nix develop -c just lexer-fixtures
PHP_SRC_DIR=/path/to/php-src nix develop -c just verify-generated-arginfo
nix develop -c just parser-fixtures
nix develop -c just semantic-fixtures
nix develop -c just runtime-fixtures
nix develop -c just runtime-semantics-fixtures
nix develop -c just server-smoke
nix develop -c just perf-report
```

Reference-dependent checks skip clearly when no PHP reference binary is
available. If `REFERENCE_PHP` is set explicitly, the check must be strict and
fail when that binary is missing or unusable.

## PHPT Workflow

The pinned `php-src` checkout is read-only input. Original upstream PHPT files
must not be edited. Generated, minimized, or derived cases live under
`tests/phpt/generated/` with provenance in `tests/phpt/manifests/`.

Bootstrap the local reference checkout when a PHPT gate needs it:

```bash
nix develop -c just bootstrap-ref
```

PHPT commands:

```bash
nix develop -c just phpt-index
nix develop -c just phpt-module MODULE=operators.conversions
nix develop -c just phpt-fast MODULE=operators.conversions FILE=path/to/test.phpt
PHPT_RUN_FULL=1 nix develop -c just phpt-full-regression
nix develop -c just phpt-verify-source-integrity
```

Run artifacts belong under `target/phpt-work/` and must not be committed.
Generated local reports under `target/phpt-work/reports/` should be concise
summaries, not raw upstream output dumps, and should stay uncommitted.

## Continuous Integration

GitHub Actions are defined in `.github/workflows/ci.yml`.

The default CI path runs:

- Rust formatting.
- Fast source-integrity checks for required module wiring and generated
  metadata.
- The required fast quality gate: source integrity, known-gap manifests,
  dependency policy, unused dependency detection, all-features compile coverage,
  and rustdoc/doctests.
- Clippy over the workspace and all targets.
- `cargo test --workspace`.
- Domain verification gates for frontend, runtime, standard library, integrated
  server behavior, and performance behavior.
- A self-contained PHPT smoke no-regression run over committed generated
  fixtures. Current accepted target non-green outcomes are listed in
  `tests/phpt/manifests/runner-smoke-known-non-green.jsonl`; any new FAIL or
  BORK rejects CI.

The full PHPT regression gate is available as a manual workflow dispatch input
because it bootstraps the pinned reference checkout and is substantially heavier
than the pull-request smoke path.

Local CI parity:

```bash
nix develop -c just ci-local
```

## Documentation

Start with:

- [Target PHP ADR](docs/adr/0001-target-php-version.md)
- [Nix development ADR](docs/adr/0002-nix-dev-environment.md)
- [Reference oracle ADR](docs/adr/0003-reference-oracle.md)
- [No vendored php-src ADR](docs/adr/0004-no-vendored-php-src.md)
- [Layer boundary ADR](docs/adr/0005-layer-boundaries.md)
- [Executor architecture](docs/runtime/executor-architecture.md)
- [Server architecture](docs/server-architecture.md)
- [Diagnostics architecture](docs/runtime/diagnostics-architecture.md)
- [Cache architecture](docs/runtime/cache-architecture.md)
- [Known-gap manifests](docs/known_gaps/README.md)
- [Generated arginfo workflow](docs/stdlib/arginfo-coercion.md)
- [API facade policy](docs/api-facades.md)
- [Semantic frontend contract](docs/frontend/definition-of-done.md)
- [Runtime contract](docs/runtime/contract.md)
- [Runtime semantics contract](docs/runtime/semantics-contract.md)
- [Performance methodology](docs/performance/methodology.md)
- [Performance runtime](docs/performance/runtime.md)
- [Performance known gaps](docs/performance/known-gaps.md)
- [PHPT runtime completion](docs/phpt/README.md)

## Git Hygiene

Do not commit:

- local `php-src` checkouts;
- raw `target/` output;
- generated PHPT runner artifacts;
- local reference binaries;
- secrets or machine-specific environment files.

Stage only files intentionally changed for the current task, especially in a
dirty worktree.
