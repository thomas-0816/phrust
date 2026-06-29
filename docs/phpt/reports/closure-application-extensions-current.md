# Application Extension Closure Current State

Branch: `phpt/closure-application-extensions`

Oracle: PHP 8.5.7 from
`/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php`.

## Dashboard

The aggregate selected gate is
`tests/phpt/manifests/modules/closure.extensions.selected.jsonl` with 43
fixtures. It is intentionally composed from focused generated fixtures already
owned by the individual extension modules and the `wp.db-network` capability
gate.

## Implemented vs Missing

| Module | Current implemented surface | Remaining gaps |
| --- | --- | --- |
| `curl` | `CurlHandle`, init/setopt/exec/getinfo/error/close/version, default-off network policy, explicit local URL gate, selected POST string/array bodies, outgoing headers, bounded local redirects, and response-header inclusion | HTTPS/TLS, `curl_multi_*`, proxy/auth/upload streaming, multipart file upload parity, full libcurl options |
| `openssl` | random bytes, digest methods, method listing, explicit verify gap | certificate/key helpers, encrypt/decrypt, real signature verification, TLS streams |
| `mysqli` | WordPress-oriented connection/query/result/error/escape/close/status surface behind `PHRUST_MYSQL_TEST_DSN`, plus opt-in in-memory SQLite compatibility for selected query/fetch/status fixtures | prepared statements, mysqlnd parity, MySQL wire-protocol parity, real protocol coverage without explicit DSN |
| `PDO` / `pdo_sqlite` / `sqlite3` | SQLite-backed platform/query basics, SQLite3 prepared binding/status helpers, PDO SQLite positional and named parameters, row IDs, object fetch basics, transactions, and selected exception mode | SQLite callbacks/custom functions, persistent connections, broader attribute behavior, exact warning/error parity |
| `phar` | read-only local PHAR reads/includes | mutation APIs, signatures, compression, `PharData`, metadata parity |
| `session` | request-local session start/status/id/name/destroy | persistent storage, handlers, serializers, full SAPI lifecycle |
| `xml` / `simplexml` | strict XML parse/reject, parser error helpers, and SimpleXML text/attribute/child/iteration/asXML/local-file basics | SAX callbacks, namespaces, XPath, DOM interop, full libxml error state |
| `intl` | NFC normalization helpers, scalar grapheme helpers, bounded transliteration, error-code probe | ICU locale/formatter/collator/IDNA/break-iterator parity |
| `fileinfo` / `exif` / `gd` | deterministic MIME, selected JPEG EXIF, selected image load/resize/write helpers | full libmagic, full EXIF matrix, full GD drawing/font/filter API |
| `zlib` | gzip/zlib/raw whole-buffer helpers and selected gzip file helpers | streaming filters, output compression, full parameter/warning parity |
| `filter` / `iconv` | common validation/sanitization and UTF-8/ASCII/ISO-8859-1 conversion helpers | full filter option matrix, full charset catalog/transliteration |
| `sodium` | selected real BLAKE2b, Ed25519, hex/base64 helpers | secretbox, password hashing, AEAD, key exchange, sealed boxes |
| `bcmath` / `gmp` | bounded decimal and BigInt arithmetic helpers | full warning/rounding/object/base-conversion parity |
| `apcu` | request-local store/fetch/add/delete/exists/clear | shared-memory persistence, iterators, stats, process lifecycle |
| `redis` / `memcached` | extension/class probe surfaces | daemon protocols, persistent connections, command emulation |

## First Highest-Value Slices

1. PDO/SQLite closure: SQLite callbacks, persistent connections, broader
   attribute behavior, and exact warning/error parity.
2. cURL/OpenSSL transport closure: deterministic local HTTPS and
   certificate/key helper coverage after selecting dependency strategy.
3. XML/SimpleXML practical broadening: namespace-aware access for selected
   cases, fuller libxml error state, and small callback coverage where
   deterministic.

## Gate Plan

- Run each owned focused module gate first.
- Run `nix develop -c just phpt-dev-module MODULE=closure.extensions`.
- Run `nix develop -c just verify-phpt`.

This report must be updated when selected fixtures are promoted or behavior
scope changes.
