# pgsql PHPT coverage

## Verified scope

- `pgsql` extension visibility.
- Generated `PgSql\Connection`, `PgSql\Result`, and `PgSql\Lob` class
  metadata.
- Core constants including `PGSQL_ASSOC`, `PGSQL_NUM`, `PGSQL_BOTH`, and
  `PGSQL_CONNECTION_OK`.
- Procedural function registration for connection, non-pooled `pg_pconnect`,
  query, query-params, prepare, execute, fetch, row, field, affected-row,
  last-error, result-error, and escape helper APIs selected by the manifest.
- Opt-in live PostgreSQL smoke under `PHRUST_POSTGRES_TEST_DSN` when a DSN is
  configured, including parameterized query execution.
- Deterministic skip behavior for the live DSN fixture when no PostgreSQL DSN
  is configured.

## Current gate

- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=pgsql`
  - Target: PASS 1, SKIP 1.
  - Reference: SKIP 2 on this host.
  - Source integrity verified 24468 php-src manifest entries; skipped 7
    host-generated entries.

## Known gaps

- `pg_pconnect` is implemented as a normal connection; persistent connection
  pooling is not claimed.
- Large objects, copy helpers, notifications, async polling, and tracing remain
  future work.
- Full libpq option, status, metadata, and server-version parity is not
  claimed.
- Default connection edge-case parity beyond the last successful connection is
  outside the selected manifest.
