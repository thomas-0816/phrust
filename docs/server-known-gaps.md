# Phrust Server Known Gaps

The web server is an integrated MVP for simple PHP applications. It executes
through the phrust lexer, parser, frontend, runtime, VM, and request-local HTTP
context; it is not an FPM, FastCGI, CGI, Apache module, or external PHP process
adapter.

Known gaps:

- Wave 2 compatibility fixtures and `server-compat-smoke` now exist as the
  incremental harness for closing these gaps. Prompt 00 keeps future sections
  as explicit skips until their owning implementation prompts make them strict.
- Multipart form uploads are not implemented. `$_FILES` is currently empty.
- Advanced output flushing, buffering, and streaming semantics are not complete.
- Header support covers common `header()`, `headers_list()`, `headers_sent()`,
  and `http_response_code()` behavior, but full PHP header edge cases are not
  complete.
- TLS termination is not part of the MVP server.
- HTTP/2 and HTTP/3 are not part of the MVP server.
- Static file serving is simple in-memory response construction; sendfile and
  static streaming are not optimized yet.
- The compiled script cache is process-local only.
- There is no cross-process cache sharing or cache invalidation protocol.
- The server handles a bounded in-flight request set, but it is not a complete
  production process manager.
- Include/require resolution is limited to deterministic allowed roots derived
  from the request script, document root, current working directory, and include
  path.
