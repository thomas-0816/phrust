# Standard Core Dashboard

closeout dashboard for the standard-library core PHPT slices. The
baseline counts are the selected triage snapshot from
`docs/phpt/reports/triage.md` / baseline `20260624T210848Z`. The closeout counts
are the selected module gates run against the committed selected
manifests.

`standard.output` and `standard.url-html` now have dedicated selected manifests:

- `tests/phpt/manifests/modules/standard.output.selected.jsonl`
- `tests/phpt/manifests/modules/standard.url-html.selected.jsonl`

## Closeout

| Module | Before FAIL/BORK | Focused gate | After FAIL/BORK | Remaining gaps |
| --- | ---: | ---: | ---: | --- |
| `standard.arrays` | 595 / 0 | 10 PASS | 0 / 0 | Full upstream array corpus, comparator sorting, callback-heavy helpers, and broader COW/reference behavior remain outside the selected focused gate. |
| `standard.strings` | 308 / 0 | 15 PASS | 0 / 0 | Full string corpus breadth remains, including formatting edge cases, flags, encodings, and less common helpers outside the focused binary-safe slice. |
| `standard.math` | 62 / 0 | 161 PASS, 11 SKIP | 0 / 0 | Reference-style SKIPs remain; broader numeric edge cases and PHPTs blocked by non-math parser/runtime helpers stay outside this closeout. |
| `standard.variables` | 348 / 0 | 26 PASS, 1 SKIP | 0 / 0 | Full `var_dump`/`print_r` matrix, magic/object visibility edges, and reference formatting remain outside the focused selected gate. |
| `standard.output` | 63 / 20 | 11 PASS | 0 / 0 | Output-buffer callbacks, chunk sizes, flag combinations, and handler-list APIs remain beyond the selected stack-backed buffer slice. |
| `standard.serialization` | 107 / 0 | 5 PASS | 0 / 0 | PHP `R`/`r` reference identity records, `allowed_classes`, magic hooks, resources, and deep object/reference serialization remain known gaps. |
| `standard.url-html` | 37 / 5 | 36 PASS | 0 / 0 | Complete HTML entity tables, non-default charsets beyond selected UTF-8 coverage, object query encoding/property visibility, parse_url edge cases beyond the selected corpus, remaining parse_str edge cases including startup filter.default deprecation output, and broader URL behavior remain outside the selected gate. |

## Validation

- `nix develop -c bash -lc 'just phpt-dev-build; for module in standard.arrays standard.strings standard.math standard.variables standard.output standard.serialization standard.url-html; do PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 just phpt-dev-module MODULE="$module"; done'`: PASS
- `nix develop -c just verify-stdlib`: PASS
- `REFERENCE_PHP=$PWD/third_party/php-src/sapi/cli/php PHPT_RUN_FULL=1 nix develop -c just phpt-full-regression`: FAIL, ran 21,548 PHPTs and reported 11 new/changed failure fingerprints outside the focused standard-core gates. Reports: `target/phpt-work/full-runs/20260628T003531Z/results.jsonl` and `target/phpt-work/full-runs/20260628T003531Z/summary.md`.
- Focused follow-up for the two `stdClass` display-name regressions from that full run, using a temporary two-test manifest for `Zend/tests/closures/closure_call.phpt` and `Zend/tests/first_class_callable/first_class_callable_errors.phpt`: PASS.

## Full Regression Follow-Up

The full-regression run did not pass. It exposed two `stdClass` diagnostic
display-name regressions, which are fixed and covered by the focused rerun
above. The remaining new/changed fingerprints are broader SAPI, stdio, include,
and `stream_isatty` outcomes outside the selected standard-core focused
manifests.

## Scope Notes

- The `After FAIL/BORK` column reports the focused selected module gates, not
  the entire upstream `ext/standard` corpus.
- The baseline failure counts remain useful for backlog sizing. They should not
  be interpreted as the current focused gate state.
- Run artifacts are intentionally kept under `target/` or
  `/private/tmp/phrust-phpt-work/`; this dashboard records only the durable
  module status.
