<?php
// oracle-probe: id=oracle-builtin-contract-function-array-unique-23bf9f3e89 area=builtin_contract kind=function symbol=array_unique source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-array-unique-23bf9f3e89 failure_category=builtin_contract
$name = "array_unique";
echo function_exists($name) ? "available\n" : "missing\n";
