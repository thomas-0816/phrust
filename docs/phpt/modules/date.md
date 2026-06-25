# date

- Priority: 19
- Selected manifest: `tests/phpt/manifests/modules/date.selected.jsonl`
- Current counts: 14 PASS, 12 SKIP, 661 FAIL, 0 BORK from 687 corpus candidates

## Scope

- date/time builtins and DateTime MVP

## Non-Scope

- complete timelib natural-language parity

## Relevant PHPT Paths

- `ext/date/tests/unserialize-test.phpt`
- `ext/date/tests/timezones.phpt`
- `ext/date/tests/timezones-list.phpt`
- `ext/date/tests/timezone_version_get_basic1.phpt`
- `ext/date/tests/timezone_version_get.phpt`
- `ext/date/tests/timezone_transitions_get_basic1.phpt`
- `ext/date/tests/timezone_open_warning.phpt`
- `ext/date/tests/timezone_open_basic1.phpt`
- `ext/date/tests/timezone_offset_get_error.phpt`
- `ext/date/tests/timezone_offset_get_basic1.phpt`
- `ext/date/tests/timezone_name_from_abbr_basic1.phpt`
- `ext/date/tests/timezone_location_get.phpt`
- `ext/date/tests/timezone_identifiers_list_wrong_constructor.phpt`
- `ext/date/tests/timezone_identifiers_list_basic1.phpt`
- `ext/date/tests/timezone_abbreviations_list_basic1.phpt`
- `ext/date/tests/timezone-configuration.phpt`
- `ext/date/tests/timestamp-in-dst.phpt`
- `ext/date/tests/test-parse-from-format.phpt`
- `ext/date/tests/sunfuncts_partial_hour_utc_offset.phpt`
- `ext/date/tests/sunfuncts.phpt`
- `ext/date/tests/strtotime_variation_scottish.phpt`
- `ext/date/tests/strtotime_basic.phpt`
- `ext/date/tests/strtotime3.phpt`
- `ext/date/tests/strtotime3-64bit.phpt`
- `ext/date/tests/strtotime2.phpt`
- `ext/date/tests/strtotime.phpt`
- `ext/date/tests/strtotime-relative.phpt`
- `ext/date/tests/strtotime-mysql.phpt`
- `ext/date/tests/strtotime-mysql-64bit.phpt`
- `ext/date/tests/strftime_variation9.phpt`
- `ext/date/tests/strftime_variation8.phpt`
- `ext/date/tests/strftime_variation7.phpt`
- `ext/date/tests/strftime_variation6.phpt`
- `ext/date/tests/strftime_variation5.phpt`
- `ext/date/tests/strftime_variation4.phpt`
- `ext/date/tests/strftime_variation3.phpt`
- `ext/date/tests/strftime_variation22.phpt`
- `ext/date/tests/strftime_variation21.phpt`
- `ext/date/tests/strftime_variation20.phpt`
- `ext/date/tests/strftime_variation19.phpt`

## Relevant php-src Source Areas

- `ext/date/tests/`

## Target Gates

- `nix develop -c just phpt-module MODULE=date`

## Known Gaps

- `runtime-error-or-diagnostic`: 535
- `runtime-unsupported-feature`: 67
- `runtime-output-mismatch`: 58
- `frontend-parse-or-compile`: 15

## Next Step

Stabilize timezone persistence and common formatting/parsing.
