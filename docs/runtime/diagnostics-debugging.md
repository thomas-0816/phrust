# Diagnostics and Debugging

Phrust diagnostics use a shared structured envelope for internal errors,
debug-only events, and future machine-readable CLI/server output. The envelope
is additive: default PHP-visible stdout, stderr, and exit behavior remains
compatibility-preserving unless a caller explicitly selects a diagnostic/debug
format.

## Envelope

`php_diagnostics` owns the stable low-level schema used by higher layers:

```json
{"kind":"diagnostic","schema_version":1,"code":"E_PHRUST_EXAMPLE","layer":"parser","phase":"parse","severity":"error","message":"unexpected token","location":{"path":"example.php","line":2,"column":5,"span":{"start":12,"end":13}},"context":{"expected":"identifier"},"php_visible":false,"request_id":null,"trace_id":null}
```

The byte span is authoritative. Line and column values are one-based display
coordinates derived from `php_source::SourceText` helpers and may be omitted
when source text is unavailable.

Stable fields:

- `kind`: `diagnostic` or `debug_event`.
- `schema_version`: current value is `1`.
- `code`: stable Phrust diagnostic or debug code.
- `legacy_id`: optional compatibility ID from an existing subsystem.
- `layer`: owning subsystem such as `lexer`, `parser`, `semantic`, `ir`,
  `optimizer`, `runtime`, `builtin`, `vm`, `executor`, `cli`, `server`, or
  `infrastructure`.
- `phase`: stable phase name inside the layer.
- `severity`: `error`, `warning`, `notice`, `deprecation`, `note`,
  `recoverable_error`, `fatal_error`, `unsupported_feature`, `info`, or
  `debug`.
- `location`, `labels`, `notes`, `suggestion`, `context`, and `cause`: optional
  supporting metadata.
- `php_visible`: whether the message is intended to mirror a PHP-visible
  diagnostic.
- `request_id` and `trace_id`: optional correlation IDs.

Context maps use deterministic key ordering. Secret-bearing context values are
redacted before rendering.

## Rendering

The shared crate supports two output formats:

- `json`: compact JSON objects, one object per line for streams.
- `text`: one deterministic line with stable fields, suitable for humans and
  grep-friendly logs.

Example text diagnostic:

```text
E_PHRUST_EXAMPLE layer=parser phase=parse severity=error path=example.php line=2 col=5 span=12..13 expected=identifier: unexpected token
```

Text output is a debugging format, not a PHP compatibility surface. PHP-visible
runtime output should continue to use the existing compatibility renderer unless
the caller explicitly asks for structured diagnostics.

## Debug Events

Debug mode is disabled by default. When enabled by a caller, debug events use
the same layer, phase, location, request, trace, and context conventions as
diagnostic envelopes.

Example JSON debug event:

```json
{"kind":"debug_event","schema_version":1,"code":"D_PHRUST_REQUEST","layer":"server","phase":"request","message":"request handled","duration_ms":3,"request_id":"req-1","trace_id":"trace-1","context":{"method":"GET","path":"/"}}
```

`DebugSink` can write events to stderr or append to a file path. Disabled sinks
return immediately and are suitable for hot paths where debug mode is usually
off.

## Enabling Debug Mode

`php-vm run` exposes native execution telemetry without enabling a second
debug executor:

```bash
php-vm run --timings-json target/diagnostics/php-vm-timings.json \
  --counters-json target/diagnostics/php-vm-counters.json path/to/script.php
php-vm run --trace --trace-runtime --trace-includes path/to/script.php
```

`phrust-php` keeps its public PHP-like flags stable, so debug mode is selected
with environment variables:

```bash
PHRUST_DEBUG=1 PHRUST_ERROR_FORMAT=json phrust-php -r 'echo "ok\n";'
PHRUST_DEBUG=1 PHRUST_DEBUG_LOG=target/diagnostics/phrust-php.jsonl phrust-php script.php
```

`phrust-server` accepts matching server flags and environment variables:

```bash
phrust-server --docroot public --debug --error-format json
phrust-server --docroot public --debug --debug-log target/diagnostics/server.jsonl
```

