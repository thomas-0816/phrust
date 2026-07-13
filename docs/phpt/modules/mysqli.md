# mysqli PHPT coverage

## Verified scope

- `mysqli` extension visibility.
- Core WordPress-facing class visibility for `mysqli`, `mysqli_result`, and
  `mysqli_stmt`.
- Procedural function visibility for connection, query, prepare, mysqlnd-style
  stats/options, ping, transaction, and multi-result APIs used by the selected
  fixtures.
- Common constants such as `MYSQLI_ASSOC`.
- Opt-in SQLite compatibility adapter under `PHRUST_MYSQLI_SQLITE_COMPAT=1` for
  deterministic query, fetch, insert id, affected rows, object properties,
  mysqlnd-shaped stats, transaction, multi-result, and error-state flow.
- Generated WordPress DB-network prepared-statement fixtures selected by the
  module manifest.
- Selected prepared-statement bind-param, bind-result, get-result, and
  result-metadata behavior.
- Client info/version platform checks selected from php-src.

## Current gate

- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=mysqli`
- Target: PASS 6, SKIP 4.
- Reference: SKIP 10 on this host.
- Source integrity: verified 24468 php-src manifest entries; skipped 7
  host-generated entries.

## Known gaps

- Full mysqli PHPT parity is not claimed.
- The deterministic SQLite compatibility adapter is not MySQL wire-protocol,
  mysqlnd, or libmysql parity.
- Live MySQL/MariaDB behavior remains gated behind explicit DSN-driven tests.
- `p:` persistent host syntax fails closed because connection pooling is not
  implemented.
- Prepared-statement coverage is limited to selected fixtures and does not
  cover the full upstream statement corpus.
- Advanced mysqlnd behavior, warnings, charset negotiation edge cases,
  metadata edge cases, and exact server error strings remain future promotion
  work.
