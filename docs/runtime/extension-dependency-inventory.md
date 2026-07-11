# Extension Dependency Inventory

This inventory is derived from `crates/php_runtime/Cargo.toml`, Rust imports,
the builtin module index, and the registry module slices. It records the
ownership baseline before the extension boundary was introduced.

## Layer Direction

The enforced direction is:

`php_runtime` core -> extension contract -> `php_extensions` implementations -> CLI/server integration

`php_runtime --no-default-features` contains values, containers, objects,
references, resources, diagnostics, request primitives, and capability types.
The default `full-runtime` feature temporarily retains unmigrated compatibility
code. `scripts/verify/runtime_core_boundaries.py` rejects backend/frontend
packages in the minimal graph and rejects an outward core dependency.

## Direct Dependency Ownership

| Family | Previous runtime dependencies | Source owners |
| --- | --- | --- |
| Database | `mysql`, `postgres`, `rusqlite` | `db/mysql.rs`, `db/postgres.rs`, `sqlite.rs`, `builtins/modules/{mysqli,pdo,pgsql}.rs` |
| Network | `curl`, `imap`, `ldap3`, `ssh2`, `suppaftp` | `builtins/context.rs`, `builtins/modules/{curl,ftp,imap,ldap,soap,ssh2}.rs` |
| Native/media | `image`, `exif`, `libxml`, `openssl`, `zip`, `pcre2`, libsodium | `builtins/modules/{exif,gd,openssl,pcre,sodium,xml,zip}.rs`, `pcre.rs`, `xml_backend.rs`, `phar.rs` |
| Frontend | `php_lexer`, `php_syntax` | `tokenizer.rs`; lexer-only string token helpers remain in the feature-gated compatibility path |
| Crypto/hash | digest, password, random and encoding crates | `builtins/modules/{core,hash,openssl,sodium,strings}.rs` |

All extension-only direct dependencies are optional and selected by
`full-runtime`; none enter the minimal runtime graph.

## Pilot Choice

`ctype` is the stateless/light pilot. It exercises function descriptors,
diagnostics, deterministic registration, and no request-state allocation.

`APCu` is the stateful pilot. Its process-shared store handle exercises state
factory metadata and the `Clock`/`ProcessSharedState` capability declarations.
The VM's temporary APCu state adapter remains until Prompt 08 replaces the
duplicated `BuiltinContext` state model with typed slots.

Both implementations now live in `crates/php_extensions`; their old runtime
module declarations and registry slices are removed.

## Clean Build Comparison

Measured on macOS in the repository Nix shell with separate empty target
directories on 2026-07-11:

| Configuration | Clean build | Direct/transitive packages | Debug runtime artifact |
| --- | ---: | ---: | ---: |
| `php_runtime --no-default-features` | 4.70 s | 29 | 10,112,592 bytes |
| default `php_runtime` | 40.41 s | 341 | 50,011,648 bytes |

The artifact is Cargo's debug `libphp_runtime-*.rlib`, used here because the
runtime package does not own an executable. Wall time is machine-local evidence,
not a stable CI threshold. Package exclusion and the minimal build itself are
the deterministic gates.

## Remaining Migration Ownership

| Owner | Remaining implementations |
| --- | --- |
| Core/general builtins | `core`, `arrays`, `strings`, `math`, `filesystem`, `streams`, `filter` |
| Encoding/data | `json`, `mbstring`, `intl`, `iconv`, `igbinary`, `msgpack`, `serialization` |
| Crypto/compression | `hash`, `openssl`, `sodium`, `pcre`, `zlib`, `zip`, `fileinfo` |
| Database/cache | `mysqli`, `pdo`, `pgsql`, `sqlite`, `redis`, `memcached`, `opcache` |
| Network/protocol | `curl`, `ftp`, `imap`, `ldap`, `soap`, `sockets`, `ssh2` |
| Media/XML | `exif`, `gd`, `xml`, `simplexml` |
| Process/platform | `pcntl`, `posix`, `readline`, `shmop`, `sysvmsg`, `sysvsem`, `sysvshm`, `gettext` |
| Language/runtime metadata | `date`, `session`, `spl`, `reflection`, `calendar`, `bcmath`, `gmp` |

This list is closed over the current `builtins/modules/mod.rs` source index;
new modules require an explicit owner and descriptor.
