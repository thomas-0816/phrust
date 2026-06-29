# closure.extensions

- Strategy: aggregate application-extension closure gate
- Selected manifest: `tests/phpt/manifests/modules/closure.extensions.selected.jsonl`
- Selected gate: 43 generated PHPTs

## Purpose

`closure.extensions` is the branch-wide dashboard gate for application extension
work. It combines the narrow module gates that matter to common application
startup, installers, package handling, media uploads, HTTP/update checks,
database bootstraps, text processing, crypto helpers, and cache probes.

## Covered Surface

| Area | Selected coverage |
| --- | --- |
| Database | `mysqli`, `PDO`, `pdo_sqlite`, and `SQLite3` platform/query basics, MySQLi SQLite compatibility status flow, SQLite3 prepared binding/status helpers, PDO SQLite parameters/transactions/object fetch/exception mode, plus explicit live MySQL DSN gates |
| HTTP and OpenSSL | cURL platform/default-off/local-URL gates, selected local cURL POST/header/redirect behavior, and selected OpenSSL digest/random/method helpers |
| XML and Intl | strict `xml_parse`, XML parser error helpers, SimpleXML child/attribute/iteration/file access, and bounded intl fallback helpers |
| Media | deterministic `fileinfo`, `exif`, and selected GD image helper fixtures |
| Compression and Archives | zlib whole-buffer helpers and read-only PHAR platform coverage |
| Utilities | `filter`, `iconv`, `sodium`, `bcmath`, and `gmp` selected application helpers |
| Cache and State | request-local APCu plus explicit Redis/Memcached probe surfaces and request-local sessions |

## Explicit Non-Scope

- FPM, FastCGI, CGI, Apache `mod_php`, phpdbg, and Zend extension ABI.
- Public internet tests or live network/database access without explicit
  capability gates.
- Full libcurl, OpenSSL, ICU, libxml, libmagic, GD, Redis, Memcached, or MySQL
  protocol parity.
- Stubs that pretend behavior succeeded without a selected fixture proving the
  behavior.

## Target Gates

- `nix develop -c just phpt-dev-module MODULE=closure.extensions`
- `nix develop -c just verify-phpt`

Run changed module gates before this aggregate gate.
