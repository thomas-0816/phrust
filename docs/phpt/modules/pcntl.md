# pcntl PHPT coverage

## Implemented slice

- Registers the CLI `pcntl` function and constant surface needed by the selected
  compatibility fixture.
- Implements host-backed `fork`, `wait`, `waitpid`, `exec`, `alarm`,
  priority helpers, last-error helpers, and wait-status helpers through libc.
- Tracks `pcntl_async_signals`, `pcntl_signal`, and
  `pcntl_signal_get_handler` in request-local runtime state.
- Keeps the generated PHPT deterministic by forking before any observable
  output and only asserting parent-side wait/status behavior.

## Known gaps

- `pcntl_signal_dispatch` currently returns success after state validation; it
  does not execute PHP callbacks because VM callback dispatch integration is
  still missing.
- Async signal delivery, signal masks, `sigwaitinfo`, `sigtimedwait`, `waitid`,
  `rfork`/`forkx`, namespace/affinity/qos helpers, and full web/server-mode
  gating remain out of this slice.
- `pcntl_exec` is present for failure-path compatibility, but successful process
  replacement is not covered by the generated PHPT.
- The local php-src oracle CLI currently does not load `ext/pcntl`; reference
  promotion is therefore limited to target-side generated fixtures until a pcntl
  oracle is available.

## Gates

- `nix develop -c cargo test -p php_runtime pcntl --no-fail-fast`
- `nix develop -c cargo test -p php_std pcntl --no-fail-fast`
- `REFERENCE_PHP=$REFERENCE_PHP PHP_SRC_DIR=$PHP_SRC_DIR PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=pcntl`
