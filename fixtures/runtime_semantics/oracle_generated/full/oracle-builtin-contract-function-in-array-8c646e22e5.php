<?php
// oracle-probe: id=oracle-builtin-contract-function-in-array-8c646e22e5 area=builtin_contract kind=function symbol=in_array source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-in-array-8c646e22e5 failure_category=builtin_contract
$name = "in_array";
echo function_exists($name) ? "available\n" : "missing\n";
