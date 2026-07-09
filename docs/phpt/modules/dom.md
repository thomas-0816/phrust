# DOM PHPT module

The DOM slice is a bounded XML-backed MVP, not a full libxml2 DOM
implementation. The selected gate covers generated rows that are stable in the
current runtime and keeps upstream DOM PHPTs outside the selected set until
their behavior is implemented.

## Selected rows

- `tests/phpt/generated/dom/platform-checks.phpt`
- `tests/phpt/generated/dom/domdocument-basic.phpt`
- `tests/phpt/generated/dom/domdocument-node-apis.phpt`
- `tests/phpt/generated/dom/domtext-node-apis.phpt`
- `tests/phpt/generated/dom/domdocument-secondary-nodes.phpt`

## Covered surface

- `extension_loaded("dom")`
- `DOMDocument`, `DOMElement`, `DOMAttr`, `DOMText`, `DOMComment`,
  `DOMCdataSection`, `DOMNode`, and `DOMNodeList` class visibility
- `DOMDocument::loadXML`, `saveXML`, `createElement`, `createTextNode`,
  `createComment`, `createCDATASection`, `createAttribute`, `appendChild`, and
  `getElementsByTagName`
- `DOMElement::getAttribute`, `setAttribute`, `setAttributeNode`, and
  `appendChild`
- `DOMNodeList::item`, `length`, `count`, and basic `foreach`
- Bounded serialization for element, text, comment, CDATA, and attribute nodes

## Current selected gate

Run with the pinned PHP source oracle:

```bash
REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php \
PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src \
PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 PHPT_DISABLE_REFERENCE_REUSE=1 \
PHPT_TIMEOUT_SECONDS=20 \
PHPT_WORK_DIR=/private/tmp/phrust-phpt-dom-selected-secondary-nodes \
nix develop -c just phpt-dev-module MODULE=dom
```

Verified selected summary after this slice: reference `SKIP 5`, target `PASS 5`.

## Remaining gaps

- libxml2-backed live node identity and ownership
- `DOMXPath`
- namespace liveness and namespace-aware mutation
- schema and DTD validation
- `loadHTML` and `saveHTML`
- full `DOMDocumentType`, `DOMDocumentFragment`, `DOMNamedNodeMap`, and related
  class/method/property parity
- upstream ext/dom PHPT promotion beyond the bounded generated rows
