# PHP Lint Oracle

The primary reference oracle for parser acceptance is the pinned PHP CLI:

```bash
export REFERENCE_PHP="$PWD/third_party/php-src/sapi/cli/php"
"$REFERENCE_PHP" -l path/to/file.php
```

The target binary is PHP 8.5.7 from tag `php-8.5.7`. If `REFERENCE_PHP` is set,
parser acceptance checks must use it and fail on mismatches unless the mismatch
is documented as a known gap.

## Normalized Result

Parser comparison should normalize `php -l` to a small JSON shape:

```json
{
  "php_version": "8.5.7",
  "file": "fixtures/parser/valid/basic.php",
  "ok": true,
  "exit_code": 0,
  "stdout": "No syntax errors detected in ...",
  "stderr": ""
}
```

The hard compatibility signal is the boolean acceptance result. Exact diagnostic
text matching is a soft goal because PHP error wording can vary across builds
and is less important than acceptance, stable spans, and recovery behavior at
this stage.

The Rust parser CLI emits the comparison-side shape:

```json
{
  "file": "fixtures/parser/valid/basic.php",
  "ok": true,
  "diagnostics": [],
  "roundtrip_ok": true
}
```

`ok` is true only when the Rust parser emits no diagnostics and the CST
roundtrip exactly reconstructs the source.

## Missing Reference Binary

If no reference PHP binary is available, reference-dependent checks must report
a clear skip reason. They must not silently pass and must not fall back to an
unversioned system PHP for strict compatibility claims.

`parser-diff` requires a reference binary reporting PHP 8.5.7. If only another
PHP binary is available, the command skips with a message identifying the
reported version.

## Known Gap Allowlist

Parser acceptance mismatches are strict. A mismatch is accepted only when it is
listed in `fixtures/parser/known_gaps.toml` with the fixture path and expected
reference/Rust behavior. Stale allowlist entries fail the comparison so the file
stays current.

## Repository Commands

The current oracle plumbing is exposed through:

```bash
nix develop -c just parser-lint-oracle
nix develop -c just parser-fixtures
nix develop -c just parser-diff
```

`parser-lint-oracle` and `parser-fixtures` run the PHP reference side over
`fixtures/parser/**/*.php`. `parser-diff` also runs the Rust parser CLI and
fails on acceptance mismatches, roundtrip failures, or stale allowlist entries.
When the pinned PHP 8.5.7 reference is unavailable, `parser-diff` skips with a
clear message.

Current valid fixtures, including inline HTML, multiple PHP blocks, and short
echo tags, are expected to be accepted by `php -l`. Current invalid and recovery
fixtures are expected to be rejected by `php -l` but still parsed losslessly by
the Rust parser with recovery-oriented CST output.
