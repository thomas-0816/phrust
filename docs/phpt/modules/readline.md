# readline PHPT coverage

## Implemented slice

- Registers the PHP 8.5 `readline` function surface and `READLINE_LIB`
  constant in `php_std` and the runtime builtin registry.
- Keeps `readline()` nonblocking in noninteractive execution and returns
  `false` when no terminal input is available.
- Implements request-local history mutation and listing through
  `readline_add_history`, `readline_clear_history`, and
  `readline_list_history`.
- Implements deterministic history file read/write for explicit filenames.
- Implements `readline_info` get/set state and callback registration state for
  `readline_completion_function`, `readline_callback_handler_install`, and
  `readline_callback_handler_remove`.
- Implements `readline_callback_read_char`, `readline_redisplay`, and
  `readline_on_new_line` as nonblocking no-ops for CLI-safe tests.

## Known gaps

- There is no system readline/libedit binding yet, so interactive terminal
  input, real completion invocation, and callback dispatch on input remain open.
- History state is request-local rather than process-global readline state.
- `readline_read_history` and `readline_write_history` do not yet enforce
  `open_basedir` checks.
- The pinned php-src CLI used by this workspace does not load ext/readline, so
  the selected PHPT row may skip on the reference side while still exercising
  the Phrust target.

## Gates

- `nix develop -c cargo test -p php_runtime readline --no-fail-fast`
- `nix develop -c cargo test -p php_std readline --no-fail-fast`
- `REFERENCE_PHP=$REFERENCE_PHP PHP_SRC_DIR=$PHP_SRC_DIR PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=readline`
