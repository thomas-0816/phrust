<?php
// oracle-probe: id=oracle-builtin-contract-function-range-a4ebfc28a6 area=builtin_contract kind=function symbol=range source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-range-a4ebfc28a6 failure_category=builtin_contract
$name = "range";
echo function_exists($name) ? "available\n" : "missing\n";
