# sapi

- Strategy: target-policy classification
- Classification: out-of-scope outside CLI-compatible behavior
- Selected manifest: `tests/phpt/manifests/modules/sapi.selected.jsonl`
- Current corpus snapshot: 347 `sapi` candidates, 2 PASS, 17 SKIP, 254 FAIL,
  73 BORK, and 346 known non-green outcomes.

## Decision

Keep production SAPI implementations out of scope.

The target binary is a CLI-compatible PHPT runner, not FPM, FastCGI, Apache,
CGI, phpdbg, or a web request lifecycle. CLI-only behavior can be handled in
`phpt.cli` or the VM CLI when a focused PHPT exercises ordinary command-line
semantics.

## Unsupported Area

- Stable ID: `PHPT-DATA-SAPI`
- Reference behavior: PHP's SAPI matrix exposes distinct CGI/FPM/Apache/phpdbg
  behavior, request environment, headers, process management, and web lifecycle
  integration.
- Current phrust behavior: only CLI-style execution is supported;
  `php_sapi_name()` reports `cli`, but `PHP_SAPI` is not yet defined as a
  constant. Non-CLI SAPI probes are unsupported or skipped by the PHPT runner.
- Fixture: `tests/phpt/generated/sapi/platform-checks.phpt`
- Next owner layer: target CLI for CLI-compatible behavior; no owner for
  production web SAPIs in current scope.

## Policy

- FPM/FastCGI/Apache/CGI: out-of-scope.
- phpdbg-specific tests: out-of-scope.
- CLI-only behavior: allowed only when routed to CLI/runtime owner modules.

## Source References

- `sapi/`
- SAPI-tagged PHPTs in the committed corpus manifest

## Target Gates

- `nix develop -c just phpt-dev-module MODULE=sapi`
- `nix develop -c just verify-phpt`
