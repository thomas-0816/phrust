<?php
// oracle-probe: id=oracle-builtin-contract-function-error-clear-last-7830770be9 area=builtin_contract kind=function symbol=error_clear_last source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-error-clear-last-7830770be9 failure_category=builtin_contract
$name = "error_clear_last";
echo function_exists($name) ? "available\n" : "missing\n";
