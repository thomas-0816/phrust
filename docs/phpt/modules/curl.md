# curl

- Strategy: deterministic HTTP client MVP
- Selected manifest: `tests/phpt/manifests/modules/curl.selected.jsonl`
- Selected gate: 4 generated PHPTs shared with `wp.db-network` and the
  closure cURL transport slice

## Implemented Surface

The runtime exposes `curl`, `CurlHandle`, and the selected WordPress-style HTTP
client helpers:

- `curl_version`
- `curl_init`
- `curl_setopt`
- `curl_exec`
- `curl_getinfo`
- `curl_errno`
- `curl_error`
- `curl_close`

Network execution is disabled by default. `curl_exec()` returns `false` and sets
a deterministic error unless `PHRUST_NET_TESTS=1` is present. The selected live
HTTP fixture is also gated by `PHRUST_CURL_TEST_URL`, which must point at a
local deterministic server.

The local HTTP transport also supports selected application options:

- `CURLOPT_FOLLOWLOCATION` for bounded same-host local redirects.
- `CURLOPT_HTTPHEADER` for outgoing request headers.
- `CURLOPT_POSTFIELDS` for string bodies and simple form-encoded arrays.
- `CURLOPT_HEADER` and `CURLINFO_HEADER_SIZE` for response-header inclusion.
- `CURLINFO_RESPONSE_CODE`, `CURLINFO_EFFECTIVE_URL`, and
  `CURLINFO_TOTAL_TIME`.

## Gaps

HTTPS/TLS transport remains unsupported in `php_runtime` until a TLS stack is
selected for that crate. `curl_multi_*`, proxy behavior, authentication, upload
streaming, multipart file upload parity, and the full libcurl option matrix
remain unsupported. Public internet endpoints are not part of the default gate.

## Source References

- `ext/curl/interface.stub.php`
- `ext/curl/tests/`

## Target Gates

- `nix develop -c cargo test -p php_runtime curl`
- `PHRUST_NET_TESTS=1 nix develop -c cargo test -p php_runtime curl`
- `nix develop -c just phpt-dev-module MODULE=curl`
- `nix develop -c just phpt-dev-module MODULE=closure.extensions`
