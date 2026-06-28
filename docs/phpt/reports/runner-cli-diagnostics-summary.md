# PHPT runner, CLI, and diagnostics closeout

Date: 2026-06-28.

## Scope

This report closes the Prompt 1A through 1D PHPT parity slice:

- PHPT runner section, expectation, `SKIPIF`, `INI`, `ENV`, `ARGS`, `STDIN`,
  `FILEEOF`, and local external-file handling.
- `php-cli` target invocation for `-v`, `-r`, `-n`, repeated `-d`, script
  argv, STDIN, `$argc`, `$argv`, and `$_SERVER` argv/argc.
- Selected runtime diagnostic-output fixtures and adjacent smoke modules.

## Current results

| Gate | Result |
| --- | --- |
| `just verify-phpt` | PASS; 21,548 corpus entries, 20,428 known non-green fingerprints, and 24,475 php-src manifest entries verified |
| `just phpt-dev-module MODULE=phpt.runner` | PASS; reference 3 PASS / 197 SKIP, target 3 PASS / 197 SKIP, 0 non-green |
| `just phpt-dev-module MODULE=phpt.cli` | PASS; 203 selected cases, target 3 PASS / 200 SKIP, 0 non-green |
| `just phpt-dev-module MODULE=diagnostics.output` | PASS; 6 PASS, 0 non-green |
| `just phpt-dev-module MODULE=zend.basic` | PASS; 10 PASS, 0 non-green |
| `just phpt-dev-module MODULE=operators.conversions` | PASS; 4 PASS, 0 non-green |
| `just verify-runtime` | PASS; runtime fixtures, runtime semantics diff, VM smoke, bytecode snapshots, and runtime/VM clippy gate passed |

## Runner outcome

The runner now handles local relative `FILE_EXTERNAL` and `EXPECT*_EXTERNAL`
payloads without allowing absolute paths, parent traversal, or symlink escapes
outside the PHPT directory. Non-UTF8 PHPT sources are read lossily for
classification and then skipped with an explicit runner gap instead of
producing reference failures in selected runs.

Current committed triage still reports 437 `phpt.runner` BORK-owned non-green
cases:

| Subclass | Count |
| --- | ---: |
| malformed-or-non-utf8-phpt | 313 |
| missing-target-cli-capability | 96 |
| unsupported-section | 21 |
| unsupported-expectation | 10 |
| unsupported-file-external | 6 |
| unsupported-runner-io | 1 |

`other-bork` remains visible in the global triage report and is not hidden by
this module closeout.

## CLI outcome

The selected `phpt.cli` manifest starts with three generated contract fixtures:

- `tests/phpt/generated/phpt.cli/argv-argc-superglobals.phpt`
- `tests/phpt/generated/phpt.cli/ini-overrides.phpt`
- `tests/phpt/generated/phpt.cli/stdin.phpt`

Those generated fixtures pass on the target. The remaining selected upstream
cases skip with concrete non-scope reasons for FPM, phpdbg, built-in server,
process-control, stdio descriptor rebinding, `--ini`, `-R`, and unrelated
runtime/frontend gaps.

## Diagnostics outcome

The selected diagnostics fixtures are green. Covered behavior includes
undefined-variable warnings, array-to-string warnings, missing include warnings,
fatal type diagnostics, builtin arity/type exceptions, and `display_errors` /
`error_reporting` plumbing.

## Merge risks and follow-up ownership

- SAPI/FPM/phpdbg/CGI tests remain out of scope for the `php-cli` target.
- CLI built-in server, process-control APIs, stdio descriptor rebinding, `--ini`,
  and `-R` remain visible as concrete skips.
- Non-UTF8 PHPT sources are classified as runner gaps rather than executed.
- The `hrtime` busy-loop fixture is a target VM step-limit gap, not runner
  behavior.
- Include/require failures keep a VM-local special renderer for PHP's two-line
  include warning and required-file fatal shape; ordinary VM and builtin
  warnings route through the shared diagnostic output helper.
- Full PHP diagnostic wording parity remains broader than the selected
  diagnostics gate.
