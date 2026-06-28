# standard.url-html

- Priority: 16.8
- Selected manifest: `tests/phpt/manifests/modules/standard.url-html.selected.jsonl`
- Prompt 16.1 derived baseline: 1 PASS, 0 SKIP, 63 FAIL, 5 BORK from 69 path-filtered candidates
- Prompt 16.9 focused gate: 4 PASS, 0 FAIL, 0 BORK

## Scope

- URL encode/decode smoke coverage
- `http_build_query` array MVP coverage
- Default `htmlspecialchars` / `htmlentities` coverage

## Non-Scope

- Complete entity tables
- Non-default charsets and flags
- Object query encoding and RFC mode options
- Full URL/HTML upstream corpus

## Relevant PHPT Paths

- `ext/standard/tests/url/bug53248.phpt`
- `tests/phpt/generated/standard.url-html/url-encode-decode-smoke.phpt`
- `tests/phpt/generated/standard.url-html/http-build-query-smoke.phpt`
- `tests/phpt/generated/standard.url-html/htmlspecialchars-htmlentities-smoke.phpt`

## Relevant Source Areas

- `crates/php_runtime/src/builtins/modules/strings.rs`
- `crates/php_runtime/src/builtins/modules/core.rs`
- `docs/stdlib-encoding-hash-url.md`
- `docs/stdlib-known-gaps.md`

## Target Gates

- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=standard.url-html`
- `nix develop -c just verify-stdlib`

## Prompt 16 Evidence

- Added a dedicated selected manifest and generated smoke fixtures for the
  URL/HTML MVP.
- Existing URL/HTML helpers matched the selected reference cases; no runtime
  code changes were needed for this focused gate.
- Latest focused target run: PASS, 4 selected PHPTs.

## Known Gaps

- Full entity-table, charset, flag, RFC-mode, object, and URL edge-case
  behavior remains outside the Prompt 16 focused gate.
