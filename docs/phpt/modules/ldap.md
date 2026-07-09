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
- Backend-required bind, mutation, compare, extended operation, and TLS calls
  fail explicitly.

## Known gaps

- No OpenLDAP/libldap backend is connected.
- Live LDAP DSN integration tests are not promoted.
- SASL, referrals, controls parsing, paged results, and TLS handshakes remain
  outside the selected deterministic facade.
