# openssl

- Strategy: deterministic OpenSSL helper slice for application crypto probes,
  with upstream digest, random, cipher metadata, AES-CBC, and warning/error
  rows promoted as target evidence
- Selected manifest: `tests/phpt/manifests/modules/openssl.selected.jsonl`
- Selected fixtures:
  - `tests/phpt/generated/wp.db-network/openssl-platform-mvp.phpt`
  - `tests/phpt/generated/wp.db-network/openssl-helpers-mvp.phpt`
  - `tests/phpt/generated/wp.db-network/openssl-encrypt-decrypt-mvp.phpt`
  - `ext/openssl/tests/openssl_get_cipher_methods.phpt`
  - `ext/openssl/tests/gh19994.phpt`
  - `ext/openssl/tests/openssl_digest_basic.phpt`
  - `ext/openssl/tests/openssl_cipher_iv_length_basic.phpt`
  - `ext/openssl/tests/openssl_cipher_key_length_basic.phpt`
  - `ext/openssl/tests/openssl_random_pseudo_bytes_basic.phpt`
  - `ext/openssl/tests/openssl_encrypt_cbc.phpt`
  - `ext/openssl/tests/openssl_decrypt_basic.phpt`
  - `ext/openssl/tests/openssl_encrypt_error.phpt`
- Current selected module gate: the pinned reference build skips rows when its
  CLI does not load openssl; the phrust target reports 12 PASS rows.

## Implemented Surface

The runtime exposes selected OpenSSL helpers: `openssl_digest`,
`openssl_get_md_methods`, `openssl_get_cipher_methods`,
`openssl_cipher_iv_length`, `openssl_cipher_key_length`,
`openssl_random_pseudo_bytes`, `openssl_encrypt`, `openssl_decrypt`,
`openssl_error_string`, `openssl_pkey_get_public`,
`openssl_get_publickey`, and `openssl_verify`.

`openssl_digest` uses the Rust `openssl` crate `MessageDigest` backend for the
selected digest algorithms. AES-128-CBC and AES-256-CBC are supported for
selected encrypt/decrypt paths, including the `aes128` alias, raw/base64
output, zero-padding mode, key and IV metadata queries, and generated named
argument handling for the selected `openssl_decrypt(..., tag: ...)` rows.

Cipher errors queue `openssl_error_string()` messages and emit PHP warnings for
the promoted upstream error row. `openssl_verify` supports selected PEM public
keys and X509 certificates for RSA/SHA digest verification.

## Gaps

The selected slice is not a complete OpenSSL extension. Certificate/key object
models, private-key parsing and export, `openssl_sign`, CSR, PKCS7, PKCS12,
CMS, AEAD modes, PSS padding, complete host OpenSSL method enumeration, and
TLS stream/curl integration remain known gaps.

The local reference oracle currently may skip openssl rows because its CLI
build does not load the extension. Target PHPT results are therefore the useful
evidence for this module until the reference build gains openssl support.

## Target Gates

- `nix develop -c cargo test -p php_runtime openssl`
- `nix develop -c cargo build -p php_vm_cli --bin phrust-php`
- `nix develop -c just phpt-dev-module MODULE=openssl`
- `nix develop -c just phpt-dev-module MODULE=closure.extensions`
