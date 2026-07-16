# Current Extension Gap Matrix

Generated from current source inputs by `scripts/stdlib/current_extension_gap_matrix.py`.
This is an audit artifact only; it does not change runtime behavior.

## Source Inputs

- `crates/php_runtime/Cargo.toml` (present)
- `crates/php_runtime/build.rs` (present)
- `crates/php_runtime/src/builtins/registry.rs` (present)
- `crates/php_runtime/src/builtins/modules` (present)
- `crates/php_vm/src/vm/builtin_classes.rs` (missing)
- `fixtures/stdlib/extensions` (present)
- `crates/php_std/src/generated/extensions` (present)
- `tests/phpt/manifests/modules` (present)
- `tests/phpt/generated` (present)

## Matrix

| Extension | Registered functions/classes/constants | Backend library currently used | Current behavior class | Highest-value missing behavior | Recommended next prompt |
| --- | --- | --- | --- | --- | --- |
| `fileinfo` | functions=5 (5 runtime), classes=1, constants=11; runtime module | libmagic via build.rs/pkg-config FFI | `library-backed-but-thin` | Flag combinations, finfo object edge cases, and broader libmagic PHPT promotion. PHPT manifest/selected/generated: 21/38/3 | FILEINFO-1 |
| `hash` | functions=20 (20 runtime), classes=1, constants=40; runtime module | RustCrypto/hash crates plus patched tiger crate | `library-backed-but-thin` | Algorithm/context parity, HMAC edge behavior, streaming file/hash contexts. PHPT manifest/selected/generated: 50/84/5 | HASH-1 |
| `zlib` | functions=29 (29 runtime), classes=2, constants=27; runtime module | flate2 | `library-backed-but-thin` | Streaming inflate/deflate contexts, gzip file handles, and warning parity. PHPT manifest/selected/generated: 19/56/3 | ZLIB-1 |
| `mbstring` | functions=25 (25 runtime), classes=0, constants=8; runtime module | encoding_rs plus custom UTF-8 helpers | `library-backed-but-thin` | Alias map breadth, stateful mb_* settings, regex-free string edge cases. PHPT manifest/selected/generated: 17/118/7 | MBSTRING-1 |
| `iconv` | functions=10 (10 runtime), classes=0, constants=4; runtime module | encoding_rs plus custom MIME helpers | `library-backed-but-thin` | Charset aliases, transliteration/ignore options, MIME folding parity. PHPT manifest/selected/generated: 38/49/8 | ICONV-1 |
| `curl` | functions=27 (27 runtime), classes=3, constants=145; runtime module | curl crate for version metadata; execution still uses custom transport | `custom-subset` | Move curl_exec and getinfo/error data to libcurl Easy2. PHPT manifest/selected/generated: 17/16/4 | CURL-1 |
| `zip` | functions=10 (10 runtime), classes=1, constants=0; runtime module | zip crate | `library-backed-but-thin` | ZipArchive write/update/delete/comment/stat coverage and libzip-like errors. PHPT manifest/selected/generated: 19/51/3 | ZIP-1 |
| `pdo` | functions=1 (1 runtime), classes=4, constants=0; runtime module | custom VM PDO facade with rusqlite/mysql/postgres-adjacent support | `custom-subset` | PDO core object model, attributes, exceptions, statement lifecycle. PHPT manifest/selected/generated: 17/4/1 | PDO-1 |
| `pdo_sqlite` | functions=0 (0 runtime), classes=1, constants=0; no runtime module | rusqlite through VM PDO path | `library-backed-but-thin` | SQLite statement binding/fetch modes/transactions/error modes. PHPT manifest/selected/generated: 17/3/3 | PDO-SQLITE-1 |
| `pdo_mysql` | functions=0 (0 runtime), classes=1, constants=0; no runtime module | mysql crate where live DSN support is present | `library-backed-but-thin` | Real PDO MySQL driver behavior beyond platform/live smoke coverage. PHPT manifest/selected/generated: 17/2/2 | PDO-MYSQL-1 |
| `pdo_pgsql` | functions=0 (0 runtime), classes=2, constants=0; no runtime module | postgres crate where live DSN support is present | `library-backed-but-thin` | Real PDO PgSQL driver behavior beyond platform/live smoke coverage. PHPT manifest/selected/generated: 17/3/3 | PDO-PGSQL-1 |
| `mysqli` | functions=62 (62 runtime), classes=5, constants=14; runtime module | mysql crate plus sqlite-compatible fallback paths | `library-backed-but-thin` | mysqlnd-compatible result, prepared statement, and error semantics. PHPT manifest/selected/generated: 17/10/3 | MYSQLI-1 |
| `pgsql` | functions=22 (22 runtime), classes=3, constants=4; runtime module | postgres crate | `library-backed-but-thin` | Connection/resource lifecycle, query/result APIs, and live PHPT expansion. PHPT manifest/selected/generated: 17/2/2 | PGSQL-1 |
| `sqlite3` | functions=0 (0 runtime), classes=4, constants=12; no runtime module | rusqlite | `library-backed-but-thin` | SQLite3 class completeness, statement/result APIs, busy/error behavior. PHPT manifest/selected/generated: 17/2/2 | SQLITE3-1 |
| `xml` | functions=15 (15 runtime), classes=1, constants=6; runtime module | custom bounded XML parser/tree | `custom-subset` | libxml-compatible SAX errors, encodings, namespaces, parser options. PHPT manifest/selected/generated: 19/12/8 | XML-1 |
| `dom` | functions=0 (0 runtime), classes=10, constants=0; no runtime module | native internal DOM classes over bounded XML data | `custom-subset` | Live node ownership, import/adopt, schema/DTD, HTML, and libxml error parity. PHPT manifest/selected/generated: 17/7/7 | DOM-1 |
| `simplexml` | functions=4 (4 runtime), classes=1, constants=0; runtime module | custom SimpleXML object model | `custom-subset` | Namespace/xpath/array-cast/reference-cell behavior. PHPT manifest/selected/generated: 17/11/11 | SIMPLEXML-1 |
| `xmlreader` | functions=0 (0 runtime), classes=1, constants=0; no runtime module | custom bounded XML reader facade | `custom-subset` | Streaming reader state, attributes, namespaces, and error semantics. PHPT manifest/selected/generated: 17/5/5 | XMLREADER-1 |
| `xmlwriter` | functions=13 (13 runtime), classes=1, constants=0; no runtime module | custom XML writer facade | `custom-subset` | Full writer API, memory/document modes, invalid name and encoding errors. PHPT manifest/selected/generated: 17/4/4 | XMLWRITER-1 |
| `intl` | functions=10 (10 runtime), classes=22, constants=2; runtime module | manual subset; no ICU backend detected | `custom-subset` | ICU-backed Normalizer, Collator, transliteration, locale data. PHPT manifest/selected/generated: 17/6/4 | INTL-1 |
| `openssl` | functions=22 (22 runtime), classes=3, constants=10; runtime module | openssl crate | `library-backed-but-thin` | Cipher/method breadth, certificate/key/resource APIs, warning parity. PHPT manifest/selected/generated: 17/14/2 | OPENSSL-1 |
| `sodium` | functions=42 (42 runtime), classes=1, constants=55; runtime module | pure Rust crypto crates; no libsodium backend detected | `custom-subset` | libsodium-compatible primitives, key validation, and constant parity. PHPT manifest/selected/generated: 17/6/3 | SODIUM-1 |
| `gd` | functions=27 (27 runtime), classes=1, constants=6; runtime module | image crate | `library-backed-but-thin` | Image resource model, drawing/text/color APIs, codec/error parity. PHPT manifest/selected/generated: 17/2/2 | GD-1 |
| `imagick` | functions=0 (0 runtime), classes=5, constants=0; no runtime module | no ImageMagick/MagickWand backend detected | `stub/fake-success-risk` | Replace class metadata/backend gate with real MagickWand-backed behavior. PHPT manifest/selected/generated: 33/1/1 | IMAGICK-1 |
| `exif` | functions=5 (5 runtime), classes=0, constants=1; runtime module | custom JPEG/EXIF parser | `custom-subset` | TIFF/IFD breadth, malformed data warnings, image-type helpers. PHPT manifest/selected/generated: 19/24/1 | EXIF-1 |
| `json` | functions=5 (5 runtime), classes=2, constants=29; runtime module | serde_json | `library-backed-and-broad` | Remaining numeric/string flag edge cases and error-message parity. PHPT manifest/selected/generated: 16/94/6 | JSON-1 |
| `pcre` | functions=11 (11 runtime), classes=0, constants=19; runtime module | patched pcre2 crate | `library-backed-but-thin` | PCRE option matrix, callbacks, error offsets, and delimiter edge cases. PHPT manifest/selected/generated: 16/173/8 | PCRE-1 |
| `apcu` | functions=12 (12 runtime), classes=0, constants=0; no runtime module | request-local custom cache state | `custom-subset` | TTL/SMA/cache-info semantics and request/persistent lifecycle parity. PHPT manifest/selected/generated: 17/2/2 | APCU-1 |
| `redis` | functions=0 (0 runtime), classes=2, constants=0; runtime module | deterministic in-memory VM fake; no Redis protocol backend | `stub/fake-success-risk` | Real phpredis client semantics or explicit fake/backend boundary. PHPT manifest/selected/generated: 35/3/3 | REDIS-1 |
| `memcached` | functions=0 (0 runtime), classes=2, constants=0; runtime module | deterministic in-memory VM fake; no libmemcached backend | `stub/fake-success-risk` | Real Memcached protocol/options/result-code behavior. PHPT manifest/selected/generated: 36/3/3 | MEMCACHED-1 |
| `igbinary` | functions=2 (2 runtime), classes=0, constants=0; runtime module | custom serializer-compatible subset | `custom-subset` | Binary format parity, object/reference behavior, session serializer hooks. PHPT manifest/selected/generated: 37/2/2 | IGBINARY-1 |
| `msgpack` | functions=4 (4 runtime), classes=2, constants=3; runtime module | custom serializer-compatible subset | `custom-subset` | MessagePack binary compatibility, options, and object/reference behavior. PHPT manifest/selected/generated: 39/2/2 | MSGPACK-1 |
| `ftp` | functions=36 (36 runtime), classes=1, constants=11; runtime module | suppaftp backend behind request-local FTP state | `library-backed-but-thin` | FTPS, broader transfer/listing modes, passive mode edge cases, and FTP error parity. PHPT manifest/selected/generated: 32/2/2 | FTP-1 |
| `ldap` | functions=58 (58 runtime), classes=3, constants=22; runtime module | ldap3 sync backend behind request-local LDAP state | `library-backed-but-thin` | Modify/TLS controls, result traversal breadth, option parity, and LDAP error stacks. PHPT manifest/selected/generated: 32/2/2 | LDAP-1 |
| `imap` | functions=36 (36 runtime), classes=1, constants=27; runtime module | imap crate with native-tls connection backend | `library-backed-but-thin` | MIME/message structure parsing, fetch/search breadth, mailbox flags, and error stack parity. PHPT manifest/selected/generated: 32/2/2 | IMAP-1 |
| `ssh2` | functions=30 (30 runtime), classes=3, constants=10; runtime module | ssh2 crate/libssh2 backend behind request-local SSH2 state | `library-backed-but-thin` | Shell/tunnel/publickey behavior, stream metadata, and broader SFTP operation parity. PHPT manifest/selected/generated: 36/2/2 | SSH2-1 |
| `soap` | functions=2 (2 runtime), classes=14, constants=81; runtime module | custom SOAP value/class facade | `custom-subset` | WSDL/client/server XML serialization and transport behavior. PHPT manifest/selected/generated: 17/3/3 | SOAP-1 |
| `sockets` | functions=20 (20 runtime), classes=2, constants=15; runtime module | libc/std socket wrappers | `library-backed-but-thin` | Socket options, address families, errors, select/sendmsg coverage. PHPT manifest/selected/generated: 31/1/1 | SOCKETS-1 |
| `posix` | functions=41 (41 runtime), classes=0, constants=15; runtime module | nix/libc | `library-backed-but-thin` | User/group/process APIs, errno parity, platform-specific skips. PHPT manifest/selected/generated: 17/37/1 | POSIX-1 |
| `pcntl` | functions=21 (21 runtime), classes=0, constants=19; runtime module | libc process-signal wrappers | `library-backed-but-thin` | Fork/wait/signal/alarm semantics and platform gate parity. PHPT manifest/selected/generated: 17/7/1 | PCNTL-1 |
| `shmop` | functions=6 (6 runtime), classes=1, constants=0; runtime module | custom/platform shared-memory facade | `custom-subset` | Real System V shared memory semantics and permissions. PHPT manifest/selected/generated: 17/4/1 | SHMOP-1 |
| `sysvmsg` | functions=7 (7 runtime), classes=1, constants=5; runtime module | custom/platform System V facade | `custom-subset` | Real queue send/receive/stat/remove semantics. PHPT manifest/selected/generated: 17/8/1 | SYSVMSG-1 |
| `sysvsem` | functions=4 (4 runtime), classes=1, constants=0; runtime module | custom/platform System V facade | `custom-subset` | Semaphore acquire/release/remove/undo semantics. PHPT manifest/selected/generated: 17/3/1 | SYSVSEM-1 |
| `sysvshm` | functions=7 (7 runtime), classes=1, constants=0; runtime module | custom/platform System V facade | `custom-subset` | Shared memory attach/put/get/remove behavior. PHPT manifest/selected/generated: 17/11/1 | SYSVSHM-1 |
| `readline` | functions=13 (13 runtime), classes=0, constants=1; runtime module | noninteractive custom facade | `metadata-only` | Interactive readline/history/completion behavior. PHPT manifest/selected/generated: 17/17/1 | READLINE-1 |
| `spl` | functions=11 (11 runtime), classes=55, constants=0; runtime module | custom VM/runtime SPL classes | `custom-subset` | Iterator/file/autoload/data-structure completeness. PHPT manifest/selected/generated: 144/232/9 | SPL-1 |
| `reflection` | functions=0 (0 runtime), classes=26, constants=0; runtime module | custom VM reflection metadata | `custom-subset` | Complete reflection metadata, attributes, types, internal signatures. PHPT manifest/selected/generated: 144/46/9 | REFLECTION-1 |
| `opcache` | functions=8 (8 runtime), classes=0, constants=0; runtime module | custom status/config facade | `metadata-only` | Real opcache semantics are out of runtime scope; keep facade honest. PHPT manifest/selected/generated: 17/1/1 | OPCACHE-1 |
| `phar` | functions=0 (0 runtime), classes=3, constants=0; no runtime module | custom read-only facade | `metadata-only` | Archive metadata, stream wrappers, signatures, write policy. PHPT manifest/selected/generated: 19/14/4 | PHAR-1 |
| `xsl` | functions=0 (0 runtime), classes=1, constants=10; no runtime module | no libxslt backend detected | `stub/fake-success-risk` | libxslt-backed XSLTProcessor behavior. PHPT manifest/selected/generated: 17/2/2 | XSL-1 |
| `standard` | functions=372 (372 runtime), classes=1, constants=136; no runtime module | custom runtime modules plus selected Rust crates | `custom-subset` | Array/string/filesystem/serialization edge parity across upstream PHPTs. PHPT manifest/selected/generated: 114/367/50 | STANDARD-1 |
| `core` | functions=62 (62 runtime), classes=40, constants=88; runtime module | VM/runtime core semantics | `custom-subset` | Zend language/runtime edge semantics and diagnostics. PHPT manifest/selected/generated: 33/1/1 | CORE-1 |
| `bcmath` | functions=10 (10 runtime), classes=0, constants=0; runtime module | custom runtime/VM implementation or metadata | `custom-subset` | No prompt-pack-specific next step; keep PHPT promotion source-derived. PHPT manifest/selected/generated: 22/3/3 | Backlog |
| `calendar` | functions=18 (18 runtime), classes=0, constants=21; runtime module | custom runtime/VM implementation or metadata | `custom-subset` | No prompt-pack-specific next step; keep PHPT promotion source-derived. PHPT manifest/selected/generated: 35/46/0 | Backlog |
| `ctype` | functions=11 (11 runtime), classes=0, constants=0; no runtime module | custom runtime/VM implementation or metadata | `custom-subset` | No prompt-pack-specific next step; keep PHPT promotion source-derived. PHPT manifest/selected/generated: 21/51/2 | Backlog |
| `date` | functions=15 (15 runtime), classes=5, constants=14; runtime module | custom runtime/VM implementation or metadata | `custom-subset` | No prompt-pack-specific next step; keep PHPT promotion source-derived. PHPT manifest/selected/generated: 16/11/7 | Backlog |
| `ffi` | functions=0 (0 runtime), classes=5, constants=0; no runtime module | custom runtime/VM implementation or metadata | `metadata-only` | No prompt-pack-specific next step; keep PHPT promotion source-derived. PHPT manifest/selected/generated: 39/3/3 | Backlog |
| `filter` | functions=7 (7 runtime), classes=0, constants=55; runtime module | custom runtime/VM implementation or metadata | `custom-subset` | No prompt-pack-specific next step; keep PHPT promotion source-derived. PHPT manifest/selected/generated: 82/117/3 | Backlog |
| `gettext` | functions=10 (10 runtime), classes=0, constants=0; runtime module | custom runtime/VM implementation or metadata | `custom-subset` | No prompt-pack-specific next step; keep PHPT promotion source-derived. PHPT manifest/selected/generated: 30/9/2 | Backlog |
| `gmp` | functions=49 (49 runtime), classes=1, constants=9; runtime module | custom runtime/VM implementation or metadata | `custom-subset` | No prompt-pack-specific next step; keep PHPT promotion source-derived. PHPT manifest/selected/generated: 30/2/2 | Backlog |
| `random` | functions=5 (5 runtime), classes=11, constants=0; no runtime module | custom runtime/VM implementation or metadata | `custom-subset` | No prompt-pack-specific next step; keep PHPT promotion source-derived. PHPT manifest/selected/generated: 0/0/0 | Backlog |
| `session` | functions=23 (23 runtime), classes=0, constants=3; runtime module | custom runtime/VM implementation or metadata | `custom-subset` | No prompt-pack-specific next step; keep PHPT promotion source-derived. PHPT manifest/selected/generated: 17/93/1 | Backlog |
| `test` | functions=1 (0 runtime), classes=0, constants=0; no runtime module | custom runtime/VM implementation or metadata | `metadata-only` | No prompt-pack-specific next step; keep PHPT promotion source-derived. PHPT manifest/selected/generated: 0/0/0 | Backlog |
| `tokenizer` | functions=2 (2 runtime), classes=1, constants=154; no runtime module | custom runtime/VM implementation or metadata | `custom-subset` | No prompt-pack-specific next step; keep PHPT promotion source-derived. PHPT manifest/selected/generated: 47/58/5 | Backlog |

## Notes

- The registered symbol counts come from `php_std::ExtensionRegistry::standard_library()` via `dump_stdlib_registry`.
- `runtime` counts mean the dumped function has a matching runtime or VM builtin registration.
- Behavior classes and next prompts are conservative annotations from the current prompt pack plus source-level backend evidence.
- PHPT columns are folded into the missing-behavior text as `manifest/selected/generated` counts for the same module name.
