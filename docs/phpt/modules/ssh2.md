# ssh2 PHPT coverage

## Verified scope

- `ssh2` extension functions, facade classes, and common constants.
- Request-local `SSH2\Session` handles without implicit network access.
- Request-local `SSH2\Sftp` handles attached to sessions.
- Credential-safe authentication failures that avoid exposing usernames,
  passwords, or key passphrases.
- Deterministic methods and fingerprint shapes for application control flow.
- Backend-required exec, shell, tunnel, SCP, and SFTP operations fail
  explicitly.

## Known gaps

- No libssh2 backend is connected.
- Live SSH server integration tests are not promoted.
- Real authentication, command execution, channels, forwarding, SCP, SFTP
  filesystem mutation, and host key validation remain future work.
