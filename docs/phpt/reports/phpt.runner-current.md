# phpt.runner current report

Last focused run: 2026-06-28.

## Commands

- `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src nix develop -c just phpt-triage`: PASS.
- `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src nix develop -c just phpt-runner-smoke`: PASS, 19 PHPT runner-smoke cases with 12 PASS, 6 SKIP, 1 XFAIL, and 0 non-green outcomes.
- `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src nix develop -c just verify-phpt`: PASS, 21,548 corpus entries and 20,428 accepted non-green fingerprints verified.

## BORK subclasses

Current committed triage reports 437 `phpt.runner` BORK-owned non-green cases:

| Subclass | Count |
| --- | ---: |
| malformed-or-non-utf8-phpt | 313 |
| missing-target-cli-capability | 96 |
| unsupported-section | 21 |
| unsupported-expectation | 10 |
| unsupported-file-external | 6 |
| unsupported-runner-io | 1 |

`other-bork` remains visible in the global triage report and is not hidden by the runner module report.

## Runner support covered by smoke fixtures

- `EXPECT`
- `EXPECTF`
- `EXPECTREGEX`
- `XFAIL`
- `SKIPIF`
- `CLEAN`
- `INI`
- `ENV`
- `ARGS`
- `STDIN`
- `FILEEOF`
- `FILE_EXTERNAL`
- selected target capability skips for CGI, PHPDBG, gzip POST, deflate POST, and extension requirements

## Remaining runner-owned BORKs

The remaining runner-owned BORKs are tracked by subclass in `docs/phpt/reports/triage.md` and `tests/phpt/manifests/known-gap-catalog.jsonl`. They stay explicit until the runner can either execute the PHPT safely or assign the test to an engine, CLI, SAPI-policy, or extension-policy owner.
