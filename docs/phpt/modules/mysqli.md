# mysqli PHPT coverage

## Verified scope

- `mysqli` extension visibility.
- Core WordPress-facing class visibility for `mysqli`, `mysqli_result`, and
  `mysqli_stmt`.
- Procedural function visibility for connection, query, and prepare APIs used
  by the selected fixtures.
- Common constants such as `MYSQLI_ASSOC`.
- Opt-in SQLite compatibility adapter under `PHRUST_MYSQLI_SQLITE_COMPAT=1` for
  deterministic query, fetch, insert id, affected rows, object properties, and
  error-state flow.
- Generated WordPress DB-network prepared-statement fixtures selected by the
  module manifest.
- Client info/version platform checks selected from php-src.

## Known gaps

- Full mysqli PHPT parity is not claimed.
- The deterministic SQLite compatibility adapter is not MySQL wire-protocol,
  mysqlnd, or libmysql parity.
- Live MySQL/MariaDB behavior remains gated behind explicit DSN-driven tests.
- Prepared-statement coverage is limited to selected fixtures and does not yet
  cover the full upstream statement corpus.
- Advanced mysqlnd behavior, warnings, charset negotiation, multi-query,
  metadata edge cases, and exact server error strings remain future promotion
  work.
