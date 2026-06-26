# PHPT: PHPT-driven Runtime Completion

PHPT turns the php-src PHPT corpus into the primary implementation loop for
runtime completion. The pinned php-src checkout is read-only input containing:

- the PHPT corpus: all discovered `.phpt` files;
- php-src C/Zend implementation source used for source lookup and behavior
  notes;
- Reference PHP build inputs and `run-tests.php` for parity checks.

The Rust engine remains the Target PHP. PHPT does not copy php-src source
into the Rust implementation and does not implement behavior by editing the
reference checkout.

Supporting docs:

- [Source integrity](source-integrity.md)
- [Source lookup](source-lookup.md)
- [Binary discovery](binary-discovery.md)
- [Official run-tests.php cross-check](official-runner.md)
- [Generated PHPTs](generated-tests.md)
- [Full PHPT gate](full-phpt-gate.md)

## Terms

Original PHPT: a `.phpt` file under the pinned php-src checkout. It is never
modified.

Derived PHPT: a generated test under `tests/phpt/generated/` with provenance
back to an Original PHPT or Reference PHP observation.

Minimized PHPT: a smaller regression case reduced from an Original PHPT while
preserving the targeted behavior.

Module batch: a curated group of Original PHPT, Derived PHPT, and Minimized PHPT
cases for one runtime area.

Full PHPT gate: a complete corpus run compared against the accepted known
failure baseline after each module batch.

Fast PHPT iteration: keep a `nix develop` shell open, run `just phpt-dev-build`
after code changes that affect binaries, then run `just phpt-dev-module
MODULE=<module>` for the full module comparison or `just phpt-fast
MODULE=<module>` for target-only feedback. Use `PATTERN=<text>` or
`FILE=<path>` with `phpt-fast` to run one failure cluster or one PHPT while
debugging. Focused runs write separate results and do not overwrite the module
cache. Use `just phpt-rerun-failures MODULE=<module>` to rerun only the latest
non-green module outcomes, or `just phpt-dev-fast ...` for the explicit local
`PHPT_DEV_REUSE_PASS=1` path that can reuse unchanged previous PASS results
across binary changes. The fast targets use an external work directory by
default, enable strict previous-result reuse, and skip rebuilds so small runtime
changes can be checked without paying full-corpus or repeated shell startup
cost. PHPT runs are serial by default; set `PHPT_JOBS=<n>` only for an
intentional parallel batch. `just phpt-build` remains the normal deterministic
build command, and `php-phpt-tools run --reuse-results <results.jsonl>` also
reuses unchanged test outcomes by strict fingerprint. The full regression script
uses the latest previous run automatically unless `PHPT_DISABLE_REUSE=1`
is set; after `just phpt-dev-build`, use `just phpt-full-fast` to reuse the
already-built PHPT binaries for a local full-gate check.

## Runner Compatibility

`just phpt-runner-smoke` is the focused gate for PHPT runner behavior. It runs
generated runner fixtures against the pinned Reference PHP CLI and currently
covers:

- `SKIPIF`
- `CLEAN`
- `EXPECT`
- `EXPECTF`
- `EXPECTREGEX`
- `XFAIL`
- `INI`
- `ENV`
- `ARGS`
- `STDIN`
- `CAPTURE_STDIO`
- `FILEEOF`
- `FILE_EXTERNAL`
- metadata-only sections `FLAKY`, `WHITESPACE_SENSITIVE`, and `XLEAK`
- SAPI policy sections `CGI` and `PHPDBG`, which are explicit `SKIP` results
  when the local runner has no matching php-cgi or phpdbg target
- compressed POST sections `GZIP_POST` and `DEFLATE_POST`, which are explicit
  `SKIP` results until a php-cgi-compatible target is configured

For original php-src `CAPTURE_STDIO` cases that use the upstream
`SKIP_IO_CAPTURE_TESTS` skip hook, the local runner sets that environment value
while evaluating `SKIPIF` when the host process does not expose stdin, stdout,
and stderr as terminals. This keeps non-interactive local and CI runs
deterministic instead of reporting TTY-topology mismatches as engine failures.

The runner classifies remaining BORKs separately from VM/runtime failures so
PHPT infrastructure gaps are fixed before module work attributes failures to
the engine.

## Required Layout

```text
third_party/php-src-8.5.7/      # preferred pinned php-src checkout
third_party/php-src/            # current local checkout name, accepted by tools
target/phpt-work/               # generated run artifacts only
tests/phpt/generated/           # derived and minimized PHPTs
tests/phpt/manifests/           # indexes, module manifests, baselines
docs/phpt/modules/            # module plans and notes
docs/phpt/php-src-behavior/   # behavior notes from source lookup
docs/phpt/reports/            # committed summary reports
```

Generated run artifacts belong under `target/phpt-work/` and must not be
committed.

## Gate Meanings

Module green means the selected module batch passes for runnable tests and
Derived PHPT or Minimized PHPT cases for that module pass.

Full-run no-regression means the complete PHPT corpus was executed and compared
with the accepted known-failure baseline. Existing known failures may remain,
but new unexpected failures, BORKs, crashes, timeouts, or changed fingerprints
outside the current module reject the change.

Final strict green means the full PHPT corpus passes under the final strict
policy. Any remaining skip or xfail must come from legitimate PHPT metadata,
platform conditions, or a documented intentionally unsupported external
extension.

## Current Foundation Status

The current PHPT foundation includes source integrity checks, source lookup,
PHPT corpus indexing, generated PHPT support, Reference PHP smoke checks,
official `run-tests.php` smoke checks, target smoke reporting, and full-corpus
known-failure baselining. Later PHPT module work uses these gates to close
runtime gaps without modifying Original PHPTs.
