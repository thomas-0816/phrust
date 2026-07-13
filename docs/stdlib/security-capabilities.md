# Standard library Security Capabilities

Standard library adds standard-library and Composer-facing APIs without widening host
access by default. The reference target is PHP 8.5.7 (`php-8.5.7`), but
compatibility is bounded by deterministic local execution.

## Default-Off Capabilities

- process execution: `proc_open`, `proc_close`, `proc_get_status`, `popen`,
  `pclose`, `exec`, `passthru`, `system`, and `shell_exec`
- network streams and remote URLs
- TLS/curl/openssl behavior
- local file reads or writes outside explicit allowed roots
- online Composer or Packagist access

## Allowed By Default

- deterministic local fixture reads and writes under explicit allowed roots
- deterministic request-local `php://memory` and `php://temp` buffers
- controlled `php://stdin`, `php://stdout`, and `php://stderr` buffers when a
  test enables the stdio capability explicitly
- local Composer source-mode fixtures
- OS-backed randomness through `random_bytes` and `random_int`; this is not a
  host filesystem, network, or process capability and tests assert only output
  shape/range

The standard library registers those process/shell names so Composer-style probes see a
defined surface. Default runtime contexts do not launch a host shell; each API
returns a deterministic PHP-visible failure value and emits
`E_PHP_VM_PROCESS_CAPABILITY_DISABLED`. Isolated VM tests may install a process
mock for `shell_exec`, `exec`, `system`, and `passthru`; the mock returns fixed
output and exit status without spawning a process. `proc_*` and `popen` resource
semantics remain a known gap until a later resource-backed capability.

Capability tests must be isolated, explicit, and run through:

```bash
nix develop -c just verify-stdlib
nix develop -c just process-capability-smoke
```

Unsupported or disabled capabilities must report deterministic diagnostics
instead of panicking or silently succeeding.

Local file APIs (`fopen`, `file_get_contents`,
`file_put_contents`, `copy`, `rename`, `unlink`, `mkdir`, `rmdir`, `touch`,
`tempnam`, and `tmpfile`) use the same allowed-root check. Paths outside the
request capability return PHP-visible failure values and do not fall back to
ambient host access.

Directory APIs (`opendir`, `scandir`, `glob`, and
`chdir`) also require allowed roots. Glob expansion enumerates only the
capability-approved local directory selected by the normalized pattern prefix.

Stream helpers preserve context options locally and keep
include-path resolution capability-checked. `stream_isatty` is deterministic and
does not probe host terminal state unless a later capability explicitly adds
that behavior.
