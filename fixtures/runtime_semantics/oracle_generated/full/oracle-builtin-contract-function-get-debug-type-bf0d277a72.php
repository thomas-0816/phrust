<?php
// oracle-probe: id=oracle-builtin-contract-function-get-debug-type-bf0d277a72 area=builtin_contract kind=function symbol=get_debug_type source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-get-debug-type-bf0d277a72 failure_category=builtin_contract
$name = "get_debug_type";
echo function_exists($name) ? "available\n" : "missing\n";
