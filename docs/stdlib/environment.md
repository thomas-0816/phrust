# Standard library Environment and Superglobals

Reference target: PHP 8.5.7 (`php-8.5.7`).

The standard library implements deterministic CLI environment and superglobal behavior
for offline Composer and framework fixture execution.

## Implemented

- `RuntimeContext` remains the only source of environment entries; host process
  environment variables are not imported implicitly.
- `php-vm run --env KEY=VALUE` injects controlled request environment entries.
- `$_ENV` is seeded from the controlled request environment.
- `$_SERVER` is seeded with deterministic CLI-safe keys: `argc`, `argv`,
  `PHP_SELF`, `SCRIPT_FILENAME`, `SCRIPT_NAME`, `DOCUMENT_ROOT`, and
  `REQUEST_TIME`.
- `getenv()` returns a stable array of request-local environment entries.
- `getenv($name)` returns a request-local value or `false`.
- `putenv($assignment)` mutates only request-local environment lookup for the
  current VM execution.
- `php_sapi_name()` returns `cli`.
- `php_uname()` and `get_current_user()` return deterministic non-host-leaking
  values.

## Security Contract

The VM intentionally does not read `std::env`, system hostname, OS release, or
login user data. Tests that need environment data must inject it through
`RuntimeContext::with_env` or `php-vm run --env KEY=VALUE`.

## Validation

- Runtime context unit tests cover deterministic `$_SERVER` and host-independent
  environment defaults.
- VM unit tests cover Composer-style env injection, `getenv`, `putenv`,
  `$_ENV`, `$_SERVER`, `php_sapi_name`, `php_uname`, and `get_current_user`.
- CLI unit tests cover `php-vm run --env` parsing.
- Differential fixture:
  - `STDLIB_ENVIRONMENT`
