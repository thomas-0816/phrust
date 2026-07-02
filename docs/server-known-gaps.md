# Phrust Server Known Gaps

The web server is integrated support for simple PHP applications. It executes
through the phrust lexer, parser, frontend, runtime, VM, and request-local HTTP
context; it is not an FPM, FastCGI, CGI, Apache module, or external PHP process
adapter.

Implemented surface:

- `server-compat-smoke all` is strict for static files, nested URL-encoded
  input, bounded multipart uploads, `$_FILES`, upload movement builtins,
  cookies, persistent sessions, output-buffer basics, include execution,
  response headers/status, `php://input`, stream output, request-local
  filesystem CWD behavior, cooperative execution deadlines, and loopback cache
  invalidation.
- The server has cooperative PHP execution deadlines, process-local include and
  entry-script caches, bounded/preloaded script-cache controls, loopback-only
  cache clearing, streaming static files, validators, byte ranges,
  precompressed sidecars, config-file support, access logs, metrics token
  protection, and Rustls HTTP/1.1 TLS termination.

Remaining known gaps:

- The implemented server is an application compatibility layer, not full PHP SAPI
  compatibility. It does not emulate FPM process management, Apache module
  globals, Zend extension ABI behavior, complete INI handling, Opcache parity,
  or the full matrix of web-server environment variables.
- Multipart form uploads cover bounded scalar and array-shaped fixture cases.
  More exotic PHP upload matrix behavior remains outside current coverage.
- PHP execution deadlines are cooperative VM dispatch checks. Blocking native
  builtins are not interrupted mid-call; timeout is observed when control
  returns to VM dispatch.
- Output buffering covers common `ob_*` capture/clean/flush operations and
  basic `flush()` behavior, but callback output handlers and true HTTP chunk
  streaming are not complete.
- Header support covers common `header()`, `headers_list()`, `headers_sent()`,
  and `http_response_code()` behavior, but full PHP header edge cases are not
  complete.
- TLS termination supports Rustls HTTP/1.1 and advertises `http/1.1` through
  ALPN. HTTP/2 and HTTP/3 are not implemented.
- Static file serving streams from Tokio file I/O and supports validators,
  byte ranges, and precompressed sidecars. Sendfile is not implemented.
- The compiled script cache is process-local only.
- Cache invalidation is local to one process through an explicitly enabled
  loopback-only admin endpoint; there is no cross-process cache sharing or
  invalidation protocol.
- The server handles a bounded in-flight request set with a default limit of
  200, but it is not a complete production process manager.
- Include/require resolution is limited to deterministic allowed roots derived
  from the request script, document root, current working directory, and include
  path.
