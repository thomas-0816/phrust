# hash

- Strategy: selected upstream digest, HMAC, and streaming context parity slice
- Selected manifest: `tests/phpt/manifests/modules/hash.selected.jsonl`
- Selected fixtures:
  - `tests/phpt/generated/hash/file.phpt`
  - `tests/phpt/generated/hash/context.phpt`
  - selected `ext/hash/tests/*.phpt` rows for adler32, CRC/FNV/JOOAT,
    MD2/MD4, md5, MurmurHash3, RIPEMD, sha1, SHA-2/SHA3, Tiger 3-pass,
    Whirlpool, MurmurHash3/xxHash seed options, HMAC-md5, file hashing, HKDF,
    PBKDF2, and stream updates

## Implemented Surface

The runtime exposes the existing `hash`, `hash_hmac`, `hash_algos`,
`hash_hmac_algos`, and `hash_equals` builtins plus `hash_file`,
`hash_hmac_file`, `hash_init`, `hash_update`, `hash_copy`, and `hash_final`.
The metadata registry exposes `HashContext` and `HASH_HMAC`.

Supported digest/HMAC algorithms in this slice are `md2`, `md4`, `md5`,
`sha1`, `sha224`, `sha256`, `sha384`, `sha512/224`, `sha512/256`, `sha512`,
`sha3-224`, `sha3-256`, `sha3-384`, `sha3-512`, `ripemd128`, `ripemd160`,
`ripemd256`, `ripemd320`, `tiger128,3`, `tiger160,3`, `tiger192,3`,
`whirlpool`, `gost`, and `gost-crypto`;
digest-only coverage also includes `adler32`, `crc32`, `crc32b`, `crc32c`,
`fnv132`, `fnv1a32`, `fnv164`, `fnv1a64`, `joaat`, `murmur3a`, `murmur3c`,
`murmur3f`, `xxh32`, `xxh64`, `xxh3`, and `xxh128`.

The selected rows cover SHA-256 file digests, SHA-256 file HMACs, raw binary
file digest output, upstream adler32/CRC/FNV/JOOAT/MD2/MD4/md5/MurmurHash3,
GOST, RIPEMD, sha1/SHA-2/SHA3/Tiger 3-pass/Whirlpool digest vectors,
MurmurHash3/xxHash seeded one-shot and incremental vectors, HMAC-md5,
upstream file hashing,
`hash_update_file`, `hash_update_stream`, `hash_pbkdf2`, and RFC5869
`hash_hkdf` vectors. The context row covers `HashContext` visibility,
incremental SHA-256 hashing, copying a partially updated context, HMAC contexts,
and finalized context rejection.

## Gaps

The full php-src hash algorithm inventory remains out of scope for this slice:
`haval*`, `tiger*,4`, and `snefru*` are still unsupported. HashContext magic
serialization/debug-info parity is also not complete. The Murmur/xxHash
seed deprecation, xxHash secret/deprecation, and serialization rows remain
unpromoted until non-int seed diagnostics, non-string secret conversion
diagnostics, and HashContext serialized state parity match php-src.

## Target Gates

- `nix develop -c cargo test -p php_runtime hash`
- `nix develop -c cargo test -p php_std hash`
- `nix develop -c just phpt-dev-module MODULE=hash`

Last upstream target sweep before this promotion: 14 PASS, 6 SKIP, 60 FAIL.
After adding Adler-32, CRC32/CRC32C, FNV, JOAAT, SHA3, RIPEMD, MD2, MD4,
MurmurHash3, Whirlpool, GOST, MurmurHash3/xxHash seed support, and Tiger
3-pass support, the selected manifest contains 39 green rows.
