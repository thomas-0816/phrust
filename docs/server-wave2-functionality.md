# Server Wave 2 Functionality Plan

Wave 2 builds on the integrated web-server MVP with unmodified PHP application
functionality first, then server speed and hardening. The server architecture
remains direct and integrated: Hyper/Tokio accepts HTTP requests, `php_server`
routes them, `php_executor` compiles and executes PHP in-process through phrust
crates, and the response is emitted directly by the server.

The server must not use FPM, FastCGI, CGI, Apache module behavior, `mod_php`,
external PHP subprocesses, external PHP worker sockets, or a replacement web
framework stack in the hot path.

## Scope

This wave is organized as a serial set of prompts. Each prompt lands with its
own focused fixtures, tests, and validation gates before the next prompt starts.

Planned functionality:

- A compatibility fixture app and `server-compat-smoke` harness for incremental
  app-surface checks.
- PHP-compatible URL-encoded input array construction for `$_GET`, `$_POST`,
  and `$_REQUEST`.
- Bounded multipart parsing and populated `$_FILES`.
- Upload builtins: `is_uploaded_file()` and `move_uploaded_file()`.
- Cookie emission through `setcookie()` and `setrawcookie()`.
- Persistent web sessions backed by integrated server storage.
- Output-buffering builtins wired to the existing VM output-buffer stack.
- PHP execution deadlines and `set_time_limit()` integration.
- Include/realpath and compiled-include caching for hot applications.
- Bounded script cache behavior, preload, anti-stampede protection, and safe
  cache invalidation.
- Static file streaming, conditional requests, ranges, and precompressed asset
  selection.
- Production-oriented config, access logs, metrics hardening, TLS, and optional
  HTTP/2 transport.

## Out Of Scope

Wave 2 does not introduce FPM, FastCGI, CGI, Apache modules, `mod_php`,
external PHP process execution, Zend ABI emulation, a complete SAPI
compatibility layer, HTTP/3, Opcache parity, a full standard library, or a
production process manager.

Known gaps should stay explicit until implemented and verified. For example,
`server-compat-smoke` starts as a compatibility framework in Prompt 00. Future
sections are intentionally skipped until the prompt that owns that behavior
makes the section strict.

## Compatibility Harness

The compatibility app lives under `fixtures/server/apps/compat/`. The harness
can run named sections:

- `static`
- `input`
- `upload`
- `cookie`
- `session`
- `output-buffer`
- `all`

Prompt 00 only makes `static` strict. Later prompts make their corresponding
sections strict as support lands.

## Expected Acceptance Commands

Prompt 00 baseline:

```bash
nix develop -c cargo fmt --all --check
nix develop -c cargo clippy -p php_server -p php_executor -p php_runtime --all-targets -- -D warnings
nix develop -c cargo test -p php_server
nix develop -c bash scripts/server/compat_smoke.sh static
nix develop -c just server-smoke
nix develop -c rg "FastCGI|php-fpm|mod_php|CGI|std::process::Command|Command::new" crates/php_server crates/php_executor docs README.md
```

The full wave ends with the broader final integration gates documented in the
serial prompt pack.
