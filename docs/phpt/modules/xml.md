# xml

- Strategy: platform-unavailable policy harness
- Classification: optional
- Selected manifest: `tests/phpt/manifests/modules/xml.selected.jsonl`
- Selected gate: 1 generated PHPT covering XML parser platform visibility
- Corpus snapshot: 65 `xml`-owned candidates in
  `tests/phpt/manifests/phpt-corpus.jsonl`; committed known outcomes are
  64 FAIL, 1 BORK, and 65 known non-green outcomes.

## Decision

Do not implement the XML SAX parser extension in this branch.

The XML extension exposes Expat-backed parser resources/objects, callback
registration, parser options, byte/line/column tracking, encoding behavior, and
error-code constants. Returning successful parse results without that state
machine would be a false positive.

## Runtime Contract

- `extension_loaded("xml")` returns `false`.
- `class_exists("XMLParser", false)` returns `false`.
- `function_exists("xml_parser_create")`,
  `function_exists("xml_parse")`, and `function_exists("xml_error_string")`
  return `false`.
- `defined("XML_ERROR_NONE")` returns `false`.

## Required PHPTs

Required for this strategy:

- `tests/phpt/generated/xml/platform-checks.phpt`

## Unsupported Area

- Stable ID: `XML-FAMILY-XML-SAX-PARSER`
- Reference behavior summary: PHP with `ext/xml` enabled exposes XML parser
  constants, `XMLParser`, and parser/callback functions declared in
  `ext/xml/xml.stub.php`.
- Current phrust behavior: XML is not registered in the standard-library
  extension registry, so extension, class, function, and constant probes return
  false.
- Fixture path: `tests/phpt/generated/xml/platform-checks.phpt`
- Next owner layer: future `php_std` extension metadata and `php_runtime`
  parser resource/object support.

## Out-of-Scope PHPTs

Out of scope for this branch:

- Upstream `ext/xml/tests/**`
- XML parser creation and callback dispatch
- Encoding, namespace, error-code, and parser position parity

## Target Gates

- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=xml`
- `nix develop -c just verify-phpt`

## Next Step

Keep XML parser PHPTs classified and defer real behavior until an approved XML
parser dependency and runtime object/resource model are selected.
