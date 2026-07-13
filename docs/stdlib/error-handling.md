# Standard library Error Handling

Reference target: PHP 8.5.7 (`php-8.5.7`).

The standard library implements the PHP error-handling MVP in the VM because the
functions depend on request-local handler stacks and INI state.

## Implemented

- `error_reporting()` reads and updates the request-local `error_reporting`
  INI value and returns the previous mask.
- `set_error_handler()` registers a normalized callable with an optional error
  level mask and returns the previous handler or `null`.
- `restore_error_handler()` pops the current error handler and returns `true`.
- `trigger_error()` and `user_error()` emit user warning, notice, and
  deprecation diagnostics through the runtime diagnostic model.
- Error handlers receive PHP's four callback arguments:
  `$errno`, `$errstr`, `$errfile`, and `$errline`.
- A handler returning `true` suppresses the default diagnostic path. A handler
  returning `false` falls through to the default diagnostic path.
- `display_errors=0` suppresses default visible warning output while preserving
  reported diagnostics.
- `E_USER_ERROR` remains fatal and is not recoverable through an error handler.
- `set_exception_handler()` and `restore_exception_handler()` maintain a
  request-local exception handler stack.
- Uncaught VM exceptions call the active exception handler before script
  termination.
- Direct calls to standard builtins that return Standard library TypeError or ValueError
  diagnostics are converted into catchable `TypeError` and `ValueError`
  throwables.

## Validation

- VM unit tests cover handler stack order, return behavior, reporting masks,
  display suppression, exception handler invocation, and fatal user errors.
- Differential fixtures cover simple PHP-compatible handler and mask behavior:
  - `STDLIB_ERROR_HANDLING`
  - `STDLIB_ERROR_REPORTING`
  - `STDLIB_EXCEPTION_HANDLER`
  - `STDLIB_TYPE_VALUE_ERRORS`
