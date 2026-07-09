# pdo_sqlite

- Strategy: SQLite-backed PDO MVP
- Classification: real implementation, still incomplete
- Selected manifest: `tests/phpt/manifests/modules/pdo_sqlite.selected.jsonl`
- Fixtures:
  - `tests/phpt/generated/pdo_sqlite/platform-checks.phpt`
  - `tests/phpt/generated/pdo_sqlite/prepared-transactions.phpt`
  - `tests/phpt/generated/pdo_sqlite/errmode-exception.phpt`

## Implemented Scope

enables `pdo_sqlite` by reusing the selected `rusqlite` connection
and result layer.

Implemented behavior:

- `extension_loaded("pdo_sqlite")` and PDO driver discovery with `sqlite`.
- SQLite `:memory:` and root-constrained local file DSNs.
- `PDO::exec`, `query`, and `prepare` with `PDOStatement::execute`.
- `PDOStatement::execute` with positional and named parameter arrays.
- `PDOStatement::bindValue` and selected `bindParam` value binding.
- `PDO::beginTransaction`, `commit`, `rollBack`, and `lastInsertId`.
- `PDOStatement::fetch`, `fetchAll`, `fetchColumn`, `columnCount`, and
  `closeCursor` for associative, numeric, both, column, and object fetch modes.
- Basic `errorCode` and `errorInfo` plumbing through the SQLite connection.
- Selected `PDO::ERRMODE_EXCEPTION` handling for SQLite query and statement
  execution failures with catchable `PDOException` objects.

## Remaining Gaps

- Stable ID: `PHPT-DATA-PDO-SQLITE-MVP-GAPS`
- SQLite-specific PDO callbacks (`sqliteCreateFunction`,
  `sqliteCreateAggregate`, and `sqliteCreateCollation`) are not implemented.
- Persistent connections, full attribute behavior, exact warning text,
  by-reference `bindParam` mutation parity, and advanced fetch modes remain
  incomplete.
- Non-SQLite PDO driver execution is covered by the owning PDO driver module
  gates; this module remains scoped to SQLite-specific execution behavior.

## Source References

- `ext/pdo_sqlite/pdo_sqlite.stub.php`
- `ext/pdo_sqlite/sqlite_driver.stub.php`
- `ext/pdo_sqlite/tests/`

## Target Gates

- `nix develop -c cargo test -p php_runtime`
- `nix develop -c cargo test -p php_vm`
- `nix develop -c just phpt-dev-module MODULE=pdo_sqlite`
