# ftp PHPT coverage

## Verified scope

- `ftp` extension function registration and `FTP\Connection` class visibility.
- PHP-src FTP constants used by common application control flow.
- Default-disabled `ftp_connect()` and `ftp_ssl_connect()` behavior for normal
  PHP calls without an opt-in backend.
- Loopback-only control-channel behavior is covered by Rust fake-server tests:
  login, current directory, directory changes, raw commands, close, and quit.
- Loopback passive listing and local filename transfers are covered in runtime
  tests for the deterministic backend.
- Directory, metadata, SITE, ALLO, chmod, rename, delete, and option APIs are
  represented in the runtime facade.

## Known gaps

- External network FTP is not enabled by default.
- TLS/SSL FTP is not implemented.
- Active data channels are not implemented.
- Stream-resource transfers and true asynchronous transfer progress remain
  future promotion work.
