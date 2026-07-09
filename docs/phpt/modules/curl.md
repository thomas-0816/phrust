# curl

- Strategy: deterministic HTTP/client bootstrap slice with selected upstream
  handle, version, error-string, multi, and share-handle probes
- Selected manifest: `tests/phpt/manifests/modules/curl.selected.jsonl`
- Selected fixtures:
  - `tests/phpt/generated/wp.db-network/curl-platform-mvp.phpt`
  - `tests/phpt/generated/wp.db-network/curl-default-off.phpt`
  - `tests/phpt/generated/wp.db-network/curl-local-http.phpt`
  - `tests/phpt/generated/curl/local-post-redirect-headers.phpt`
  - `ext/curl/tests/curl_escape.phpt`
  - `ext/curl/tests/curl_multi_strerror_001.phpt`
  - `ext/curl/tests/curl_version_basic_001.phpt`
  - `ext/curl/tests/curl_strerror_001.phpt`
  - `ext/curl/tests/curl_share_setopt_basic001.phpt`
  - `ext/curl/tests/curl_share_errno_strerror_001.phpt`
  - `ext/curl/tests/curlopt_private.phpt`
  - `ext/curl/tests/bug52202.phpt`
- Current selected module gate:
  `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 PHPT_TIMEOUT_SECONDS=20 PHPT_WORK_DIR=/private/tmp/phrust-phpt-curl-selected-libcurl-version nix develop -c just phpt-dev-module MODULE=curl`
  reports reference SKIP 12 because curl is not loaded there, and target PASS
  10 / SKIP 2 under default network gating.

## Implemented Surface

The runtime exposes `curl_init`, `curl_setopt`, `curl_setopt_array`,
`curl_exec`, `curl_getinfo`, `curl_errno`, `curl_error`, `curl_close`,
`curl_copy_handle`, `curl_reset`, `curl_version`, `curl_escape`,
`curl_unescape`, `curl_strerror`, the selected `curl_multi_*` queue helpers,
and basic `curl_share_*` handle/error helpers.

The selected rows cover extension and class visibility, basic handle creation,
default-off network policy, deterministic local HTTP execution when enabled,
POST string/array bodies, outgoing headers, bounded local redirects,
response-header inclusion, header-size info, URL escaping, selected
`curl_version()` shape, multi error strings, cURL error strings, share-handle
option validation, share-handle errno readback, and share error strings.

`CURLOPT_PRIVATE` and `CURLINFO_PRIVATE` are supported as handle-local metadata
and are copied by `curl_copy_handle`; the promoted rows cover object identity
preservation and a regression where `CURLOPT_PRIVATE` must not be clobbered by
later transfer operations. Basic share handles accept `CURLSHOPT_SHARE` and
`CURLSHOPT_UNSHARE` and expose PHP-compatible invalid share option errors for
the selected upstream rows.

`curl_version()` now derives `version_number`, `features`, `version`, `host`,
`ssl_version`, `libz_version`, and `protocols` from the linked libcurl metadata
through the Rust `curl` crate. The selected `curl_version_basic_001.phpt` row
passes against the target with this dynamic metadata, and the runtime unit test
asserts that the old placeholder `phrust-curl-mvp` metadata is not reported.

## Gaps

The transfer transport remains the existing hand-rolled HTTP/OpenSSL path
rather than a libcurl-backed handle core. Full libcurl option parity, callback
dispatch, file uploads, `file://` protocol support, complete TLS option
behavior, public-network tests, async/select multi behavior, and real
share-handle data sharing remain out of scope for the selected slice.

The local reference oracle currently skips curl rows because its CLI build does
not load curl. Target PHPT results are therefore the useful evidence for this
module until the reference build gains curl support.

## Target Gates

- `nix develop -c cargo test -p php_runtime curl`
- `nix develop -c cargo build -p php_vm_cli --bin phrust-php`
- `nix develop -c just phpt-dev-module MODULE=curl`
