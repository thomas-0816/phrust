# calendar PHPT coverage

Current selected upstream PHPT coverage:

- 46 upstream `ext/calendar` rows currently pass on the target, covering
  Gregorian, Julian, Jewish, and French republican conversion helpers,
  `cal_from_jd()`, `cal_to_jd()`, `cal_days_in_month()`, `JDMonthName()`,
  `JDDayOfWeek()`, `easter_days()`, `JDToUnix()`, `UnixToJD()`,
  Hebrew-letter `JDToJewish()` formatting, and overflow regression rows for
  extreme serial day numbers, `PHP_INT_MAX` Easter year validation, exact
  `jdtounix()` bounds diagnostics, `easter_date()` timestamp year diagnostics,
  the final month of the French calendar, and `juliantojd()` large-year
  narrowing parity. The promoted rows also cover exact calendar `ValueError`
  argument labels and integer bounds plus invalid Jewish serial-day array
  output.

The selected PHPTs avoid host-dependent locale data and assert deterministic
serial day number behavior against the pinned php-src oracle. The latest full
upstream target sweep now reports 46 PASS, 7 SKIP, and 0 FAIL. The selected
manifest intentionally contains the 46 non-skipped rows under the pinned
php-src oracle; broader timezone/host-local coverage for skipped
`easter_date()` rows remains outside the deterministic slice.

Focused gate:

```bash
REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php \
PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src \
PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 \
PHPT_DISABLE_REFERENCE_REUSE=1 PHPT_TIMEOUT_SECONDS=20 \
nix develop -c just phpt-dev-module MODULE=calendar
```
