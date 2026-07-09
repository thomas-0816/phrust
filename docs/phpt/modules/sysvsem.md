# sysvsem PHPT coverage

Current focused coverage:

- `sysvsem` extension visibility, `SysvSemaphore` class visibility, and
  function registration.
- SysV-backed semaphore handles from `sem_get()` on Unix hosts.
- `sem_acquire()` and `sem_release()` limit behavior.
- Nonblocking acquisition failure when the semaphore is already acquired.
- `sem_remove()` removal semantics and failed acquisition after removal.
- Upstream `ext/sysvsem/tests/sysv.phpt` single-request semaphore/shared-memory
  integration using standard `ftok()` keys.
- Upstream `ext/sysvsem/tests/nowait.phpt` forked nonblocking acquire behavior
  and semaphore handoff ordering through `pcntl_fork()`.

This slice uses host System V semaphores on Unix and keeps a deterministic
fallback for non-Unix builds. Forked PHPT coverage is bounded to the upstream
sysvsem handoff row so CI does not introduce broader process-control behavior
than the selected prompt requires.

Measured target coverage:

- Selected rows: 3
- Passing rows: 3
- Known failures: 0
- Full upstream target sweep: 2 PASS / 0 FAIL.

Focused gate:

```bash
PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=sysvsem
```
