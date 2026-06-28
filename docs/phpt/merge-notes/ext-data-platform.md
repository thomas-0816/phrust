# ext-data-platform merge notes

Branch prompt set: Branch 4 - Data, PHAR, Session, Opcache, SAPI, GD.

## Database Policy

| Module | Decision | Classification | Implemented surface |
| --- | --- | --- | --- |
| `pdo` | Keep unavailable; no PDO core in this branch. | optional | Negative platform/class probes only. |
| `pdo_sqlite` | No tiny MVP until real PDO and SQLite semantics exist. | real-implementation-required | Negative platform/class probes only. |
| `sqlite3` | No in-memory MVP without an approved SQLite dependency and real query semantics. | real-implementation-required | Negative platform/class probes only. |
| `mysqli` | Network DB support is out of scope. | out-of-scope | Negative platform/class probes only. |
| `mysqlnd` | Native MySQL driver work is out of scope and owned by future MySQL support. | out-of-scope | Negative platform probe only. |

## PHAR Decision

Composer source mode is sufficient for the current compatibility target. PHAR
support remains a future real implementation because even read-only `phar://`
requires archive parsing, wrapper integration, stub handling, and a signing
policy. No PHAR runtime code was added.

## Session Decision

Session remains unavailable. A CLI-only MVP is possible later, but must be
implemented through request-local runtime state, superglobals, INI behavior,
serialization, and deterministic storage policy. This branch does not fake
`$_SESSION` or `session_start`.

## Opcache, SAPI, and GD Policy

| Module | Decision | Classification | Notes |
| --- | --- | --- | --- |
| `opcache` | Do not implement Opcache or JIT. | out-of-scope | Ordinary PHP behavior found in opcache-located tests should move to its owning module when minimized. |
| `sapi` | Keep only CLI-compatible behavior in scope. | out-of-scope outside CLI | FPM, FastCGI, Apache, CGI, and phpdbg remain out of scope. |
| `gd` | Do not implement image processing. | out-of-scope | Requires a future graphics dependency and binary output policy. |

## Merge Risks

- The selected module gates prove current negative platform behavior, not real
  database, PHAR, session, SAPI, Opcache, or GD support.
- The full PHPT baseline is unchanged; this branch does not accept a new
  baseline or remove extension failures from accounting.
- Future implementation branches must replace the relevant negative platform
  probes with real behavior and module-specific PHPTs.
