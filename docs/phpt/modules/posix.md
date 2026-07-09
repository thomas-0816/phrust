# posix PHPT coverage

Current focused coverage:

- `posix` extension visibility and selected POSIX constants.
- Host-backed process, user, and group identifiers including `posix_getpid()`,
  `posix_getuid()`, `posix_getgid()`, and related ID helpers.
- Password and group database array shapes from `posix_getpwuid()`,
  `posix_getpwnam()`, `posix_getgrgid()`, and `posix_getgrnam()`,
  including Darwin `posix_getgrgid(-1)` `nogroup` parity.
- `posix_getgroups()`, `posix_getlogin()`, `posix_uname()`,
  `posix_access()`, `posix_eaccess()`, `posix_get_last_error()`,
  `posix_errno()`, and `posix_strerror()`.
- `posix_sysconf()`, `posix_pathconf()`, `posix_times()`,
  `posix_getrlimit()`, `posix_kill()` signal-0 probing, integer-fd
  `posix_isatty()`, and integer-fd `posix_ttyname()`.
- Upstream PHPT error rows for `posix_eaccess()` long filenames,
  `posix_getsid()` negative process IDs, `posix_kill()` invalid arguments,
  `posix_strerror()`, `posix_sysconf()`, `posix_pathconf()`, and denied
  `posix_setuid()` errno.
- Upstream host-backed PHPT rows for `posix_getrlimit()`, `posix_times()`,
  and `posix_uname()`.

The selected PHPT set contains 37 green rows: one generated local contract row
and 36 promoted upstream `ext/posix` rows. It only asserts stable shapes,
types, local file/process checks, and host data returned by libc; it does not
invent POSIX data. Remaining gaps are resource-fd pathconf/ttyname integration,
stream-to-host-fd mapping, successful identity mutation, process-group
mutation, `posix_setrlimit()`, `mknod()` device handling, and remaining
php-src warning strings.

Focused gate:

```bash
PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=posix
```
