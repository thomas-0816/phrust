# shmop PHPT coverage

Current focused coverage:

- `shmop` extension visibility, `Shmop` class visibility, and function
  registration.
- Host System V shared-memory segments for `shmop_open()` modes used by the
  selected fixture, backed by `shmget()`, `shmat()`, `shmdt()`, and
  `shmctl(IPC_RMID)`.
- Binary-safe `shmop_write()` and `shmop_read()` including embedded NUL bytes.
- Read-only attach mode, `IPC_PRIVATE`-style key `0` isolation, `shmop_size()`,
  and `shmop_delete()` semantics.
- Portable upstream rows `ext/shmop/tests/001.phpt`,
  `ext/shmop/tests/002.phpt`, and
  `ext/shmop/tests/shmop_open_private.phpt`.

The selected fixture derives a unique SysV key with `tempnam()`/`ftok()` and
deletes the created segment after the read/write checks. Remaining
host/platform gaps are platform errno-specific warning text, exhaustive host
resource cleanup edge behavior, exact `shmop_close()` deprecation warning text,
and the two Windows-only upstream rows on non-Windows hosts.

Measured target coverage:

- Selected rows: 4
- Passing rows: 4
- Known failures: 0
- Full upstream target sweep: 3 PASS / 2 SKIP. The skipped rows are
  `ext/shmop/tests/bug81407.phpt` and `ext/shmop/tests/gh14537.phpt`, both
  Windows-only.

Focused gate:

```bash
PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=shmop
```
