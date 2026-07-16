<?php
// oracle-probe: id=oracle-builtin-contract-function-array-is-list-ca80c52158 area=builtin_contract kind=function symbol=array_is_list source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-array-is-list-ca80c52158 failure_category=builtin_contract
$name = "array_is_list";
echo function_exists($name) ? "available\n" : "missing\n";
