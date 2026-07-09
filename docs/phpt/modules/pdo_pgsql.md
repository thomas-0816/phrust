# pdo_pgsql PHPT coverage

## Verified scope

- `pdo_pgsql` extension visibility.
- PostgreSQL driver discovery through `pdo_drivers()` and
  `PDO::getAvailableDrivers()`.
- Generated `Pdo\Pgsql` and `PDO_PGSql_Ext` class metadata.
- Opt-in live PostgreSQL PDO DSN smoke under `PHRUST_POSTGRES_TEST_DSN`,
  including query and prepared-statement flow when the environment variable is
  set.
- Deterministic skip behavior for the live DSN fixture when no PostgreSQL DSN
  is configured.

## Known gaps

- Unix socket DSNs are not covered by the selected fixtures.
- Persistent connections and server-version-specific attributes remain future
  work.
- Large objects, notifications, copy helpers, and full libpq parity are not
  claimed.
- PostgreSQL array, json, bytea, cursor, and metadata edge cases remain outside
  the current bounded manifest.
