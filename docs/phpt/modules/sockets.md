# sockets PHPT coverage

The `sockets` module currently covers a deterministic loopback/local stream
socket slice:

- `Socket` object visibility and invalid `socket_create()` behavior.
- `socket_create()`, `socket_bind()`, `socket_listen()`, `socket_connect()`,
  `socket_accept()`, and `socket_close()` for IPv4 TCP loopback and Unix-domain
  stream sockets where the platform exposes `AF_UNIX`.
- `socket_read()`/`socket_write()` and `socket_recv()`/`socket_send()`.
- `socket_getsockname()` and `socket_getpeername()` for loopback TCP sockets,
  plus Unix-domain local socket name reporting.
- `socket_shutdown()`, `socket_last_error()`, `socket_clear_error()`, and
  `socket_strerror()`.
- `inet_pton()`/`inet_ntop()` roundtrips for IPv4 and IPv6 packed addresses.

The selected PHPT stays loopback/local-only and does not require external
network access. Broader parity remains intentionally outside this slice: UDP,
`socket_select()`, nonblocking and timeout behavior, ancillary data, addrinfo
helpers, stream import/export, and platform-specific Windows helpers.
