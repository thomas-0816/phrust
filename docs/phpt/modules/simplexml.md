# SimpleXML PHPT module

The SimpleXML slice is a bounded XML-backed MVP. It uses the same strict local
XML tree as the DOM/XMLReader/XMLWriter slices and does not yet provide full
libxml2 namespace, XPath, or live DOM identity semantics.

## Selected rows

- `tests/phpt/generated/simplexml/platform-checks.phpt`
- `tests/phpt/generated/simplexml/simplexml-basic.phpt`
- `tests/phpt/generated/simplexml/children-method.phpt`
- `tests/phpt/generated/simplexml/count-selections.phpt`
- `tests/phpt/generated/simplexml/array-offsets-getname.phpt`
- `tests/phpt/generated/simplexml/xpath-basic.phpt`
- `tests/phpt/generated/simplexml/asxml-savexml-file.phpt`
- `tests/phpt/generated/simplexml/load-file.phpt`
- `tests/phpt/generated/simplexml/wordpress-snippets.phpt`
- `tests/phpt/generated/simplexml/dom-interop.phpt`
- `tests/phpt/generated/simplexml/libxml-namespaces-cdata.phpt`

## Covered surface

- `extension_loaded("simplexml")`
- `SimpleXMLElement` class visibility
- `simplexml_load_string` and `simplexml_load_file`
- `simplexml_import_dom` over bounded DOM document/element storage
- `dom_import_simplexml` over bounded SimpleXML element/list storage
- Text conversion, `attributes()`, `children()`, `count()`, and `getName()`
- Array-style attribute offsets and numeric child selection offsets
- Bounded `xpath()` over strict XML elements and attributes
- `registerXPathNamespace()` prefix registration for the bounded XPath matcher
- Child property access, `foreach`, duplicate child-list iteration keys
- `asXML()` and `saveXML()` string and filename output
- WordPress-style RSS, plugin metadata, and config XML snippets

## Current selected gate

Run with the pinned PHP source oracle:

```bash
REFERENCE_PHP=third_party/php-src/sapi/cli/php \
PHP_SRC_DIR=third_party/php-src \
PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 PHPT_DISABLE_REFERENCE_REUSE=1 \
PHPT_TIMEOUT_SECONDS=20 \
PHPT_WORK_DIR=target/phpt-work/simplexml-selected \
nix develop -c just phpt-dev-module MODULE=simplexml
```

Verified selected summary after the native adapter slice: reference `PASS 11`,
target `PASS 11`, with result reuse disabled.

The native adapter lives in
`crates/php_vm/src/vm/jit_abi/internal_classes/simple_xml.rs`; XML parsing and
SimpleXML data semantics remain owned by `php_runtime::xml`.

## Remaining gaps

- Full upstream ext/simplexml corpus
- `SimpleXMLIterator`
- Full libxml namespace semantics
- Full XPath grammar
- Live DOM/SimpleXML node identity sharing and mutation visibility
- PHP array/object casting, debug output, comments, processing instructions, and
  namespace edge behavior beyond the bounded generated rows
