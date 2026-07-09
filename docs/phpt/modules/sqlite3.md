# sqlite3 PHPT coverage

## Verified scope

- `sqlite3` extension visibility.
- Class visibility for `SQLite3`, `SQLite3Stmt`, `SQLite3Result`, and
  `SQLite3Exception`.
- Core SQLite3 constants for fetch modes, value types, open flags, and
  deterministic function flags.
- `SQLite3(":memory:")` and local file database construction.
- Basic `exec()`, `query()`, `querySingle()`, `close()`, `lastErrorCode()`, and
  `lastErrorMsg()` behavior.
- `SQLite3Result::numColumns()`, `fetchArray()`, `fetchAll()`, `reset()`, and
  `finalize()` for selected associative and numeric fetch modes.
- Prepared statements with selected positional and named `bindValue()` and
  `bindParam()` coverage.
- `lastInsertRowID()`, `changes()`, `busyTimeout()`, and `escapeString()`.

## Known gaps

- Callback APIs, custom SQL functions, collations, authorizers, and progress
  handlers are not covered by the selected fixtures.
- Backups, blob streams, loadable extensions, and advanced file-open edge cases
  remain future work.
- Full exception-mode parity and exact warning text are not yet covered across
  the upstream SQLite3 corpus.
- Platform-specific SQLite library behavior is limited to the deterministic
  local fixtures selected by the module manifest.
