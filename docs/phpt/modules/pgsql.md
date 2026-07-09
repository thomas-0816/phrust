# pgsql PHPT coverage

## Verified scope

- `pgsql` extension visibility.
- Generated `PgSql\Connection`, `PgSql\Result`, and `PgSql\Lob` class
  metadata.
- Core constants including `PGSQL_ASSOC`, `PGSQL_NUM`, `PGSQL_BOTH`, and
  `PGSQL_CONNECTION_OK`.
- Procedural function registration for connection, query, prepare, execute,
  fetch, row, field, affected-row, last-error, and escape helper APIs selected
  by the manifest.
- Opt-in live PostgreSQL smoke under `PHRUST_POSTGRES_TEST_DSN` when a DSN is
  configured.
- Deterministic skip behavior for the live DSN fixture when no PostgreSQL DSN
  is configured.

## Known gaps

- Persistent connections and connection pooling are not covered.
- Large objects, copy helpers, notifications, async polling, and tracing remain
  future work.
- Full libpq option, status, metadata, and server-version parity is not
  claimed.
- Default connection edge-case parity beyond the last successful connection is
  outside the selected manifest.
