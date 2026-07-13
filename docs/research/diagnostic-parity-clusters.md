# Diagnostic-Parity Clusters of the Full PHPT Baseline

Date: 2026-07-12.

Reference target: PHP 8.5.7 (`php-8.5.7`).

Measurement evidence for the known-gaps reduction campaign: the 12,592 FAIL
outcomes of full baseline run `20260712T101615Z` (corpus 21,548; PASS 5,668;
SKIP 3,137; XFAIL 8; BORK 143) clustered by the *actual-side* failure
signature — the phrust diagnostic ID in the runner's failure detail, falling
back to the normalized actual-output prefix. Expected-side clustering was tried
first and is too flat to rank (largest family: 26 tests); the actual-side
signature is what ranks the levers.

Method: parse `detail` fields from the run's `results.jsonl`
(`target/phpt-work/full-runs/<ts>/results.jsonl`), extract `E_PHP_*` IDs, else
normalize paths/digits out of the `actual=` excerpt. Counts are per failing
test. This is a point-in-time measurement; re-derive it from the current run
before acting on the numbers.

## Ranked clusters (top of 12,592 FAILs)

| Tests | Signature | Reading |
| ---: | --- | --- |
| 1,744 | `E_PHP_RUNTIME_UNDEFINED_FUNCTION` | Missing builtins, not message format: only 10 of these tests assert the message itself. Top missing: `mktime` 304, `stream_set_blocking` 144, `imagecreate` 124, `stream_socket_server` 112, `openssl_csr_new` 108, `fscanf` 98, `fgetcsv` 74, `srand` 72, `settype` 64, `set_include_path` 54, `gmmktime` 52. Secondary defect: the diagnostic is an uncatchable fatal where the reference throws catchable `Error` with `Call to undefined function {name}()`. |
| 1,093 | `E_PHP_VM_UNKNOWN_CLASS` | Missing extension/class surfaces (message names vary; see module plans). |
| 894 | `E_PHP_VM_UNKNOWN_METHOD` | Concentrated in partial extensions: `SoapClient` (~330 across test methods), `Phar::offsetSet/setStub/buildFromIterator/addFromString` (~250), `DOMDocument::loadHTML/create*NS` (~120), `DateTime::createFromFormat` 48. |
| 709 | empty actual output | Target produced nothing; needs sub-triage (early fatal swallowed vs. missing output path). |
| 370 | `E_PHP_RUNTIME_UNDEFINED_CONSTANT` | Top missing: `STREAM_SERVER_BIND` 122, `LIBXML_NOERROR` 86, `SOCK_DGRAM` 34, `EXTR_REFS` 22, `IMG_ARC_PIE` 16 — constants are cheap registrations where the owning extension is in scope. |
| 326 | `E_PHP_IR_UNSUPPORTED_HIR_STATEMENT` | IR lowering gaps (references/complex assignment shapes). |
| 156 | `ReflectionClass::newLazyGhost`/`newLazyProxy` undefined | PHP 8.4+ lazy-object reflection surface missing. |
| 90 | `E_PHP_VM_UNSUPPORTED_FFI` | FFI out of scope per extension policy. |
| 74 | `E_PHP_RUNTIME_SOAP_HTTP_DISABLED` | Consistent with the soap permanent-gap policy row. |
| 68 | `E_PHP_VM_XML_ARITY` | XML builtin arity drift — mechanical arginfo comparison work. |
| 63 | `E_PHP_VM_DATETIME_PARSE` | Date-format parser gaps. |
| 44 | `PHPT_TIMEOUT` | Hangs; each is a bug worth an issue regardless of policy. |

## zend.basic drill-down (Phase 0 of the module campaign)

The 2,432 zend.basic FAILs split into 966 with a phrust diagnostic ID (feature
missing/aborted) and 1,466 pure output diffs. Pairwise (expected, actual)
line-shape clustering is too flat there (1,031 shapes); classifying by *failure
mode* is not:

| Tests | Mode | Concentrated in |
| ---: | --- | --- |
| 330 | other output divergence | type_declarations 53, inheritance 19, gc 17 |
| 300 | missing `Fatal error` | type_declarations 92, attributes 37, errmsg 21 — compile-/runtime-time validations the reference enforces and phrust does not |
| 258 | value divergence, same output shape | generators 33, type_declarations 24, gc 20 — semantic bugs, not diagnostics |
| 176 | phrust diagnostic leak (non-`E_PHP_*` text aborts output) | attributes 56, property_hooks 56 |
| 128 | spurious `Fatal error` (phrust rejects valid code) | type_declarations 39, led by asymmetric_visibility |
| 64 | diagnostic wording differs | mechanical message-format parity |
| 43 | missing `Parse error` | heredoc/nowdoc indentation rules 16, group_use trailing comma 8 |
| 39 | truncated/empty actual output | needs per-test minimization |
| 36+23 | missing `Warning`/`Deprecated` | `#[NoDiscard]` and `#[Deprecated]` attribute semantics |
| 32+2 | spurious `Warning`/`Deprecated` | over-eager emission |
| 17 | internal panic or stack overflow | hard bugs regardless of parity |
| 17 | timeout | hangs; each warrants a fixture |

Reading: the type-system block (Phase 1.1) carries three modes at once
(missing-fatal 92 + spurious-fatal 39 + divergence 77 in type_declarations
alone); attributes/property_hooks failures are dominated by the diagnostic-leak
mode, i.e. missing runtime support surfacing as raw diagnostic text; and 34
tests (panics + timeouts) are unconditional bugs.

### Normalized first-diff-line shapes (pure output diffs)

The same 1,466 diffs keyed by the normalized first differing line on both
sides (paths → `%FILE%`, digits → `N`, quoted text → `"%S%"`); shapes with a
same-shape pair are value divergences, shapes with an empty or path-prefixed
actual side are missing diagnostics or diagnostic leaks:

| Tests | Expected first diff line | Actual first diff line |
| ---: | --- | --- |
| 61 | `int(N)` | `int(N)` |
| 25 | `string(N) "%S%"` | `string(N) "%S%"` |
| 14 | `Fatal error: Uncaught Error: Undefined constant "%S%" …` | (empty) |
| 13 | `array(N) {` | diagnostic leak (path prefix) |
| 10 | `array(N) {` | `array(N) {` |
| 10 | `bool(true)` | `bool(false)` |
| 10 | `N` | `N` |
| 10 | `Parse error: syntax error, unexpected identifier "%S%" …` | diagnostic leak |
| 9 | `bool(false)` | `bool(true)` |
| 9 | `C::__destruct` | `C::__destruct` |
| 8 | `int(N)` | diagnostic leak |
| 7 | `Parse error: … unexpected token "%S%", expecting "%S%" …` | (empty) |
| 7 | `Parse error: Invalid indentation - tabs and spaces …` | wrong output |

The long tail is flat (1,031 distinct shapes) — per-shape fixes below this
table are not leveraged; the failure-mode classes above are the working axis.

## How this feeds the closure loop

The catalog rows and module plans (`tests/phpt/manifests/…`) stay the source of
truth for accepted gaps; this note only ranks where the next fixtures should be
minimized from. Per `docs/phpt/known-gaps.md`, fixes then follow the owning
layer: builtin registrations in `php_runtime`/`php_std` from arginfo, constants
via the generated extension surfaces, VM diagnostics parity in `php_vm`, IR
lowering in `php_ir`. Engine changes that shift baseline counts are separate,
explicitly approved work — never bundled into a bookkeeping refresh.
