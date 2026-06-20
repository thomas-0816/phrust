# PHP Reference Metadata

This directory stores lockfiles and metadata for the pinned PHP reference.

The `php-src` source tree itself is not committed to this repository. A later
bootstrap script will create a local checkout at:

```text
third_party/php-src
```

Expected reference target:

- PHP series: `8.5`
- PHP version: `8.5.7`
- Git tag: `php-8.5.7`
- Repository: `https://github.com/php/php-src.git`

Future files in this directory may include:

- `php-src.lock.example.toml`
- `php-src.lock.toml`
- `php-src.metadata.json`
- `derived/tokens.json`
- `derived/parser-rules.json`
- `derived/scanner-states.json`
- `derived/syntax-features.json`

These files may record commit IDs, paths, hashes, sizes, line counts, and other
metadata needed to reproduce the reference target without vendoring `php-src`.

## Bootstrap

Create the local reference checkout and lockfile with:

```bash
nix develop -c just bootstrap-ref
```

Verify an existing checkout against the lockfile with:

```bash
nix develop -c just verify-ref
```

Extract deterministic metadata with:

```bash
nix develop -c just extract-ref-metadata
```

Dump the reference tokenizer constants with:

```bash
nix develop -c just dump-reference-tokens
```

Tokenize a fixture through the PHP reference oracle with:

```bash
nix develop -c just tokenize-ref tests/fixtures/lexer/example.php
```

Both oracle commands use `REFERENCE_PHP` when set, then
`third_party/php-src/sapi/cli/php` when present, then `php` from `PATH`.

The lockfile records:

- PHP series, version, tag, repository, and resolved commit.
- Local checkout path.
- Critical scanner, parser, VM, AST, compiler, and type files.

The metadata file records only paths, hashes, sizes, line counts, directory
summaries, and Git state. It does not copy PHP source code into `references/`.

See `docs/phase-0/license-and-copying-policy.md` for the Phase 0 policy on
license, provenance, and copying.

The `derived/` paths are reserved for generated artifacts. They are not
required for normal Phase 1 validation.
