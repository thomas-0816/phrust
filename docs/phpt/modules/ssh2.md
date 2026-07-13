# ssh2 PHPT coverage

## Verified scope

- `ssh2` extension functions, facade classes, and common constants.
- Request-local `SSH2\Session` handles without implicit network access.
- Request-local `SSH2\Sftp` handles attached to sessions.
- Credential-safe authentication failures that avoid exposing usernames,
  passwords, or key passphrases.
- Deterministic methods and fingerprint shapes for application control flow.
- libssh2-backed `ssh2_connect`, password/public-key authentication,
  `ssh2_exec`, `ssh2_sftp`, SCP send/receive, fingerprint, and disconnect when
  `PHRUST_NET_TESTS` and `PHRUST_SSH2_LIVE_ENDPOINT` explicitly match the
  requested endpoint.
- Backend-required exec, shell, tunnel, SCP, and SFTP operations fail
  explicitly when no matching live endpoint is configured.

## Known gaps

- Live SSH server tests require external environment configuration and are
  skipped by default.
- Host-key known-hosts validation and trust policy management are not complete.
- Interactive shell, tunnels, port forwarding, publickey admin functions, SSH
  agent/keyboard-interactive auth, and full SFTP filesystem mutation/stat/readlink
  operations remain future work.
