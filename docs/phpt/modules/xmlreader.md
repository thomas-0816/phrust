# xmlreader PHPT module status

## Scope

- `extension_loaded("xmlreader")`.
- `XMLReader::XML`, `open`, `read`, `next`, and `close`.
- Bounded node cursor properties: `nodeType`, `name`, `localName`, `prefix`,
  `namespaceURI`, `depth`, `value`, `attributeCount`, `hasAttributes`, and
  `hasValue`.
- Attribute navigation and namespace lookup with `getAttribute`,
  `getAttributeNo`, `lookupNamespace`, `moveToAttribute`,
  `moveToAttributeNo`, `moveToFirstAttribute`, `moveToNextAttribute`, and
  `moveToElement`.
- String readers with `readString`, `readInnerXml`, and `readOuterXml`.
- Bounded `expand()` DOM interop returning a `DOMElement` for element cursors.

## Non-scope

- Full upstream `ext/xmlreader` corpus parity.
- URI and stream-wrapper behavior beyond local file reads.
- Validation flags and schema validation.
- Live XMLReader/DOM ownership and lifetime coupling.
- Full libxml error queue integration.

## Selected tests

- `tests/phpt/generated/xmlreader/basic.phpt`
- `tests/phpt/generated/xmlreader/navigation-readxml.phpt`
- `tests/phpt/generated/xmlreader/open-local-file.phpt`
- `tests/phpt/generated/xmlreader/attributes-namespaces.phpt`
- `tests/phpt/generated/xmlreader/expand-dom-interop.phpt`

## Verification

- `REFERENCE_PHP="$PWD/third_party/php-src/sapi/cli/php" PHP_SRC_DIR="$PWD/third_party/php-src" PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 PHPT_DISABLE_REFERENCE_REUSE=1 PHPT_TIMEOUT_SECONDS=20 PHPT_WORK_DIR="$PWD/target/phpt-work/xmlreader-selected" nix develop -c just phpt-dev-module MODULE=xmlreader`
  - Reference: PASS 5, non-green 0.
  - Target: PASS 5, non-green 0.
- `nix develop -c cargo test -q -p php_runtime xml::tests`
  - PASS: 3 tests.
