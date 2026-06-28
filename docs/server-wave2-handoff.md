# Server Wave 2 Handoff

## Implemented

- Strict compatibility smoke coverage for static files, nested request input,
  multipart uploads, upload movement builtins, cookies, persistent sessions,
  output-buffer basics, and include execution.
- Cooperative PHP execution deadlines with timeout metrics and `set_time_limit()`
  integration for web requests.
- Process-local include resolution/compile caching and bounded entry-script
  caching with preload, metadata checks, anti-stampede behavior, metrics, and a
  loopback-only cache-clear endpoint.
- Streaming static responses with `HEAD`, weak ETags, `Last-Modified`,
  conditional `304`, byte ranges, invalid range `416`, and precompressed
  `.br`, `.zst`, and `.gz` sidecars.
- Production-oriented configuration: optional config file, CLI overrides,
  access logs, metrics token protection, startup diagnostics, and Rustls
  HTTP/1.1 TLS termination.

The server remains integrated and in-process. Request handling calls phrust
crates directly; it does not use FPM, FastCGI, CGI, Apache module behavior,
`mod_php`, external PHP subprocesses, or external worker sockets in the hot
path.

## Commands

Prompt-level gates passed while building this wave:

```bash
nix develop -c cargo fmt --all --check
nix develop -c cargo clippy -p php_server --all-targets -- -D warnings
nix develop -c cargo test -p php_server
nix develop -c just server-smoke
nix develop -c just server-compat-smoke all
nix develop -c just server-benchmark-smoke
nix develop -c just server-tls-smoke
```

Final Prompt 13 gates passed on 2026-06-28:

```bash
nix develop -c just fmt
nix develop -c cargo clippy --workspace --all-targets -- -D warnings
nix develop -c cargo test --workspace
nix develop -c just verify-runtime
nix develop -c just verify-stdlib
nix develop -c just server-smoke
nix develop -c just server-compat-smoke all
nix develop -c just server-benchmark-smoke
rg "FastCGI|php-fpm|mod_php|CGI|std::process::Command|Command::new" crates/php_server crates/php_executor docs README.md
```

The fallback audit produced documentation-policy matches only. There were no
matches in production server or executor source files.

## Benchmark Note

Local sample, 2026-06-28, macOS/Darwin development host, release
`phrust-server`, loopback, sequential `curl`, 20 samples per path after warmup.
These numbers are a smoke comparison, not a load-test claim:

| Path | Mean |
| --- | ---: |
| Static streaming `/static.txt` | 0.360 ms |
| Warm PHP entry script `/entry.php` | 0.427 ms |
| Warm front controller with `require` include | 0.444 ms |
| Small multipart upload | 0.601 ms |
| Session read/write with existing cookie | 0.693 ms |

The relative order matched expectations on this host: static is cheapest, warm
entry/front-controller paths are close after caching, and upload/session paths
pay extra request parsing and filesystem work. Re-run on target hardware before
using these values for capacity planning.

## Caveats

- HTTP/2 and HTTP/3 are not implemented.
- TLS is Rustls HTTP/1.1 with `http/1.1` ALPN.
- Sendfile is not implemented; static files stream through Tokio file I/O.
- Caches and sessions are process-local. There is no cross-process cache
  invalidation or session locking.
- The compatibility surface is MVP web-app compatibility, not full PHP SAPI,
  FPM, Apache module, Zend ABI, extension ABI, or Opcache compatibility.
