# xmlwriter PHPT module status

## Scope

- `extension_loaded("xmlwriter")`.
- Memory writer construction with `XMLWriter::openMemory`, `XMLWriter::toMemory`,
  and `xmlwriter_open_memory`.
- Object writer methods: `startDocument`, `startElement`, `writeAttribute`,
  `text`, `writeComment`, `writeCdata`, `writeElement`, `endElement`,
  `endDocument`, and `outputMemory`.
- Procedural aliases for the selected memory writer surface:
  `xmlwriter_start_document`, `xmlwriter_start_element`,
  `xmlwriter_write_attribute`, `xmlwriter_text`,
  `xmlwriter_write_comment`, `xmlwriter_write_cdata`,
  `xmlwriter_write_element`, `xmlwriter_end_document`, and
  `xmlwriter_output_memory`.
- URI writer construction with `openUri`, `toUri`, and `xmlwriter_open_uri`,
  plus capability-checked object and procedural `flush` behavior.

## Non-scope

- Full upstream `ext/xmlwriter` corpus parity.
- Stream-wrapper output targets beyond the selected local-file URI slice.
- Namespace writers, indentation options, DTD nodes, PI nodes, and full libxml
  error behavior.
- Start/end comment and CDATA state machines.

## Selected tests

- `tests/phpt/generated/xmlwriter/basic.phpt`
- `tests/phpt/generated/xmlwriter/procedural-memory.phpt`
- `tests/phpt/generated/xmlwriter/comments-cdata.phpt`
- `tests/phpt/generated/xmlwriter/uri-flush.phpt`

## Verification

- `REFERENCE_PHP="$PWD/third_party/php-src/sapi/cli/php" PHP_SRC_DIR="$PWD/third_party/php-src" PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 PHPT_DISABLE_REFERENCE_REUSE=1 PHPT_TIMEOUT_SECONDS=20 PHPT_WORK_DIR="$PWD/target/phpt-work/xmlwriter-selected" nix develop -c just phpt-dev-module MODULE=xmlwriter`
  - Reference: PASS 4, non-green 0.
  - Target: PASS 4, non-green 0.
- `nix develop -c cargo test -q -p php_runtime xml::tests`
  - PASS: 3 tests.
