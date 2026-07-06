# ADR 0025: Runtime semantics Destructor Queue MVP

## Status

Accepted for Runtime semantics.

## Context

Runtime objects are identity-bearing `ObjectRef` handles backed by shared Rust
storage. Running PHP destructors directly from Rust `Drop` would need to execute
VM code while object handles, call frames, arrays, and references may still hold
clones of the same handle. That is unsafe for the current runtime model and
would make double execution hard to rule out.

## Decision

Runtime semantics introduces a VM-owned `DestructorQueue` in execution state:

- objects with a public non-static `__destruct()` are registered after
  successful construction and after successful clone creation;
- each object identity is registered at most once while it is in the queue;
- request shutdown drains the queue in reverse registration order;
- destructors that create new destructible objects append them to a later drain
  batch;
- queue draining stops with `E_PHP_VM_DESTRUCTOR_QUEUE_OVERFLOW` after 4096
  destructor executions in one request;
- a destructor exception or runtime error stops shutdown and returns that
  runtime error with already produced output preserved;
- cyclic-object collection and unset-time destructor execution are explicit
  Runtime semantics known gaps.

## Consequences

The MVP is deterministic and does not rely on Rust `Drop`, so it avoids unsafe
VM reentry and double-free style hazards. It is PHP-near for simple request
shutdown programs, but it intentionally does not claim exact refcount-triggered
destruction, cycle collector behavior, destructor ordering for globals versus
locals, or shutdown interaction with generators and fibers.

## Alternatives Considered

- Execute destructors from Rust `Drop`. Rejected because PHP code would run
  while Rust ownership and VM frame state are being torn down.
- Run destructors immediately on `unset()`. Rejected for Runtime semantics because exact
  refcount and cycle-collector timing is not implemented.
- Ignore destructors until Standard library. Rejected because object lifetime is visible
  enough to affect Runtime semantics object, clone, exception, and shutdown fixtures.

## Standard library Follow-up

Standard library must decide unset-time destruction, cycle-collection destruction,
shutdown ordering for globals/statics, destructor interaction with suspended
generators/fibers, and exact error-object behavior when destructors throw.
