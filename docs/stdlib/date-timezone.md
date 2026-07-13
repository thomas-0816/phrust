# Standard library Date/Time MVP

Reference target: PHP 8.5.7 (`php-8.5.7`).

Date/Time starts with a Rust-owned abstraction rather
than timelib FFI. The runtime exposes a deterministic timezone registry,
request-local default timezone state, core date functions, and DateTime-like
runtime object helpers through:

- `date_default_timezone_get`
- `date_default_timezone_set`
- `timezone_identifiers_list`
- `timezone_open`
- `timezone_name_get`
- `date`
- `gmdate`
- `time`
- `microtime`
- `strtotime`
- `date_format`
- `date_interval_format`
- `DateTimeInterface`, `DateTime`, `DateTimeImmutable`, `DateTimeZone`, and
  `DateInterval` metadata in `php_std`

The initial registry intentionally covers `UTC`, `Europe/Berlin`, and a small
set of common package-facing identifiers. It does not read host `TZ`, platform
timezone databases, or locale state.

The DateTime helper layer stores timestamps and timezone identifiers as runtime
object properties and covers constructor-style creation, `format`,
`getTimestamp`, `getTimezone`, `setTimestamp`, `setTimezone`, `modify`, `add`,
`sub`, and `diff` MVP behavior through VM method dispatch and internal helper
functions. Mutable helpers update `DateTime` in place; immutable helpers return
a new `DateTimeImmutable` object. `DateInterval` stores an MVP signed second
delta plus basic public interval fields.

`strtotime` accepts PHP timestamp notation such as `@1700000000`, ISO-like
absolute strings such as `2024-01-02 03:04:05`, and restricted relative
modifiers such as `+2 days`. Unsupported natural-language forms return
deterministic failure instead of guessing.

## Strategy

The standard-library scope permits narrow PCRE2/tzdata-style dependencies. This scope keeps the
Date/Time boundary dependency-free and leaves room for a later tzdb crate or
timelib FFI behind the same runtime abstraction.

## Known Gaps

The following gaps are tracked in `docs/stdlib/known-gaps.md`:

- `STDLIB-GAP-DATE-TIMELIB-PARITY`
- `STDLIB-GAP-DATETIME-FULL-API`
- `STDLIB-GAP-DATETIME-TZDB-DST`
