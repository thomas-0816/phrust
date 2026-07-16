<?php
// oracle-probe: id=oracle-builtin-contract-function-error-get-last-03c8196344 area=builtin_contract kind=function symbol=error_get_last source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-error-get-last-03c8196344 failure_category=builtin_contract
$name = "error_get_last";
echo function_exists($name) ? "available\n" : "missing\n";
