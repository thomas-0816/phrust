# soap

- Strategy: platform-unavailable policy harness
- Classification: out-of-scope
- Selected manifest: `tests/phpt/manifests/modules/soap.selected.jsonl`
- Selected gate: 1 generated PHPT covering SOAP platform visibility
- Corpus snapshot: 589 `soap`-owned candidates in
  `tests/phpt/manifests/phpt-corpus.jsonl`; committed baseline counts are
  0 PASS, 16 SKIP, 567 FAIL, 6 BORK, and 577 known non-green outcomes.

## Decision

Do not implement SOAP in this branch.

SOAP requires WSDL parsing, XML schema handling, DOM/libxml behavior, HTTP and
stream integration, encoding rules, persistence modes, and security-sensitive
request/response processing. It is not part of the current core-runtime PHPT
green path.

## Runtime Contract

- `extension_loaded("soap")` returns `false`.
- `class_exists("SoapClient", false)`, `class_exists("SoapServer", false)`,
  `class_exists("SoapFault", false)`, and `class_exists("SoapHeader", false)`
  return `false`.

## Required PHPTs

Required for this strategy:

- `tests/phpt/generated/soap/platform-checks.phpt`

## Unsupported Area

- Stable ID: `XML-FAMILY-SOAP-OUT-OF-SCOPE`
- Reference behavior summary: PHP with `ext/soap` enabled exposes SOAP client,
  server, fault, parameter, header, and WSDL/encoding behavior declared in
  `ext/soap/soap.stub.php`.
- Current phrust behavior: SOAP is not registered in the standard-library
  extension registry, so extension and class probes return false.
- Fixture path: `tests/phpt/generated/soap/platform-checks.phpt`
- Next owner layer: no current owner; a future SOAP layer would need DOM/XML,
  streams, HTTP, and schema support first.

## Out-of-Scope PHPTs

Out of scope for this branch:

- Upstream `ext/soap/tests/**`
- WSDL, HTTP transport, XML schema, encoding, persistence, and security
  regression behavior

## Target Gates

- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=soap`
- `nix develop -c just verify-phpt`

## Next Step

Keep SOAP out of scope and visible in baseline accounting.
