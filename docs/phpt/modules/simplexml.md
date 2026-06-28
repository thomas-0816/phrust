# simplexml

- Strategy: platform-unavailable policy harness
- Classification: optional
- Selected manifest: `tests/phpt/manifests/modules/simplexml.selected.jsonl`
- Selected gate: 1 generated PHPT covering SimpleXML platform visibility
- Corpus snapshot: 157 `simplexml`-owned candidates in
  `tests/phpt/manifests/phpt-corpus.jsonl`; committed baseline counts are
  0 PASS, 2 SKIP, 155 FAIL, 0 BORK, and 157 known non-green outcomes.

## Decision

Do not implement SimpleXML in this branch.

SimpleXML depends on real XML parsing, libxml error behavior, object property
views over XML nodes, iterator behavior, namespace handling, and DOM
interoperability. A stub that returns successful XML objects would hide the
actual unsupported area.

## Runtime Contract

- `extension_loaded("simplexml")` returns `false`.
- `class_exists("SimpleXMLElement", false)` and
  `class_exists("SimpleXMLIterator", false)` return `false`.
- `function_exists("simplexml_load_string")`,
  `function_exists("simplexml_load_file")`, and
  `function_exists("simplexml_import_dom")` return `false`.

## Required PHPTs

Required for this strategy:

- `tests/phpt/generated/simplexml/platform-checks.phpt`

## Unsupported Area

- Stable ID: `XML-FAMILY-SIMPLEXML-REAL-IMPLEMENTATION`
- Reference behavior summary: PHP with `ext/simplexml` enabled exposes
  `SimpleXMLElement`, `SimpleXMLIterator`, and loader/import functions declared
  in `ext/simplexml/simplexml.stub.php`.
- Current phrust behavior: SimpleXML is not registered in the standard-library
  extension registry, so extension, class, and function probes return false.
- Fixture path: `tests/phpt/generated/simplexml/platform-checks.phpt`
- Next owner layer: future DOM/XML parser work plus `php_runtime` object and
  iterator integration.

## Out-of-Scope PHPTs

Out of scope for this branch:

- Upstream `ext/simplexml/tests/**`
- `ext/libxml/tests/**` cases owned by SimpleXML policy
- XML object views, XPath-like access, namespaces, iteration, and DOM import

## Target Gates

- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=simplexml`
- `nix develop -c just verify-phpt`

## Next Step

Keep SimpleXML counted and blocked on the future DOM/XML parser strategy.
