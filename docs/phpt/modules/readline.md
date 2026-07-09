# readline PHPT coverage

## Verified scope

- `readline` extension visibility and the `READLINE_LIB` constant.
- Noninteractive `readline()` calls return `false` without blocking CI.
- Request-local history mutation through `readline_add_history()`,
  `readline_list_history()`, and `readline_clear_history()`.
- History file round trips through `readline_write_history()` and
  `readline_read_history()`.
- `readline_read_history()` and `readline_write_history()` respect
  `open_basedir` before touching history paths.
- `readline_info()` get/set behavior, PHP-visible default keys, ordering, and
  value normalization for request-local line metadata.
- Completion callback registration with `readline_completion_function()`,
  including readline-specific invalid callback TypeError wording.
- Callback handler install/remove lifecycle, prompt output, plus safe no-op
  `readline_callback_read_char()`, `readline_redisplay()`, and
  `readline_on_new_line()`.
- Upstream php-src readline target sweep: 25 total, 16 pass, 8 skip, 1 fail.
  The only target failure is `ext/readline/tests/readline_basic.phpt`, which
  requires interactive terminal input.

## Known gaps

- No terminal-backed readline, libedit, or rustyline adapter is connected yet.
- Interactive line editing, prompt display, key handling, and terminal redisplay
  are intentionally out of scope for the selected noninteractive PHPT set.
- Completion callbacks are registered but are not invoked from terminal input.
- Callback line delivery is not implemented because no nonblocking terminal
  input loop exists yet.
- Host readline state and libedit-specific behavior remain future PHPT
  promotion work.
