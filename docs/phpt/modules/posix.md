# posix PHPT coverage

## Implemented slice

- Registers the PHP 8.5 `posix` function surface in `php_std` and the runtime builtin registry.
- Implements host-backed process and identity helpers: `posix_getpid`, `posix_getppid`,
  `posix_getuid`, `posix_geteuid`, `posix_getgid`, `posix_getegid`, `posix_getpgrp`,
  `posix_getpgid`, `posix_getsid`, `posix_getcwd`, and `posix_uname`.
- Implements selected process/session and signal probes: `posix_setsid` and
  `posix_kill` for host libc return/errno behavior.
- Implements passwd/group database lookups: `posix_getpwuid`, `posix_getpwnam`,
  `posix_getgrgid`, `posix_getgrnam`, `posix_getgroups`, and `posix_getlogin`.
- Implements file/system helpers for deterministic local cases: `posix_access`,
  `posix_eaccess`, `posix_mkfifo`, `posix_pathconf`, `posix_sysconf`, `posix_times`,
  `posix_ctermid`, `posix_getrlimit`, `posix_isatty`, `posix_ttyname`,
  `posix_get_last_error`, `posix_errno`, and `posix_strerror`.
- Exposes common access/path/sysconf/rlimit constants through platform libc values.

## Known gaps

- Resource fd pathconf and tty resource handling are not mapped to phrust stream
  resources yet; integer file descriptors are supported for `posix_isatty` and
  `posix_ttyname`.
- Signal callback delivery, identity mutation, process-group mutation, `posix_setrlimit`,
  and `mknod` are registered but currently return `false` and set `ENOSYS`.
- `posix_eaccess` shares the `access(2)` implementation for now.
- Error and warning text is bounded to stable return/errno behavior, not exact
  php-src warning strings.

## Gates

- `nix develop -c cargo test -p php_runtime posix --no-fail-fast`
- `nix develop -c cargo test -p php_std posix --no-fail-fast`
- `REFERENCE_PHP=$REFERENCE_PHP PHP_SRC_DIR=$PHP_SRC_DIR PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=posix`
