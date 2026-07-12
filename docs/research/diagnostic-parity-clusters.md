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

## How this feeds the closure loop

The catalog rows and module plans (`tests/phpt/manifests/…`) stay the source of
truth for accepted gaps; this note only ranks where the next fixtures should be
minimized from. Per `docs/phpt/known-gaps.md`, fixes then follow the owning
layer: builtin registrations in `php_runtime`/`php_std` from arginfo, constants
via the generated extension surfaces, VM diagnostics parity in `php_vm`, IR
lowering in `php_ir`. Engine changes that shift baseline counts are separate,
explicitly approved work — never bundled into a bookkeeping refresh.
