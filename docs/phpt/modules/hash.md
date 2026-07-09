# hash

- Strategy: selected upstream digest, HMAC, and streaming context parity slice
- Selected manifest: `tests/phpt/manifests/modules/hash.selected.jsonl`
- Selected fixtures:
  - `tests/phpt/generated/hash/file.phpt`
  - `tests/phpt/generated/hash/context.phpt`
  - `tests/phpt/generated/hash/algos.phpt`
  - `tests/phpt/generated/hash/tiger4.phpt`
  - `tests/phpt/generated/hash/snefru256.phpt`
  - selected `ext/hash/tests/*.phpt` rows for adler32, CRC/FNV/JOOAT,
    HAVAL, MD2/MD4, md5, MurmurHash3, RIPEMD, sha1, SHA-2/SHA3,
    Tiger 3-pass, Tiger 4-pass, Snefru, Whirlpool, MurmurHash3/xxHash seed options and
    deprecation diagnostics,
    hash_equals, `hash_copy`, clone, HMAC-md5, HMAC basics, file hashing,
    HKDF, HKDF edge cases, PBKDF2, stream updates, legacy mhash compatibility,
    HashContext lifecycle diagnostics, HashContext serialization exceptions,
    malformed HashContext payload diagnostics, xxHash serialized memsize
    validation, xxHash secret object/stringable diagnostics, and HashContext
    debug-info formatting, plus SensitiveParameterValue masking in hash
    exception traces
- The selected mhash rows and GH-16711 mhash regression rows skip under the
    pinned reference PHP when `PHP_MHASH_BC` is not enabled; one stream-update
    row also skips there because the pinned reference lacks `openssl`. The
    phrust target currently passes all selected rows.
- Latest selected module gate with reference and target reuse disabled:
  reference 77 PASS / 7 SKIP; target 84 PASS; 0 unexpected non-green outcomes.

## Implemented Surface

The runtime exposes the existing `hash`, `hash_hmac`, `hash_algos`,
`hash_hmac_algos`, and `hash_equals` builtins plus `hash_file`,
`hash_hmac_file`, `hash_init`, `hash_update`, `hash_copy`, and `hash_final`.
The legacy mhash compatibility surface exposes `mhash`, `mhash_count`,
`mhash_get_block_size`, `mhash_get_hash_name`, and `mhash_keygen_s2k` plus the
php-src `MHASH_*` constants. The metadata registry exposes `HashContext`,
`HASH_HMAC`, and the mhash constants.

Supported digest/HMAC algorithms in this slice are `md2`, `md4`, `md5`,
`sha1`, `sha224`, `sha256`, `sha384`, `sha512/224`, `sha512/256`, `sha512`,
`sha3-224`, `sha3-256`, `sha3-384`, `sha3-512`, `ripemd128`, `ripemd160`,
`ripemd256`, `ripemd320`, `tiger128,3`, `tiger160,3`, `tiger192,3`,
`tiger128,4`, `tiger160,4`, `tiger192,4`, `snefru`, `snefru256`,
`haval128,3`, `haval160,3`, `haval192,3`, `haval224,3`, `haval256,3`,
`haval128,4`, `haval160,4`, `haval192,4`, `haval224,4`, `haval256,4`,
`haval128,5`, `haval160,5`, `haval192,5`, `haval224,5`, `haval256,5`,
`whirlpool`, `gost`, and `gost-crypto`;
digest-only coverage also includes `adler32`, `crc32`, `crc32b`, `crc32c`,
`fnv132`, `fnv1a32`, `fnv164`, `fnv1a64`, `joaat`, `murmur3a`, `murmur3c`,
`murmur3f`, `xxh32`, `xxh64`, `xxh3`, and `xxh128`.

