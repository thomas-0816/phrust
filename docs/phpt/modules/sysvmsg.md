# sysvmsg PHPT coverage

Current focused coverage:

- Selected rows: 8 (`tests/phpt/generated/sysvmsg/basic.phpt` plus upstream
  `001.phpt`, `002.phpt`, `003.phpt`, `004.phpt`, `005.phpt`, `006.phpt`,
  and `gh16592.phpt`).
- Full upstream target sweep on this host: 7 PASS / 0 FAIL.
- `sysvmsg` extension visibility, `SysvMessageQueue` class visibility, and
  function registration.
- Request-local queue lookup and `msg_queue_exists()` behavior.
- `msg_send()` and `msg_receive()` for serialized PHP values and raw string,
  integer, float, and boolean payloads.
- Corrupted raw message receive when PHP unserialization is requested.
- By-reference receive outputs for message type, message value, and error code.
- `msg_stat_queue()`, `msg_set_queue()`, `msg_remove_queue()`, and queue
  removal visibility, including `msg_perm.uid` and `msg_perm.gid` metadata.
- `SysvMessageQueue` object dumps do not expose internal queue ids.
- `msg_send()` propagates PHP-compatible `TypeError` text when object
  serialization fails because `__serialize()` does not return an array.
- Removed queue operations preserve the handle while returning `false`, setting
  errno, and emitting the PHP-compatible `msg_send()` warning.

This slice uses deterministic request-local queues for isolated tests. Remaining
gaps are cross-process System V queues, host kernel queue IDs, permission
enforcement, platform errno-specific warning text, and blocking wait behavior.

Focused gate:

```bash
PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=sysvmsg
```
