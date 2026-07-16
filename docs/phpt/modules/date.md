# date

- Priority: 19
- Selected manifest: `tests/phpt/manifests/modules/date.selected.jsonl`
- Focused selected counts: 11 PASS, 0 SKIP, 0 FAIL, 0 BORK from 11 Date/Time
  fixtures
- Corpus triage counts: 14 PASS, 12 SKIP, 661 FAIL, 0 BORK from 687
  upstream `ext/date` corpus candidates before this focused promotion

## Scope

- request-local `date_default_timezone_get` / `date_default_timezone_set`
- `time`, `microtime`, `date`, and `gmdate` over selected format characters
- `DateTime` and `DateTimeImmutable` construction, formatting, timestamps,
  timezone access/mutation, `add`, `sub`, `modify`, and `diff` MVP paths
- `DateTimeZone` construction, `getName`, `timezone_open`,
  `timezone_name_get`, and deterministic identifier listing
- controlled `strtotime` parsing for ISO-like dates, numeric timestamps, and
  simple day-relative modifiers
- `DateInterval` ISO subset parsing, basic properties, formatting, and
  DateTime `add`/`sub` integration

## Non-Scope

- complete timelib natural-language parsing
- full timezone database transitions, aliases, and historical DST behavior
- complete Date/Time class method, property, warning, and exception parity

## Selected PHPT Fixture Groups

- `tests/phpt/generated/date/timezone-state.phpt`
- `tests/phpt/generated/date/date-time-functions.phpt`
- `tests/phpt/generated/date/datetime-format.phpt`
- `tests/phpt/generated/date/datetimeimmutable-format.phpt`
- `tests/phpt/generated/date/datetimezone-mvp.phpt`
- `tests/phpt/generated/date/strtotime-mvp.phpt`
- `tests/phpt/generated/date/dateinterval-mvp.phpt`
- `ext/date/tests/DateInterval_format.phpt`
- `ext/date/tests/DateInterval_format_a.phpt`
- `ext/date/tests/DateTimeZone_getName_basic1.phpt`
- `ext/date/tests/006.phpt`

## Relevant Source Areas

- `crates/php_runtime/src/datetime.rs`
- `crates/php_runtime/src/builtins/modules/date.rs`
- `crates/php_vm/src/vm/jit_abi/internal_classes/date_time.rs`
- `crates/php_vm/src/vm/jit_abi/native_builtins.rs`
- `docs/stdlib/known-gaps.md`

## Target Gates

- `nix develop -c cargo test -p php_runtime datetime`
- `nix develop -c cargo test -p php_vm`
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=date`
- `nix develop -c just diff-json-pcre-date`
- `nix develop -c just verify-stdlib`
- `nix develop -c just verify-phpt`

## Known Gaps

- focused selected manifest contains eleven Date/Time contracts and is expected
  to be green for both reference PHP 8.5.7 and the
  target runtime
- broader upstream `ext/date` rows remain documented corpus/backlog work for
  full timelib parsing, complete timezone database behavior, DatePeriod,
  `createFromFormat`, advanced interval behavior, and byte-perfect diagnostics
- the Date/Time MVP intentionally uses a deterministic fixed-offset timezone
  registry instead of importing PHP timelib/tzdb

## Next Step

The selected gate is closed for the focused generated Date/Time contracts. Keep the
selected manifest green while promoting broader upstream `ext/date` PHPT rows
only when their timelib, tzdb, and class-surface requirements are implemented.
