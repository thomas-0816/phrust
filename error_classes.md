# Phrust Error Classes Found During WordPress Bring-Up

This report groups the recurring classes of phrust failures observed while
running WordPress locally against the Docker Compose MariaDB service. It is not
a list of individual bugs or a chronological debug log.

## Frontend and Lowering Gaps

These failures happen before PHP code can execute. The source parses and is
valid PHP, but phrust cannot yet lower the resulting syntax or HIR into IR.

Common shapes in this class include:

- Assignment expressions used inside larger expressions.
- Null-coalescing and conditional assignment idioms.
- Dynamic property, array, or object-access expressions in conditions.
- Destructuring forms that use string keys or nested targets.

These errors usually surface as unsupported HIR or IR diagnostics. The fix is
to teach the frontend or IR lowerer the PHP expression shape while preserving
PHP evaluation order and side effects.

## Dynamic Loading and Autoload Gaps

These failures happen when WordPress relies on PHP's normal class loading
behavior. WordPress registers autoloaders and then expects class, parent class,
interface, and trait dependencies to become available as code is executed.

Common shapes in this class include:

- Parent classes required by an already loaded child class.
- Inherited methods declared in a class loaded by a different required file.
- Static calls that resolve through dynamically loaded class hierarchies.
- Interface or trait checks that need autoload side effects before lookup.

These errors often appear as missing class or missing parent class diagnostics
even though the relevant autoloader can load the file. The fix is usually to
make lookup and validation use the runtime class table, not only the currently
compiled unit.

## Runtime Dispatch and Resolution Gaps

These failures happen after code is lowered, when the VM has to apply PHP's
method, function, property, and constant resolution rules.

Common shapes in this class include:

- Static method calls inherited from a parent class.
- `self`, `static`, and parent-sensitive class resolution.
- Magic fallback behavior such as `__call` and `__callStatic`.
- Visibility checks that depend on the calling scope and loaded hierarchy.

These errors may look like unknown method, invalid static scope, wrong class
context, or visibility failures. Fixes generally belong in VM dispatch and
lookup code, with state-aware resolution where includes or autoloaders can add
classes at runtime.

## Runtime Semantics Gaps

These failures happen when phrust executes valid code but does not yet model
the PHP behavior closely enough for WordPress.

Common shapes in this class include:

- Truthiness and null checks in compound control-flow expressions.
- Copy, reference, and by-reference argument behavior.
- Array write, read, and destructuring semantics.
- Static locals, class statics, and persistent per-request state.

These failures can produce incorrect results, fatal VM errors, or later
secondary failures in unrelated-looking WordPress code. The fix is to implement
the PHP-visible behavior directly, preferably with a focused oracle-backed
fixture.

## Builtin and Standard Library Coverage Gaps

WordPress uses a broad PHP runtime surface. Some failures come from functions,
classes, constants, or extensions that are missing or incomplete in phrust.

Common shapes in this class include:

- Core functions used during bootstrap and configuration loading.
- Array, string, path, serialization, callback, and reflection helpers.
- MySQLi behavior needed to talk to the MariaDB service.
- Extension visibility and feature-detection functions such as
  `function_exists`, `class_exists`, `interface_exists`, and
  `extension_loaded`.

These errors are best fixed at the owning builtin or standard-library layer,
not by special-casing WordPress.

## Web Server and Request Environment Gaps

These failures come from differences between CLI-style execution and a real web
request environment. WordPress depends on request globals, paths, headers,
cookies, uploads, sessions, status handling, and response metadata.

Common shapes in this class include:

- Incorrect or incomplete `$_SERVER`, `$_GET`, `$_POST`, `$_COOKIE`, or
  request-body state.
- Script resolution, document-root, and path-info differences.
- Header, status-code, redirect, cookie, or output-buffering behavior.
- Timeout and error-reporting behavior that hides the real PHP failure.

These errors should be fixed in the web server integration or SAPI-style
request setup, while keeping the VM and runtime layers generic.

## Database Integration Gaps

These failures happen once WordPress reaches installation and talks to MariaDB.
They are distinct from Docker availability problems: the database service can
be running while phrust still handles the PHP database API incorrectly.

Common shapes in this class include:

- MySQLi connection and error-reporting differences.
- Result-set, prepared-statement, escaping, and type-conversion behavior.
- Charset, collation, socket, and host-port option handling.
- WordPress installation queries depending on exact PHP extension semantics.

These fixes belong in the database and MySQLi runtime modules, validated
against the Docker Compose MariaDB service.

## Error Handling and Diagnostics Gaps

Some failures are primarily about how phrust reports, routes, or recovers from
errors, not the original PHP operation.

Common shapes in this class include:

- Fatal errors raised where PHP would throw a catchable `Error` or exception.
- Missing source locations or stack frames in diagnostics.
- Autoload or include failures without enough context to identify the path.
- Secondary errors masking the first real unsupported behavior.

Improving this class makes the remaining WordPress bring-up work faster and
less ambiguous, even when it does not directly add new PHP behavior.

## External Environment Failures

These are not phrust language or runtime bugs, but they affect the WordPress
bring-up loop and need to be separated from product failures.

Common shapes in this class include:

- Docker or MariaDB not running.
- Stale local server binaries after code changes.
- Reference PHP or Nix cache issues during validation.
- Local request timeouts while a VM failure is still being diagnosed.

These should be reported as environment or validation blockers, not fixed by
changing PHP semantics.