Server environment variables are `PHRUST_SERVER_DEBUG=1`,
`PHRUST_SERVER_ERROR_FORMAT=text|json`, and `PHRUST_SERVER_DEBUG_LOG=<path>`.
Server debug events include deterministic request IDs such as
`req-00000001` and cover request acceptance, route resolution, body handling,
script/cache decisions, PHP execution, and response status. Secret-bearing
headers and context values are redacted before rendering.

Debug output always goes to stderr or the selected debug log file. It must not
be mixed into PHP stdout or HTTP response bodies.

## Smoke Gates

The diagnostics smoke gates are deterministic and do not require a php-src
reference checkout:

```bash
just diagnostics-smoke
just debug-smoke
```

`diagnostics-smoke` exercises CLI usage, source-read, parser/semantic,
IR/runtime/include, and server configuration failures. It validates JSON lines
with `json.loads`, checks required fields, and rejects vague user-facing text
such as raw `Debug` enum output or `called Result::unwrap()`.

`debug-smoke` checks `php-vm` native timing/counter telemetry,
`PHRUST_DEBUG=1 phrust-php`, and `phrust-server --debug` request events. The
smoke asserts that PHP stdout remains exact while telemetry is emitted outside
stdout.

## Redaction

Redaction is case-insensitive and applies before JSON or text rendering. The
following keys are always redacted:

- `authorization`
- `cookie`
- `set-cookie`
- `x-phrust-metrics-token`

Keys containing `password`, `token`, or `secret` are also redacted, including
compound names such as `db_password`, `apiToken`, and `client_secret`.

Redacted values render as `[redacted]`.

## Frontend Diagnostics

Lexer, parser, and semantic diagnostics can be converted into the shared
envelope without changing their existing public structs or compatibility IDs.

Lexer example:

```json
{"kind":"diagnostic","schema_version":1,"code":"E_PHP_LEXER_BAD_CHARACTER","legacy_id":"bad_character","layer":"lexer","phase":"scan","severity":"error","message":"bad control character in scripting mode","location":{"path":"bad.php","line":1,"column":7,"span":{"start":6,"end":7}},"suggestion":"remove the control character or escape it in a string","context":{"line":"1","scanner_mode":"scripting"},"php_visible":true,"request_id":null,"trace_id":null}
```

Parser example:

```text
E_PHP_PARSE_EXPECTED_EXPRESSION layer=parser phase=parse severity=error path=echo.php source_id=envelope-source line=1 col=12 span=11..12 expected=expression: expected expression; suggestion=insert a valid expression
```

Semantic example:

```json
{"kind":"diagnostic","schema_version":1,"code":"E_PHP_DUPLICATE_PARAMETER","legacy_id":"E_PHP_DUPLICATE_PARAMETER","layer":"semantic","phase":"hir_lowering","severity":"error","message":"duplicate parameter name `$x`","location":{"path":"sem.php","source_id":"sem-source","line":1,"column":23,"span":{"start":22,"end":24}},"labels":[{"message":"previous parameter is here","span":{"start":18,"end":20}}],"notes":["parameter names are unique within a function signature"],"php_visible":true,"request_id":null,"trace_id":null}
```

## Compatibility Promise

Structured diagnostics do not change normal PHP-compatible output by default.
CLI and server callers may add explicit debug/error-format options in higher
layers, but normal mode should preserve existing stdout, stderr, and exit
status behavior unless a prompt or ADR explicitly changes that contract.

## Audit Policy

`just diagnostics-audit` runs `scripts/diagnostics/audit.py`. The audit scans
CLI, executor, server, VM include, and runtime builtin boundaries for patterns
that often produce vague or unparsable diagnostics:

- direct `eprintln!` outside approved top-level renderers.
- boundary `Err(format!(...))` or `Err("...".to_string())` without conversion.
- `map_err(|error| error.to_string())` crossing public boundaries.
- `unwrap()`, `expect(...)`, or `panic!` in user-influenced paths.
- raw `{:?}` formatting of public errors.

Allowed invariants must explain the invariant in the message or use a nearby
suppression comment:

```rust
// phrust-diagnostics-allow: invariant response builder rejects only invalid static status codes
let response = builder.body(body).expect("response builder is valid");
```

The script prints every suppression as `[allow] ...` so suppressed cases remain
visible in CI logs. Existing CLI String-returning internals have a pinned legacy
baseline with TODO reasons; the audit fails if that baseline grows. When the
count decreases, update the baseline downward in the same change.
