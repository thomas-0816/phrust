# zend.functions

- Priority: 9
- Selected manifest: `tests/phpt/manifests/modules/zend.functions.selected.jsonl`
- Current counts: 85 PASS, 53 SKIP, 727 FAIL, 0 BORK from 887 corpus candidates

## Scope

- user functions
- closures
- callables
- arity
- type coercion

## Non-Scope

- Reflection API surface

## Relevant PHPT Paths

- `tests/output/ob_start_callback_output/functions_that_output_nested.phpt`
- `tests/output/ob_start_callback_output/functions_that_output.phpt`
- `ext/zend_test/tests/variadic_arguments.phpt`
- `ext/zend_test/tests/observer_zend_call_function_01.phpt`
- `ext/zend_test/tests/observer_sqlite_create_function.phpt`
- `ext/zend_test/tests/observer_fiber_functions_03.phpt`
- `ext/zend_test/tests/observer_fiber_functions_02.phpt`
- `ext/zend_test/tests/observer_fiber_functions_01.phpt`
- `ext/zend_test/tests/observer_closure_03.phpt`
- `ext/zend_test/tests/observer_closure_02.phpt`
- `ext/zend_test/tests/observer_closure_01.phpt`
- `ext/zend_test/tests/get_function_or_method_name_01.phpt`
- `ext/xsl/tests/xsltprocessor_registerPHPFunctions-string.phpt`
- `ext/xsl/tests/xsltprocessor_registerPHPFunctions-string-notallowed.phpt`
- `ext/xsl/tests/xsltprocessor_registerPHPFunctions-string-multiple.phpt`
- `ext/xsl/tests/xsltprocessor_registerPHPFunctions-null.phpt`
- `ext/xsl/tests/xsltprocessor_registerPHPFunctions-funcundef.phpt`
- `ext/xsl/tests/xsltprocessor_registerPHPFunctions-funcnostring.phpt`
- `ext/xsl/tests/xsltprocessor_registerPHPFunctions-array.phpt`
- `ext/xsl/tests/xsltprocessor_registerPHPFunctions-array-notallowed.phpt`
- `ext/xsl/tests/xsltprocessor_registerPHPFunctions-array-multiple.phpt`
- `ext/xsl/tests/xsltprocessor_registerPHPFunctions-allfuncs.phpt`
- `ext/xsl/tests/xsltprocessor_exsl_registerPhpFunctionNs.phpt`
- `ext/xsl/tests/registerPHPFunctionNS_errors.phpt`
- `ext/xsl/tests/registerPHPFunctionNS.phpt`
- `ext/xsl/tests/php_function_edge_cases.phpt`
- `ext/xsl/tests/XSLTProcessor_callables_errors.phpt`
- `ext/xsl/tests/XSLTProcessor_callables.phpt`
- `ext/xml/tests/xml_closures_001.phpt`
- `ext/standard/tests/general_functions/var_export_error3.phpt`
- `ext/standard/tests/general_functions/var_export_error2.phpt`
- `ext/standard/tests/general_functions/var_export_basic9.phpt`
- `ext/standard/tests/general_functions/var_export_basic8.phpt`
- `ext/standard/tests/general_functions/var_export_basic7.phpt`
- `ext/standard/tests/general_functions/var_export_basic6.phpt`
- `ext/standard/tests/general_functions/var_export_basic5.phpt`
- `ext/standard/tests/general_functions/var_export_basic4.phpt`
- `ext/standard/tests/general_functions/var_export_basic3.phpt`
- `ext/standard/tests/general_functions/var_export_basic2.phpt`
- `ext/standard/tests/general_functions/var_export_basic1_32.phpt`

## Relevant php-src Source Areas

- `crates/php_semantics/`
- `crates/php_runtime/`
- `crates/php_vm/`

## Target Gates

- `nix develop -c just phpt-module MODULE=zend.functions`

## Known Gaps

- `runtime-error-or-diagnostic`: 475
- `runtime-unsupported-feature`: 238
- `runtime-output-mismatch`: 91
- `frontend-parse-or-compile`: 11

## Next Step

Use generated arginfo for builtin arity and parameter metadata.
