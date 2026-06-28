# dom

- Strategy: platform-unavailable policy harness
- Classification: optional
- Selected manifest: `tests/phpt/manifests/modules/dom.selected.jsonl`
- Selected gate: 1 generated PHPT covering DOM platform visibility
- Corpus snapshot: 879 `dom`-owned candidates in
  `tests/phpt/manifests/phpt-corpus.jsonl`; committed baseline counts are
  7 PASS, 14 SKIP, 851 FAIL, 7 BORK, and 879 known non-green outcomes.

## Decision

Do not implement DOM in this branch.

The DOM extension requires a document object model, namespace-aware node
ownership, HTML/XML parsing and serialization, XPath integration, libxml error
state, file/stream loading, Reflection metadata, and class behavior across a
large object surface. A fake parser or partial successful `DOMDocument` object
would make framework probes believe DOM exists while leaving PHP-visible
behavior incorrect.

## Runtime Contract

- `extension_loaded("dom")` returns `false`.
- `class_exists("DOMDocument", false)`, `class_exists("DOMElement", false)`,
  `class_exists("DOMNode", false)`, and `class_exists("DOMXPath", false)`
  return `false`.
- No DOM parsing, serialization, XPath, schema validation, or libxml behavior is
  implemented by this branch.

## Required PHPTs

Required for this strategy:

- `tests/phpt/generated/dom/platform-checks.phpt`

This fixture records platform visibility only. It is not a DOM behavior test.

## Unsupported Area

- Stable ID: `XML-FAMILY-DOM-REAL-IMPLEMENTATION`
- Reference behavior summary: PHP with `ext/dom` enabled exposes the DOM class
  hierarchy from `ext/dom/php_dom.stub.php` and executes the upstream
  `ext/dom/tests/**` corpus against a libxml-backed object model.
- Current phrust behavior: DOM is not registered in the standard-library
  extension registry, so extension and class probes return false.
- Fixture path: `tests/phpt/generated/dom/platform-checks.phpt`
- Next owner layer: future `php_std` extension registry metadata plus
  `php_runtime`/`php_vm` object, stream, and XML integration.

## Out-of-Scope PHPTs

Out of scope for this branch:

- Upstream `ext/dom/tests/**`
- XML/HTML parse and serialize behavior
- XPath and schema validation
- DOM node mutation, liveness, namespace, and Reflection parity

## Target Gates

- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=dom`
- `nix develop -c just verify-phpt`

## Next Step

Keep DOM counted in PHPT bookkeeping and defer real behavior to a dedicated
DOM/XML implementation strategy.
