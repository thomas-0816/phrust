# pcntl PHPT coverage

Current focused coverage:

- `pcntl` extension visibility, selected function visibility, and common
  signal/wait/priority constants.
- CLI-only `pcntl_fork()` with immediate child exit and parent
  `pcntl_waitpid()` status collection.
- Exit status helpers including `pcntl_wifexited()`, `pcntl_wexitstatus()`,
  `pcntl_wifsignaled()`, and `pcntl_wtermsig()`.
- Request-local `pcntl_async_signals()` state, signal handler registration and
  lookup, `pcntl_signal_dispatch()`, `pcntl_alarm()`, and errno helpers.
- `PCNTL_ECHILD`, `PCNTL_EINVAL`, and `PCNTL_EINTR` errno aliases used by
  last-error checks.
- Darwin priority constants and validation for `pcntl_getpriority()` and
  `pcntl_setpriority()`.
- `pcntl_exec()` argv/env argument conversion, including object conversion
  errors and null-byte validation for arguments, environment names, and
  environment values.

The selected PHPT set contains 7 green rows: one generated local contract row
and 6 promoted upstream `ext/pcntl` rows. Process behavior remains isolated:
the child exits immediately and the parent waits for that exact PID. Broader
parity remains outside this slice: executing PHP signal callbacks during
dispatch, async signal delivery, successful `pcntl_exec()` process-replacement
PHPTs, signal masks, `waitid`, `forkx`, `rfork`, `unshare`, CPU affinity, QoS
helpers, and web/server-mode enablement.

Focused gate:

```bash
PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=pcntl
```
