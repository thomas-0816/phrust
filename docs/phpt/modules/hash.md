# hash

- Strategy: selected upstream digest, HMAC, and streaming context parity slice
- Selected manifest: `tests/phpt/manifests/modules/hash.selected.jsonl`
- Selected fixtures:
  - `tests/phpt/generated/hash/file.phpt`
  - `tests/phpt/generated/hash/context.phpt`
  - selected `ext/hash/tests/*.phpt` rows for adler32, CRC/FNV/JOOAT,
    MD2/MD4, md5, MurmurHash3, RIPEMD, sha1, SHA-2/SHA3, Whirlpool,
    HMAC-md5, file hashing, HKDF, PBKDF2, and stream updates

## Implemented Surface

The runtime exposes the existing `hash`, `hash_hmac`, `hash_algos`,
`hash_hmac_algos`, and `hash_equals` builtins plus `hash_file`,
`hash_hmac_file`, `hash_init`, `hash_update`, `hash_copy`, and `hash_final`.
The metadata registry exposes `HashContext` and `HASH_HMAC`.

Supported digest/HMAC algorithms in this slice are `md2`, `md4`, `md5`,
`sha1`, `sha224`, `sha256`, `sha384`, `sha512/224`, `sha512/256`, `sha512`,
`sha3-224`, `sha3-256`, `sha3-384`, `sha3-512`, `ripemd128`, `ripemd160`,
`ripemd256`, `ripemd320`, and `whirlpool`; digest-only coverage also includes
`adler32`, `crc32`, `crc32b`, `crc32c`, `fnv132`, `fnv1a32`, `fnv164`,
`fnv1a64`, `joaat`, `murmur3a`, `murmur3c`, and `murmur3f`.

The selected rows cover SHA-256 file digests, SHA-256 file HMACs, raw binary
file digest output, upstream adler32/CRC/FNV/JOOAT/MD2/MD4/md5/MurmurHash3,
RIPEMD, sha1/SHA-2/SHA3/Whirlpool digest vectors, HMAC-md5, upstream file
hashing, `hash_update_file`, `hash_update_stream`, `hash_pbkdf2`, and RFC5869
`hash_hkdf` vectors. The context row covers `HashContext` visibility,
incremental SHA-256 hashing, copying a partially updated context, HMAC contexts,
and finalized context rejection.

## Gaps

The full php-src hash algorithm inventory remains out of scope for this slice:
`xxh*`, `haval*`, `tiger*`, `snefru*`, and `gost*` are still unsupported.
HashContext magic serialization/debug-info parity is also not complete.
Murmur/xxHash seed and options-array rows remain unpromoted until the `hash`
and `hash_init` options parameter is implemented.

## Target Gates

- `nix develop -c cargo test -p php_runtime hash`
- `nix develop -c cargo test -p php_std hash`
- `nix develop -c just phpt-dev-module MODULE=hash`

Last upstream target sweep before this promotion: 14 PASS, 6 SKIP, 60 FAIL.
After adding Adler-32, CRC32/CRC32C, FNV, JOAAT, SHA3, RIPEMD, MD2, MD4, and
MurmurHash3, and Whirlpool, the selected manifest contains 35 green rows.
