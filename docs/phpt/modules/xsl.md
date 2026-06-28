# xsl

- Strategy: platform-unavailable policy harness
- Classification: optional
- Selected manifest: `tests/phpt/manifests/modules/xsl.selected.jsonl`
- Selected gate: 1 generated PHPT covering XSL platform visibility
- Corpus snapshot: 72 `xsl`-owned candidates in
  `tests/phpt/manifests/phpt-corpus.jsonl`; committed known outcomes are
  65 FAIL, 7 BORK, and 72 known non-green outcomes.

## Decision

Do not implement XSL in this branch.

XSL requires DOM inputs, libxslt/libexslt integration, stylesheet import/include
handling, PHP callback registration, security preferences, filesystem/network
policy, and output serialization. That is beyond a safe platform-check harness.

## Runtime Contract

- `extension_loaded("xsl")` returns `false`.
- `class_exists("XSLTProcessor", false)` returns `false`.
- XSL constants such as `XSL_CLONE_AUTO` are not defined.

## Required PHPTs

Required for this strategy:

- `tests/phpt/generated/xsl/platform-checks.phpt`

## Unsupported Area

- Stable ID: `XML-FAMILY-XSL-REAL-IMPLEMENTATION`
- Reference behavior summary: PHP with `ext/xsl` enabled exposes
  `XSLTProcessor` and XSL/libxslt constants declared in
  `ext/xsl/php_xsl.stub.php`.
- Current phrust behavior: XSL is not registered in the standard-library
  extension registry, so extension, class, and constant probes return false.
- Fixture path: `tests/phpt/generated/xsl/platform-checks.phpt`
- Next owner layer: future DOM/XML implementation plus a dedicated XSL owner
  layer if libxslt integration is approved.

## Out-of-Scope PHPTs

Out of scope for this branch:

- Upstream `ext/xsl/tests/**`
- Stylesheet parsing, transforms, file/network includes, callback registration,
  and libxslt security preferences

## Target Gates

- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=xsl`
- `nix develop -c just verify-phpt`

## Next Step

Keep XSL classified as optional and blocked on DOM/XML plus an explicit libxslt
dependency decision.
