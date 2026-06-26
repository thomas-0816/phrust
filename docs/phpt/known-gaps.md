# PHPT Known Gaps

Generated from baseline `20260624T210848Z` with 20428 known non-green fingerprints. This catalog is the stable owner map for PHPT failures that are accepted in the committed full baseline.

Each row carries the hard-rule fields required for a known gap: ID, reference behavior, current Rust behavior, fixture or PHPT example, and planned solution layer.

| ID | Baseline count | Reference behavior | Current Rust behavior | Fixture or PHPT example | Planned solution layer |
| --- | ---: | --- | --- | --- | --- |
| `runtime-error-or-diagnostic` | 11402 | PHP emits the exact warning, notice, fatal, stack, and exit behavior expected by the PHPT oracle. | The target exits or formats diagnostics differently from PHP for this baseline fingerprint. | `Zend/tests/67468.phpt` | php_runtime/php_vm diagnostics and error channel |
| `runtime-output-mismatch` | 2315 | PHP stdout and stderr match the PHPT expectation after normal EXPECT/EXPECTF/EXPECTREGEX handling. | The target completes but emits different observable output. | `Zend/tests/access_modifiers/access_modifiers_012.phpt` | php_runtime builtins, php_vm execution semantics, or output buffering |
| `runtime-unsupported-feature` | 6185 | PHP executes the language or builtin feature covered by the PHPT. | The runtime or VM reports an unsupported/not-implemented diagnostic. | `Zend/tests/ArrayAccess/ArrayAccess_indirect_append.phpt` | php_ir/php_runtime/php_vm feature implementation |
| `frontend-parse-or-compile` | 187 | PHP accepts the source or reports the same syntax/compile-time diagnostic as the PHPT expects. | The lexer, parser, semantic frontend, or IR lowering rejects or lowers the source differently. | `Zend/tests/backtrace/fatal_error_backtraces_001.phpt` | php_syntax/php_ast/php_semantics/php_ir |
| `runtime-timeout` | 19 | PHP completes the PHPT within the runner timeout or skips it deterministically. | The target exceeds the PHPT timeout budget. | `Zend/tests/assert/expect_015.phpt` | php_vm control flow, termination, or performance |
| `phpt-runner-section` | 0 | PHP run-tests handles the section and passes the transformed test to the target correctly. | The PHPT runner marks the test BORK because this section is not yet supported. | `ext/standard/tests/file/file_variation.phpt` | php_phpt_tools runner section handling |
| `needs-triage` | 320 | PHP behavior is known through the PHPT oracle but the owning failure class is not yet specific enough. | The fingerprint is retained as known non-green until a narrower owner and implementation path is assigned. | `Zend/tests/multibyte/multibyte_encoding_001.phpt` | PHPT triage and module ownership |
| `unsupported-section` | 21 | run-tests.php understands the section and prepares the target invocation accordingly. | The local PHPT runner BORKs because the section is unsupported. | `ext/standard/tests/basic/bug.phpt` | php_phpt_tools runner section handling |
| `missing-target-cli-capability` | 96 | The upstream target supports CLI/SAPI-specific invocation required by the PHPT. | The current target mode cannot emulate phpdbg, CGI, or another required SAPI capability. | `sapi/phpdbg/tests/print_001.phpt` | target CLI/SAPI policy or explicit extension policy |
| `unsupported-file-external` | 6 | run-tests.php loads the external FILE payload and executes it as the test script. | The runner marks the PHPT BORK because safe FILE_EXTERNAL support is not complete. | `ext/standard/tests/file/bug45181.phpt` | php_phpt_tools runner file materialization |
| `unsupported-expectation` | 10 | run-tests.php compares output with the declared expectation section. | The runner BORKs because this expectation form is not yet supported or normalized. | `ext/standard/tests/general_functions/bug.phpt` | php_phpt_tools expectation matcher |
| `unsupported-runner-io` | 1 | run-tests.php passes ARGS, STDIN, ENV, INI, CLEAN, or related IO setup to the target. | The local runner cannot yet reproduce that setup for this PHPT. | `ext/standard/tests/streams/bug.phpt` | php_phpt_tools runner environment and process setup |
| `malformed-or-non-utf8-phpt` | 313 | run-tests.php either parses the PHPT with PHP's file handling or reports a deterministic BORK. | The local runner classifies the PHPT as malformed or lossy/non-UTF8 input. | `tests/phpt/manifests/full-known-failures.jsonl` | php_phpt_tools parser and source decoding |
| `malformed-or-incomplete-phpt` | 0 | run-tests.php reports malformed PHPT structure consistently. | The local runner classifies missing required sections as BORK. | `tests/phpt/manifests/full-known-failures.jsonl` | php_phpt_tools PHPT parser diagnostics |
| `unknown-bork` | 0 | run-tests.php gives a concrete reason why the PHPT cannot be executed. | The local baseline retained a BORK without a more specific subclass. | `tests/phpt/manifests/full-baseline-module-counts.jsonl` | PHPT triage subclass refinement |
| `other-bork` | 8 | run-tests.php gives a concrete reason why the PHPT cannot be executed. | The local baseline groups a low-volume BORK outside the named subclasses. | `tests/phpt/manifests/full-baseline-module-counts.jsonl` | PHPT triage subclass refinement |

## Invariants

- `tests/phpt/manifests/known-gap-catalog.jsonl` is the machine-readable form of this catalog.
- `just phpt-verify-baseline` rejects a known failure whose `primary_missing_feature_guess` is missing here.
- BORK subclasses from `full-baseline-module-counts.jsonl` must also have catalog rows.
- The catalog documents accepted baseline gaps only; it does not make new failures acceptable without `PHPT_ACCEPT_BASELINE=1`.
