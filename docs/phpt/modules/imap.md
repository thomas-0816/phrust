# imap PHPT coverage

## Verified scope

- `imap` extension functions, `IMAP\Connection`, and common constants.
- Request-local connection handles without implicit network access.
- Deterministic empty mailbox behavior for headers, search, fetch, status,
  counts, and mailbox info.
- Rust IMAP backend for open, close, check, headers, fetch body/header, mailbox
  info, search, and error queues when `PHRUST_NET_TESTS` and
  `PHRUST_IMAP_LIVE_*` are explicitly configured.
- PHP mailbox connection string parsing for host, port, SSL, and
  `novalidate-cert`.
- Request-local delete and expunge state for application control flow.
- Error and alert queue behavior through `imap_last_error()`, `imap_errors()`,
  and `imap_alerts()`.
- Backend-required append, copy, and move operations fail explicitly.

## Known gaps

- c-client backend integration is not in scope; the selected live backend is
  the Rust `imap` crate.
- Live IMAP/IMAPS tests require external environment configuration and are
  skipped by default.
- MIME body parsing, message structure compatibility, server-side mutation
  persistence, and mailbox administration remain future work.
