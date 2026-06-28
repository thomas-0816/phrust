# sqlite3

- Strategy: platform-negative classification
- Classification: real-implementation-required before enabling
- Selected manifest: `tests/phpt/manifests/modules/sqlite3.selected.jsonl`
- Current corpus snapshot: 96 `sqlite3` candidates, 0 PASS, 7 SKIP, 89 FAIL,
  0 BORK, and 96 known non-green outcomes.

## Decision

Do not implement an in-memory SQLite MVP in this branch.

The `SQLite3` extension is smaller than PDO, but even the minimal useful
surface requires a real SQLite dependency, object lifetime, result objects,
parameter binding, error codes, and deterministic storage semantics. This branch
should classify first; enabling `SQLite3` without that support would make
framework probes believe database behavior exists.

## Unsupported Area

- Stable ID: `PHPT-DATA-SQLITE3`
- Reference behavior: PHP with `sqlite3` enabled exposes `SQLite3`,
  `SQLite3Stmt`, `SQLite3Result`, constants, in-memory/file databases, queries,
  prepared statements, errors, callbacks, and BLOB handling.
- Current phrust behavior: `extension_loaded("sqlite3")` and
  `class_exists("SQLite3")` are false.
- Fixture: `tests/phpt/generated/sqlite3/platform-checks.phpt`
- Next owner layer: future database extension layer with an approved SQLite
  dependency and real query execution.

## Source References

- `ext/sqlite3/sqlite3.stub.php`
- `ext/sqlite3/tests/`

## Target Gates

- `nix develop -c just phpt-dev-module MODULE=sqlite3`
- `nix develop -c just verify-phpt`
