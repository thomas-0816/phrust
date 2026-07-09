# imap PHPT coverage

## Verified scope

- `imap` extension functions, `IMAP\Connection`, and common constants.
- Request-local connection handles without implicit network access.
- Deterministic empty mailbox behavior for headers, search, fetch, status,
  counts, and mailbox info.
- Request-local delete and expunge state for application control flow.
- Error and alert queue behavior through `imap_last_error()`, `imap_errors()`,
  and `imap_alerts()`.
- Backend-required append, copy, and move operations fail explicitly.

## Known gaps

- No c-client or IMAP crate backend is connected.
- Live IMAP/IMAPS server integration tests are not promoted.
- TLS, authentication, MIME body parsing, server-side search, and persistent
  mailbox mutation remain future work.