The selected rows cover exact `hash_algos`/`hash_hmac_algos` inventory order,
SHA-256 file digests, SHA-256 file HMACs, raw binary file digest output,
upstream adler32/CRC/FNV/JOOAT/MD2/MD4/md5/MurmurHash3, GOST, HAVAL,
RIPEMD, sha1/SHA-2/SHA3/Tiger 3-pass/Tiger 4-pass/Snefru/Whirlpool digest
vectors, MurmurHash3/xxHash seeded one-shot and incremental vectors, HMAC-md5,
upstream HMAC basics, file hashing including php-src PHPT sibling fixture-file
paths, `hash_copy` and clone behavior across the selected algorithm inventory,
`hash_update_file`, `hash_update_stream`, `hash_pbkdf2`, and RFC5869
`hash_hkdf` vectors, plus HKDF edge cases for default lengths, oversized
lengths, and algorithm case-sensitivity. The promoted error rows cover invalid
algorithm ValueError wording for `hash`, `hash_file`, `hash_hmac`,
`hash_hmac_file`, `hash_init`, `hash_pbkdf2`, and `hash_hkdf`, missing-file
diagnostics for `hash_file`, null-byte filename diagnostics for
`hash_hmac_file`, HMAC-mode algorithm/key diagnostics for `hash_init`, plus
PBKDF2 iteration/length and HKDF key/length argument ValueErrors. The promoted
`hash_equals` row covers constant-time string comparison behavior plus strict
TypeError reporting for non-string arguments.
The promoted seed deprecation rows cover PHP's non-int seed diagnostics for
MurmurHash3 and xxHash algorithms, including the xxh3/xxh128 ignored-seed
behavior and xxHash non-string secret deprecation emission before catchable
ValueError paths, including object `__toString` exception propagation during
secret conversion. The context row covers `HashContext` visibility, incremental
SHA-256 hashing, copying a partially updated context, HMAC contexts, and
finalized context rejection. Promoted upstream HashContext rows cover direct
construction visibility, finalized-context reuse diagnostics, and `var_dump`
debug-info output with only the PHP-visible `algo` field. A focused VM
regression also covers direct `HashContext::__debugInfo()` calls and the
catchable `ArgumentCountError` arity path. The promoted mhash rows cover
legacy algorithm ID mapping, digest lengths, raw digest output, S2K key
    generation, reflection-visible constants through
    `ReflectionExtension::getConstants`, constant/function deprecation ordering
    for direct reads and `constant()`, and invalid numeric IDs returning `false`
    when the reference oracle exposes `PHP_MHASH_BC`; in the current pinned
    oracle build, those rows are selected but skip through their upstream
    `function_exists('mhash')` guard. The promoted upstream full-inventory rows
    now cover `hash_algos()` and `hash_hmac_algos()` byte-for-byte order, active
    HashContext cloning, finalized/HMAC HashContext serialization exceptions,
    malformed HashContext serialized-payload diagnostics, xxHash serialized
    memsize validation, serializable HashContext round trips for supported
    non-xxh3/xxh128 algorithms, xxHash stringable-secret conversion
    diagnostics, SensitiveParameterValue masking in hash exception stack
    traces, and PHPT-style `__DIR__ . "file"` sibling fixture files.

## Gaps

HashContext native serialized-state byte parity is not complete across the full
algorithm inventory; `hash_serialize_003.phpt` still exposes exact wire-format
differences for active contexts. A focused current target run of that row still
fails because php-src expects native active-context state bytes while phrust
serializes its own internal `__phrust_*` context representation.
The pinned reference needs `PHP_MHASH_BC` and `openssl` before every selected
hash row can pass on both sides without environmental skips.

## Target Gates

- `nix develop -c cargo test -p php_runtime hash`
- `nix develop -c cargo test -p php_std hash`
- `nix develop -c just phpt-dev-module MODULE=hash`

Last upstream target sweep before HASH-1 implementation: 14 PASS, 6 SKIP, 60 FAIL.
After adding Adler-32, CRC32/CRC32C, FNV, JOAAT, SHA3, RIPEMD, MD2, MD4,
MurmurHash3, Whirlpool, GOST, MurmurHash3/xxHash seed support, Tiger 3-pass,
Tiger 4-pass, Snefru, and HAVAL support, strict hash_equals argument validation,
hash/hash_file/HMAC/HMAC-file/hash_init/PBKDF2/HKDF ValueError wording, and
MurmurHash3/xxHash seed deprecation diagnostics, direct HashContext
construction visibility, finalized-context reuse diagnostics, HashContext
debug-info output, mhash compatibility/deprecation coverage, algorithm-list
inventory rows, active HashContext cloning, serializable HashContext round trips
for supported algorithms, finalized/HMAC HashContext serialization exceptions,
malformed HashContext payload diagnostics, xxHash serialized-state memsize
validation, xxHash stringable-secret conversion diagnostics, object
`__toString` exception propagation during secret conversion, and PHPT sibling
fixture-file access, plus SensitiveParameterValue masking in hash exception
stack traces, the selected manifest contains 84 target passing rows. The latest
selected module gate reports 77 PASS / 7 SKIP on the pinned reference and 84
PASS on the target. The focused unselected `hash_serialize_003.phpt` target row
still fails with a native active-context serialized-state mismatch; that remains
the documented exact native serialization gap. The pinned reference run
currently reports environmental skips for six rows that require `PHP_MHASH_BC`,
and `hash_update_stream_basic_001.phpt` requires `openssl`.
