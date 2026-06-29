# openssl

- Strategy: deterministic crypto helper MVP
- Selected manifest: `tests/phpt/manifests/modules/openssl.selected.jsonl`
- Selected gate: 2 generated PHPTs shared with `wp.db-network`

## Implemented Surface

The runtime exposes the `openssl` extension and selected helpers used by
application update, hashing, and security probes:

- `openssl_random_pseudo_bytes`
- `openssl_digest`
- `openssl_get_md_methods`
- `openssl_verify`
- `OPENSSL_ALGO_SHA256`

Digest support is implemented for the selected hash families backed by Rust
digest crates. `openssl_random_pseudo_bytes()` uses OS randomness.
`openssl_verify()` intentionally returns the explicit unsupported verification
result covered by the selected fixture; it does not fake signature validation.

## Gaps

Certificate parsing, key loading, signature verification parity,
`openssl_encrypt`, `openssl_decrypt`, stream TLS contexts, host OpenSSL
configuration, and certificate-store behavior remain unsupported until backed by
deterministic fixtures and an approved dependency strategy.

## Source References

- `ext/openssl/openssl.stub.php`
- `ext/openssl/tests/`

## Target Gates

- `nix develop -c cargo test -p php_runtime openssl`
- `nix develop -c just phpt-dev-module MODULE=openssl`
- `nix develop -c just phpt-dev-module MODULE=closure.extensions`
