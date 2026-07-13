# ldap PHPT coverage

## Verified scope

- `ldap` extension function, class, and stable constant registration.
- Request-local `LDAP\Connection`, `LDAP\Result`, and `LDAP\ResultEntry`
  handles without implicit network access.
- Common option set/get behavior, including protocol and TLS policy options.
- Error state through `ldap_errno()`, `ldap_error()`, and `ldap_err2str()`.
- String helpers including `ldap_escape()`, `ldap_explode_dn()`, and
  `ldap_dn2ufn()`.
- Deterministic empty results for read, list, search, entry counts, and entry
  traversal.
- `ldap3` bind and search are wired behind explicit `PHRUST_NET_TESTS` and
  `PHRUST_LDAP_LIVE_URI` opt-in configuration.
- Backend-required bind, mutation, compare, extended operation, and TLS calls
  fail explicitly when no matching live endpoint is configured.

## Known gaps

- OpenLDAP/libldap FFI backend integration is not in scope; the selected live
  backend is the `ldap3` Rust client.
- Live LDAP tests require external environment configuration and are skipped by
  default.
- SASL, referrals, controls parsing, paged results, and TLS handshakes remain
  outside the selected deterministic facade.
