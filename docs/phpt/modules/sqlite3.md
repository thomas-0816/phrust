# sqlite3

- Strategy: deterministic local MVP
- Classification: real implementation, still incomplete
- Selected manifest: `tests/phpt/manifests/modules/sqlite3.selected.jsonl`
- Fixture: `tests/phpt/generated/sqlite3/platform-checks.phpt`

## Implemented Scope

enables `sqlite3` by default and backs it with `rusqlite`.

Implemented behavior:

- `SQLite3`, `SQLite3Result`, `SQLite3Stmt`, and `SQLite3Exception` class
  visibility for framework probes.
- `:memory:` databases and root-constrained local file databases.
- `SQLite3::__construct`, `open`, `exec`, `query`, `querySingle`,
  `prepare`, `lastErrorCode`, `lastExtendedErrorCode`, `lastErrorMsg`,
  `lastInsertRowID`, `changes`, `busyTimeout`, `escapeString`, and `close`.
- `SQLite3Result::fetchArray`, `fetchAll`, `reset`, `finalize`, and
  `numColumns`.
- `SQLite3Stmt::bindValue`, selected `bindParam`, `execute`, `reset`, `clear`,
  and `close` for migration-style positional and named parameter flows.
- Common SQLite3 constants for fetch modes, value types, open flags, and
  deterministic functions.

The generated fixture covers in-memory query execution, result iteration,
successful error state, close behavior, and a local file database round trip.
The prepared/status fixture covers bound inserts, selected result execution,
`lastInsertRowID`, `changes`, `busyTimeout`, and `escapeString`.

## Remaining Gaps

- Stable ID: `PHPT-DATA-SQLITE3-MVP-GAPS`
- Exact PHP warning text and all error-code edge cases are not complete.
- Callbacks, custom SQL functions, collations, authorizers, backups, blob
  streams, loadable extensions, and exception-mode behavior are outside this
  MVP.
- The implementation intentionally does not provide network databases or PDO;
  PDO_SQLite owns SQLite-specific PDO integration.

## Source References

- `ext/sqlite3/sqlite3.stub.php`
- `ext/sqlite3/tests/`

## Target Gates

- `nix develop -c cargo test -p php_runtime`
- `nix develop -c cargo test -p php_vm`
- `nix develop -c just phpt-dev-module MODULE=sqlite3`
