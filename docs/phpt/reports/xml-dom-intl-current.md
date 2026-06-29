# XML DOM Intl Current Focus Report

This branch enables a bounded WordPress XML/DOM/SimpleXML/XMLReader/XMLWriter
and Intl slice without adding SAPI behavior or vendoring php-src implementation
code.

## Parser Strategy And Dependencies

No new Rust dependency was added. The XML-family extensions share
`php_runtime::xml`, a strict local XML tree/parser facade that supports element
names, attributes, text nodes, built-in XML entities, deterministic
serialization, and a tiny reader/writer event surface. It rejects DTDs,
processing instructions outside the XML declaration, external entities,
unresolved entities, mismatched closes, and trailing content with stable runtime
diagnostics.

## Before / After

| Module | Before | After |
| --- | --- | --- |
| `xml` | Platform/doc posture only; no usable parser object path. | Enabled `XMLParser`, `xml_parser_create()`, and strict `xml_parse()` over the shared parser. |
| `dom` | Platform/doc posture only; no executable DOM document workflow. | Enabled XML-backed `DOMDocument`, `DOMElement`, `DOMNode`, `DOMNodeList`, load/save, constructed roots, element-local attributes/append, node properties, and tag lookup. |
| `simplexml` | Platform/doc posture only; no executable SimpleXML object workflow. | Enabled `simplexml_load_string()`, `SimpleXMLElement`, text conversion, child access, attributes, iteration, `asXML()`, and selected WordPress-style snippets. |
| `xmlreader` | No selected module surface. | Enabled in-memory `XMLReader` cursor methods and constants for element/text/end-element traversal. |
| `xmlwriter` | No selected module surface. | Enabled in-memory `XMLWriter` document/element/attribute/text output. |
| `intl` | Stub-only optional surface. | Enabled bounded NFC normalizer helpers, UTF-8-character grapheme helpers, small Latin ASCII transliteration, and `intl_get_error_code()`. |

## Implemented Scope

| Module | Selected PHPTs | Implemented behavior |
| --- | ---: | --- |
| `xml` | 2 | `XMLParser`, `xml_parser_create`, strict `xml_parse`, built-in entities, malformed XML rejection |
| `dom` | 3 | `DOMDocument`, `DOMElement`, `DOMNode`, `DOMNodeList`, `loadXML`, `saveXML`, `createElement`, `appendChild`, `getElementsByTagName`, node properties, attributes |
| `simplexml` | 3 | `simplexml_load_string`, text conversion, child access, attributes, iteration, `asXML`, WordPress-style RSS/plugin/config snippets |
| `xmlreader` | 1 | in-memory XML traversal, node fields, `getAttribute`, `close` |
| `xmlwriter` | 1 | memory writer document, element, attribute, text, output |
| `intl` | 3 | NFC normalizer helpers, UTF-8-character grapheme helpers, small Latin ASCII transliterator |

## Reference Oracle

The available sibling PHP oracle at
`/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php` reports all
requested extensions as missing: `dom`, `xml`, `simplexml`, `xmlreader`,
`xmlwriter`, and `intl`. Generated PHPTs therefore use `--EXTENSIONS--` so
target-only runs execute the new behavior while reference-backed module runs
skip these cases on that oracle instead of producing false differential claims.

No repo-local pinned `third_party/php-src` checkout is present in this branch,
so source-integrity verification skips with the standard `PHP_SRC_DIR` /
`just bootstrap-ref` guidance. Upstream ext PHPT corpus files remain read-only
inputs and were not copied into the repository; this branch uses focused
generated PHPTs for the executable selected slice.

## Stable Gaps

- `XML-DOM-INTL-XML-SAX-CALLBACKS`
- `XML-DOM-INTL-LIBXML-ERROR-STATE`
- `XML-DOM-INTL-DOM-NODE-MODEL`
- `XML-DOM-INTL-DOM-LIBXML-HTML`
- `XML-DOM-INTL-SIMPLEXML-NAMESPACES-XPATH`
- `XML-DOM-INTL-SIMPLEXML-LIBXML-ERRORS`
- `XML-DOM-INTL-XMLREADER-FULL-STREAM`
- `XML-DOM-INTL-XMLWRITER-FULL-SURFACE`
- `XML-DOM-INTL-INTL-ICU-DATA`
- `XML-DOM-INTL-INTL-GRAPHEME-SEGMENTATION`
- `XML-DOM-INTL-INTL-NORMALIZATION-FORMS`

## Remaining High-Priority Gaps

- Model libxml error state and options without enabling external entity or
  network loading.
- Expand DOM from object-local MVP mutation to live node ownership, namespace
  behavior, and broader node classes.
- Add namespace and XPath-aware SimpleXML behavior after DOM ownership is
  widened.
- Replace the bounded Intl NFC/transliteration/grapheme approximations with an
  ICU-backed strategy only after dependency policy approval.

## Required Gates

- `nix develop -c cargo test -p php_runtime`
- `nix develop -c cargo test -p php_vm`
- `nix develop -c just phpt-module-target MODULE=xml`
- `nix develop -c just phpt-module-target MODULE=dom`
- `nix develop -c just phpt-module-target MODULE=simplexml`
- `nix develop -c just phpt-module-target MODULE=xmlreader`
- `nix develop -c just phpt-module-target MODULE=xmlwriter`
- `nix develop -c just phpt-module-target MODULE=intl`
- `nix develop -c just verify-stdlib`
- `nix develop -c just verify-phpt`
