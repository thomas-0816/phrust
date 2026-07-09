# sodium

- Strategy: deterministic sodium helper slice covering extension visibility,
  version metadata, existing generichash/signature/encoding helpers, and
  upstream hex conversion behavior
- Selected manifest: `tests/phpt/manifests/modules/sodium.selected.jsonl`
- Selected fixtures:
  - `tests/phpt/generated/sodium/basic.phpt`
  - `ext/sodium/tests/installed.phpt`
  - `ext/sodium/tests/version.phpt`
  - `ext/sodium/tests/crypto_hex.phpt`
- Current selected module gate: 4 selected rows, 0 non-green outcomes.

## Implemented Surface

The runtime exposes `sodium_bin2hex`, `sodium_hex2bin`,
`sodium_bin2base64`, `sodium_base642bin`,
`sodium_crypto_generichash`, `sodium_crypto_generichash_keygen`,
`sodium_crypto_sign_detached`, and
`sodium_crypto_sign_verify_detached`.

The stdlib descriptor exposes the selected base64, generichash, and signature
constants plus `SODIUM_LIBRARY_VERSION`,
`SODIUM_LIBRARY_MAJOR_VERSION`, and `SODIUM_LIBRARY_MINOR_VERSION` for
PHP-visible version probes.

## Gaps

This is still the existing pure-Rust sodium subset, not the prompt-pack target
of a libsodium FFI-backed implementation. Secretbox, password hashing, AEAD,
key exchange, keypair extraction, scalar multiplication, secure memory helpers,
increment/add/compare helpers, and stream APIs remain known gaps.

## Target Gates

- `nix develop -c cargo test -p php_runtime sodium`
- `nix develop -c cargo build -p php_vm_cli --bin phrust-php`
- `nix develop -c just phpt-dev-module MODULE=sodium`
- `nix develop -c just phpt-dev-module MODULE=closure.extensions`
