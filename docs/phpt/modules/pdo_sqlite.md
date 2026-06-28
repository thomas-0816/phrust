# pdo_sqlite

- Strategy: platform-negative classification
- Classification: real-implementation-required before enabling
- Selected manifest: `tests/phpt/manifests/modules/pdo_sqlite.selected.jsonl`
- Current corpus snapshot: 80 `pdo_sqlite` candidates, 0 PASS, 6 SKIP, 73
  FAIL, 1 BORK, and 80 known non-green outcomes.

## Decision

Do not expose a PDO SQLite stub or in-memory MVP in this branch.

The safe minimum for `pdo_sqlite` is still real PDO object behavior, statements,
errors, transactions, and SQLite-backed query semantics. The current dependency
and runtime architecture do not provide that surface, and the prompt forbids
fake query success. Until PDO core and a real SQLite engine boundary exist, the
platform contract remains negative.

## Unsupported Area

- Stable ID: `PHPT-DATA-PDO-SQLITE`
- Reference behavior: PHP with `pdo_sqlite` enabled exposes the extension,
  SQLite PDO driver metadata, `sqliteCreateFunction`, `sqliteCreateAggregate`,
  `sqliteCreateCollation`, and real SQLite queries through PDO.
- Current phrust behavior: `extension_loaded("pdo_sqlite")` is false and PDO
  classes are unavailable.
- Fixture: `tests/phpt/generated/pdo_sqlite/platform-checks.phpt`
- Next owner layer: future database extension layer after PDO core exists.

## Source References

- `ext/pdo_sqlite/pdo_sqlite.stub.php`
- `ext/pdo_sqlite/sqlite_driver.stub.php`
- `ext/pdo_sqlite/tests/`

## Target Gates

- `nix develop -c just phpt-dev-module MODULE=pdo_sqlite`
- `nix develop -c just verify-phpt`
